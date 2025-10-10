//! Local audio input transport implementation.
//!
//! This module provides a local audio input transport that uses CPAL for real-time
//! audio input through the system's audio devices.

use async_trait::async_trait;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{Device, Host, StreamConfig};
use delegate::delegate;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio::sync::Mutex;

use crate::processors::frame::{
    BaseInterruptionStrategy, FrameCallback, FrameDirection, FrameProcessorMetrics,
    FrameProcessorSetup, FrameProcessorTrait,
};
use crate::task_manager::{TaskHandle, TaskManager};
use crate::transport::input::BaseInputTransport;
use crate::transport::local::params::LocalAudioTransportParams;
use crate::{BaseClock, FrameType, InputAudioRawFrame, StartFrame};

/// Local audio input transport using CPAL.
///
/// Captures audio from the system's audio input device and converts it to
/// InputAudioRawFrame objects for processing in the pipeline.
pub struct LocalAudioInputTransport {
    /// Base input transport implementation
    base: BaseInputTransport,

    /// Local transport parameters
    params: LocalAudioTransportParams,

    /// CPAL host for audio device management
    host: Host,

    /// Output device
    input_device: Option<Device>,

    /// Stream configuration
    stream_config: Option<StreamConfig>,

    /// Running state
    is_running: Arc<AtomicBool>,

    /// Audio frame sender
    frame_sender: Arc<Mutex<Option<mpsc::UnboundedSender<InputAudioRawFrame>>>>,

    /// Stream control sender (to control the stream from a separate task)
    stream_control_sender: Arc<Mutex<Option<mpsc::UnboundedSender<StreamControlCommand>>>>,
}

/// Commands to control the audio stream
#[derive(Debug)]
enum StreamControlCommand {
    Stop,
}

impl LocalAudioInputTransport {
    /// Initialize the local audio input transport.
    ///
    /// # Arguments
    ///
    /// * `params` - Local transport configuration parameters
    /// * `task_manager` - Task manager for background tasks
    ///
    /// # Returns
    ///
    /// A new `LocalAudioInputTransport` instance.
    pub fn new(
        params: LocalAudioTransportParams,
        task_manager: Arc<TaskManager>,
    ) -> Result<Self, String> {
        let host = cpal::default_host();

        let base = BaseInputTransport::new(
            params.base.clone(),
            task_manager,
            Some("LocalAudioInputTransport".to_string()),
        );

        Ok(Self {
            base,
            params,
            host,
            input_device: None,
            stream_config: None,
            is_running: Arc::new(AtomicBool::new(false)),
            frame_sender: Arc::new(Mutex::new(None)),
            stream_control_sender: Arc::new(Mutex::new(None)),
        })
    }

