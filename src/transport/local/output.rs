//! Local audio output transport implementation.
//!
//! This module provides a local audio output transport that uses CPAL for real-time
//! audio output through the system's audio devices.

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{Device, Host, Stream, StreamConfig};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio::sync::Mutex;

use crate::task_manager::TaskManager;
use crate::transport::local::params::LocalAudioTransportParams;
use crate::transport::output::BaseOutputTransport;
use crate::{EndFrame, OutputAudioRawFrame, StartFrame};

/// Local audio output transport using CPAL.
///
/// Plays audio frames through the system's audio output device by converting
/// OutputAudioRawFrame objects to playable audio data.
pub struct LocalAudioOutputTransport {
    /// Base output transport implementation
    base: Arc<BaseOutputTransport>,

    /// Local transport parameters
    params: LocalAudioTransportParams,

    /// CPAL host for audio device management
    host: Host,

    /// Output device
    output_device: Option<Device>,

    /// Output stream
    output_stream: Arc<Mutex<Option<Stream>>>,

    /// Stream configuration
    stream_config: Option<StreamConfig>,

    /// Running state
    is_running: Arc<AtomicBool>,

    /// Audio frame receiver for the output stream
    frame_receiver: Arc<Mutex<Option<mpsc::UnboundedReceiver<OutputAudioRawFrame>>>>,

    /// Audio frame sender for pushing frames to the stream
    frame_sender: Arc<Mutex<Option<mpsc::UnboundedSender<OutputAudioRawFrame>>>>,
}

impl LocalAudioOutputTransport {
    /// Initialize the local audio output transport.
    ///
    /// # Arguments
    ///
    /// * `params` - Local transport configuration parameters
    /// * `task_manager` - Task manager for background tasks
    ///
    /// # Returns
    ///
    /// A new `LocalAudioOutputTransport` instance.
    pub fn new(
        params: LocalAudioTransportParams,
        task_manager: Arc<TaskManager>,
    ) -> Result<Self, String> {
        let host = cpal::default_host();

        let base = BaseOutputTransport::new(
            params.base.clone(),
            task_manager,
            Some("LocalAudioOutputTransport".to_string()),
        );

        Ok(Self {
            base,
            params,
            host,
            output_device: None,
            output_stream: Arc::new(Mutex::new(None)),
            stream_config: None,
            is_running: Arc::new(AtomicBool::new(false)),
            frame_receiver: Arc::new(Mutex::new(None)),
            frame_sender: Arc::new(Mutex::new(None)),
        })
    }

    /// Get the output device based on configuration.
    fn get_output_device(&self) -> Result<Device, String> {
        match &self.params.output_device_name {
            Some(device_name) => {
                // Find device by name
                for device in self
                    .host
                    .output_devices()
                    .map_err(|e| format!("Failed to enumerate output devices: {}", e))?
                {
                    if let Ok(name) = device.name() {
                        if name == *device_name {
                            return Ok(device);
                        }
                    }
                }
                Err(format!("Output device '{}' not found", device_name))
            }
            None => {
                // Use default output device
                self.host
                    .default_output_device()
                    .ok_or_else(|| "No default output device available".to_string())
            }
        }
    }

    /// Setup the audio stream configuration.
    fn setup_stream_config(
        &self,
        device: &Device,
        sample_rate: u32,
    ) -> Result<StreamConfig, String> {
        let supported_configs = device
            .supported_output_configs()
            .map_err(|e| format!("Failed to get supported output configs: {}", e))?;

        // Try to find a config that matches our requirements
        for supported_config_range in supported_configs {
            dbg!(supported_config_range);

            if supported_config_range.channels() == self.params.base.audio_out_channels as u16
                && supported_config_range.min_sample_rate().0 <= sample_rate
                && supported_config_range.max_sample_rate().0 >= sample_rate
            {
                let config = StreamConfig {
                    channels: self.params.base.audio_out_channels as u16,
                    sample_rate: cpal::SampleRate(sample_rate),
                    buffer_size: self
                        .params
                        .buffer_size
                        .map(cpal::BufferSize::Fixed)
                        .unwrap_or(cpal::BufferSize::Default),
                };
                return Ok(config);
            }
        }

        Err("No compatible audio output configuration found".to_string())
    }

    /// Start the audio output stream.
    ///
    /// # Arguments
    ///
    /// * `frame` - The start frame containing initialization parameters
    pub async fn start(&mut self, frame: &StartFrame) -> Result<(), String> {
        log::debug!("Starting local audio output transport");

        // Start the base transport
        self.base.start(frame).await.map_err(|e| e.to_string())?;

        if self.is_running.load(Ordering::Relaxed) {
            return Ok(());
        }

        let sample_rate = self
            .params
            .base
            .audio_out_sample_rate
            .unwrap_or(frame.audio_out_sample_rate);

        // Get output device
        let device = self.get_output_device()?;
        self.output_device = Some(device.clone());

        // Setup stream configuration
        let config = self.setup_stream_config(&device, sample_rate)?;
        self.stream_config = Some(config.clone());

        // Create frame channel
        let (tx, rx) = mpsc::unbounded_channel::<OutputAudioRawFrame>();
        *self.frame_sender.lock().await = Some(tx);
        *self.frame_receiver.lock().await = Some(rx);

        // Setup the output stream callback
        let frame_receiver = Arc::clone(&self.frame_receiver);
        let is_running = Arc::clone(&self.is_running);

        let error_callback = |err| {
            log::error!("Audio output stream error: {}", err);
        };

        let data_callback = move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
            if !is_running.load(Ordering::Relaxed) {
                // Fill with silence
                for sample in data.iter_mut() {
                    *sample = 0.0;
                }
                return;
            }

            // Try to get a frame from the receiver (non-blocking)
            let mut frame_guard = match frame_receiver.try_lock() {
                Ok(guard) => guard,
                Err(_) => {
                    // Fill with silence if we can't get the lock
                    for sample in data.iter_mut() {
                        *sample = 0.0;
                    }
                    return;
                }
            };

            if let Some(ref mut receiver) = *frame_guard {
                match receiver.try_recv() {
                    Ok(audio_frame) => {
                        // Convert audio frame data to f32 samples
                        let bytes_per_sample = 2; // Assuming 16-bit audio
                        let num_samples = audio_frame.audio_frame.audio.len() / bytes_per_sample;
                        let samples_to_copy = std::cmp::min(num_samples, data.len());

                        for i in 0..samples_to_copy {
                            let byte_index = i * bytes_per_sample;
                            if byte_index + 1 < audio_frame.audio_frame.audio.len() {
                                let sample_i16 = i16::from_le_bytes([
                                    audio_frame.audio_frame.audio[byte_index],
                                    audio_frame.audio_frame.audio[byte_index + 1],
                                ]);
                                data[i] = sample_i16 as f32 / i16::MAX as f32;
                            } else {
                                data[i] = 0.0;
                            }
                        }

                        // Fill remaining samples with silence
                        for i in samples_to_copy..data.len() {
                            data[i] = 0.0;
                        }
                    }
                    Err(_) => {
                        // No frame available, fill with silence
                        for sample in data.iter_mut() {
                            *sample = 0.0;
                        }
                    }
                }
            } else {
                // No receiver available, fill with silence
                for sample in data.iter_mut() {
                    *sample = 0.0;
                }
            }
        };

        // Build and start the stream
        let stream = device
            .build_output_stream(&config, data_callback, error_callback, None)
            .map_err(|e| format!("Failed to build output stream: {}", e))?;

        stream
            .play()
            .map_err(|e| format!("Failed to start output stream: {}", e))?;

        *self.output_stream.lock().await = Some(stream);
        self.is_running.store(true, Ordering::Relaxed);

        log::debug!("Local audio output transport started successfully");
        Ok(())
    }

    /// Stop and cleanup the audio output stream.
    pub async fn cleanup(&mut self) -> Result<(), String> {
        log::debug!("Cleaning up local audio output transport");

        self.is_running.store(false, Ordering::Relaxed);

        // Stop and drop the stream
        let mut stream_guard = self.output_stream.lock().await;
        if let Some(stream) = stream_guard.take() {
            drop(stream);
        }

        // Clear the frame channel
        *self.frame_sender.lock().await = None;
        *self.frame_receiver.lock().await = None;

        // Cleanup base transport - need to create an EndFrame
        let end_frame = EndFrame::new();
        self.base
            .stop(&end_frame)
            .await
            .map_err(|e| e.to_string())?;

        log::debug!("Local audio output transport cleaned up successfully");
        Ok(())
    }

    /// Write an audio frame to the output stream.
    ///
    /// # Arguments
    ///
    /// * `frame` - The audio frame to write to the output device
    ///
    /// # Returns
    ///
    /// `true` if the audio frame was queued successfully, `false` otherwise.
    pub async fn write_audio_frame(&self, frame: OutputAudioRawFrame) -> bool {
        if !self.is_running.load(Ordering::Relaxed) {
            return false;
        }

        let sender_guard = self.frame_sender.lock().await;
        if let Some(ref sender) = *sender_guard {
            match sender.send(frame) {
                Ok(_) => {
                    log::debug!("Audio frame queued for output");
                    true
                }
                Err(e) => {
                    log::error!("Failed to queue audio frame: {}", e);
                    false
                }
            }
        } else {
            log::warn!("No frame sender available");
            false
        }
    }

    /// Check if the transport is currently running.
    pub fn is_running(&self) -> bool {
        self.is_running.load(Ordering::Relaxed)
    }
}