    /// Get the input device based on configuration.
    fn get_input_device(&self) -> Result<Device, String> {
        match &self.params.input_device_name {
            Some(device_name) => {
                // Find device by name
                for device in self
                    .host
                    .input_devices()
                    .map_err(|e| format!("Failed to enumerate input devices: {}", e))?
                {
                    if let Ok(name) = device.name() {
                        if name == *device_name {
                            return Ok(device);
                        }
                    }
                }
                Err(format!("Input device '{}' not found", device_name))
            }
            None => {
                // Use default input device
                self.host
                    .default_input_device()
                    .ok_or_else(|| "No default input device available".to_string())
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
            .supported_input_configs()
            .map_err(|e| format!("Failed to get supported input configs: {}", e))?;

        // Try to find a config that matches our requirements
        for supported_config_range in supported_configs {
            if supported_config_range.channels() == self.params.base.audio_in_channels as u16
                && supported_config_range.min_sample_rate().0 <= sample_rate
                && supported_config_range.max_sample_rate().0 >= sample_rate
            {
                let config = StreamConfig {
                    channels: self.params.base.audio_in_channels as u16,
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

        Err("No compatible audio input configuration found".to_string())
    }

    /// Start the audio input stream.
    ///
    /// # Arguments
    ///
    /// * `frame` - The start frame containing initialization parameters
    pub async fn start(&mut self, frame: &StartFrame) -> Result<(), String> {
        log::debug!("Starting local audio input transport");

        // Start the base transport
        self.base.start(frame).await?;

        if self.is_running.load(Ordering::Relaxed) {
            return Ok(());
        }

        let sample_rate = self
            .params
            .base
            .audio_in_sample_rate
            .unwrap_or(frame.audio_in_sample_rate);

        // Get input device
        let device = self.get_input_device()?;
        self.input_device = Some(device.clone());

        // Setup stream configuration
        let config = self.setup_stream_config(&device, sample_rate)?;
        self.stream_config = Some(config.clone());

        // Create frame sender and stream control
        let (tx, mut rx) = mpsc::unbounded_channel::<InputAudioRawFrame>();
        let (control_tx, mut control_rx) = mpsc::unbounded_channel::<StreamControlCommand>();
        *self.frame_sender.lock().await = Some(tx.clone());
        *self.stream_control_sender.lock().await = Some(control_tx);

        // Setup the input stream callback
        let frame_sender = tx.clone();
        let is_running = Arc::clone(&self.is_running);
        let num_channels = self.params.base.audio_in_channels;

        let error_callback = |err| {
            log::error!("Audio input stream error: {}", err);
        };

        let data_callback = move |data: &[f32], _: &cpal::InputCallbackInfo| {
            if !is_running.load(Ordering::Relaxed) {
                return;
            }

            // Convert f32 samples to i16 (assuming 16-bit audio)
            let mut audio_data = Vec::with_capacity(data.len() * 2);
            for &sample in data {
                let sample_i16 = (sample * i16::MAX as f32) as i16;
                audio_data.extend_from_slice(&sample_i16.to_le_bytes());
            }

            let frame = InputAudioRawFrame::new(audio_data, sample_rate, num_channels);

            if let Err(e) = frame_sender.send(frame) {
                log::error!("Failed to send audio frame: {}", e);
            }
        };

        // Build and start the stream in a separate task to avoid Send/Sync issues
        let device_clone = device.clone();
        let config_clone = config.clone();
        let is_running_clone = Arc::clone(&self.is_running);

        tokio::task::spawn_blocking(move || {
            let stream = device_clone
                .build_input_stream(&config_clone, data_callback, error_callback, None)
                .map_err(|e| format!("Failed to build input stream: {}", e))?;

            stream
                .play()
                .map_err(|e| format!("Failed to start input stream: {}", e))?;

            // Keep the stream alive and listen for control commands
            let rt = tokio::runtime::Handle::current();
            rt.block_on(async {
                loop {
                    tokio::select! {
                        cmd = control_rx.recv() => {
                            match cmd {
                                Some(StreamControlCommand::Stop) | None => {
                                    log::debug!("Stopping audio input stream");
                                    break;
                                }
                            }
                        }
                    }
                }
            });

            drop(stream);
            is_running_clone.store(false, Ordering::Relaxed);
            Ok::<(), String>(())
        });

        self.is_running.store(true, Ordering::Relaxed);

        // Start task to process frames
        tokio::spawn(async move {
            while let Some(frame) = rx.recv().await {
                // Process the frame through the base transport
                // This is a simplified version - you'd need proper error handling
                log::debug!("Received audio frame with {} bytes", frame.audio.len());
            }
        });

        log::debug!("Local audio input transport started successfully");
        Ok(())
    }

    /// Stop and cleanup the audio input stream.
    pub async fn cleanup(&mut self) -> Result<(), String> {
        self.is_running.store(false, Ordering::Relaxed);

        // Stop the stream by sending a control command
        if let Some(sender) = self.stream_control_sender.lock().await.take() {
            if let Err(e) = sender.send(StreamControlCommand::Stop) {
                log::warn!("Failed to send stop command to stream: {}", e);
            }
        }

        // Clear the frame sender
        *self.frame_sender.lock().await = None;

        log::debug!("Audio input transport cleaned up");
        Ok(())
    }

    /// Get the current sample rate.
    pub fn sample_rate(&self) -> u32 {
        self.base.sample_rate()
    }

    /// Check if the transport is currently running.
    pub fn is_running(&self) -> bool {
        self.is_running.load(Ordering::Relaxed)
    }

    async fn _create_task<F, Fut>(
        &self,
        future: F,
        name: Option<String>,
    ) -> Result<TaskHandle, String>
    where
        F: FnOnce(crate::task_manager::TaskContext) -> Fut + Send + 'static,
        Fut: std::future::Future<Output = ()> + Send + 'static,
    {
        self.base.create_task(future, name).await
    }
}

#[async_trait]
impl FrameProcessorTrait for LocalAudioInputTransport {
    // Use delegate macro for all sync methods
    delegate! {
        to self.base {
            fn can_generate_metrics(&self) -> bool;
            fn id(&self) -> u64;
            fn name(&self) -> &str;
            fn is_started(&self) -> bool;
            fn is_cancelling(&self) -> bool;
            fn set_allow_interruptions(&mut self, allow: bool);
            fn set_enable_metrics(&mut self, enable: bool);
            fn set_enable_usage_metrics(&mut self, enable: bool);
            fn set_report_only_initial_ttfb(&mut self, report: bool);
            fn set_clock(&mut self, clock: Arc<dyn BaseClock>);
            fn set_task_manager(&mut self, task_manager: Arc<TaskManager>);
            fn add_processor(&mut self, processor: Arc<Mutex<dyn FrameProcessorTrait>>);
            fn clear_processors(&mut self);
            fn is_compound_processor(&self) -> bool;
            fn processor_count(&self) -> usize;
            fn get_processor(&self, index: usize) -> Option<&Arc<Mutex<dyn FrameProcessorTrait>>>;
            fn processors(&self) -> Vec<Arc<Mutex<dyn FrameProcessorTrait>>>;
            fn link(&mut self, next: Arc<Mutex<dyn FrameProcessorTrait>>);
            fn add_interruption_strategy(&mut self, strategy: Arc<dyn BaseInterruptionStrategy>);
        }
    }

    // Async methods - delegate to base

    async fn cancel_task(
        &self,
        task: &TaskHandle,
        timeout: Option<std::time::Duration>,
    ) -> Result<(), String> {
        self.base.cancel_task(task, timeout).await
    }

    async fn get_metrics(&self) -> FrameProcessorMetrics {
        self.base.get_metrics().await
    }

    async fn setup(&mut self, setup: FrameProcessorSetup) -> Result<(), String> {
        self.base.setup(setup).await
    }

    async fn setup_all_processors(&self, setup: FrameProcessorSetup) -> Result<(), String> {
        self.base.setup_all_processors(setup).await
    }

    // async fn cleanup_all_processors(&self) -> Result<(), String> {
    //     self.base.cleanup_all_processors().await
    // }

    async fn push_frame(&self, frame: FrameType, direction: FrameDirection) -> Result<(), String> {
        self.base.push_frame(frame, direction).await
    }

    async fn push_frame_with_callback(
        &mut self,
        frame: FrameType,
        direction: FrameDirection,
        callback: Option<FrameCallback>,
    ) -> Result<(), String> {
        self.base
            .push_frame_with_callback(frame, direction, callback)
            .await
    }

    async fn queue_frame(
        &mut self,
        frame: FrameType,
        direction: FrameDirection,
        callback: Option<FrameCallback>,
    ) -> Result<(), String> {
        self.base.queue_frame(frame, direction, callback).await
    }

    async fn process_frame(
        &mut self,
        frame: FrameType,
        direction: FrameDirection,
    ) -> Result<(), String> {
        self.base.process_frame(frame, direction).await
    }
}
