//! Complete local audio transport with input and output capabilities.
//!
//! This module provides a unified interface for local audio I/O using CPAL,
//! supporting both audio capture and playback through the system's audio devices.

use cpal::traits::{DeviceTrait, HostTrait};
use std::sync::Arc;
use tokio::sync::Mutex;

use crate::task_manager::TaskManager;
use crate::transport::local::input::LocalAudioInputTransport;
use crate::transport::local::output::LocalAudioOutputTransport;
use crate::transport::local::params::LocalAudioTransportParams;
use crate::{OutputAudioRawFrame, StartFrame};

/// Complete local audio transport with input and output capabilities.
///
/// Provides a unified interface for local audio I/O using CPAL, supporting
/// both audio capture and playback through the system's audio devices.
pub struct LocalAudioTransport {
    /// Transport configuration parameters
    params: LocalAudioTransportParams,

    /// Task manager for background tasks
    task_manager: Arc<TaskManager>,

    /// Audio input transport
    input_transport: Arc<Mutex<Option<LocalAudioInputTransport>>>,

    /// Audio output transport
    output_transport: Arc<Mutex<Option<LocalAudioOutputTransport>>>,
}

impl LocalAudioTransport {
    /// Initialize the local audio transport.
    ///
    /// # Arguments
    ///
    /// * `params` - Transport configuration parameters
    /// * `task_manager` - Task manager for background tasks
    ///
    /// # Returns
    ///
    /// A new `LocalAudioTransport` instance.
    pub fn new(params: LocalAudioTransportParams, task_manager: Arc<TaskManager>) -> Self {
        Self {
            params,
            task_manager,
            input_transport: Arc::new(Mutex::new(None)),
            output_transport: Arc::new(Mutex::new(None)),
        }
    }

    /// Start both input and output transports.
    ///
    /// # Arguments
    ///
    /// * `frame` - The start frame containing initialization parameters
    pub async fn start(&self, frame: &StartFrame) -> Result<(), String> {
        log::debug!("Starting local audio transport");

        // Start input transport if audio input is enabled
        if self.params.base.audio_in_enabled {
            let mut input_guard = self.input_transport.lock().await;
            if input_guard.is_none() {
                let mut input_transport = LocalAudioInputTransport::new(
                    self.params.clone(),
                    Arc::clone(&self.task_manager),
                )?;
                input_transport.start(frame).await?;
                *input_guard = Some(input_transport);
            }
        }

        // Start output transport if audio output is enabled
        if self.params.base.audio_out_enabled {
            let mut output_guard = self.output_transport.lock().await;
            if output_guard.is_none() {
                let mut output_transport = LocalAudioOutputTransport::new(
                    self.params.clone(),
                    Arc::clone(&self.task_manager),
                )?;
                output_transport.start(frame).await?;
                *output_guard = Some(output_transport);
            }
        }

        log::debug!("Local audio transport started successfully");
        Ok(())
    }

    /// Stop and cleanup both input and output transports.
    pub async fn stop(&self) -> Result<(), String> {
        log::debug!("Stopping local audio transport");

        // Stop input transport
        let mut input_guard = self.input_transport.lock().await;
        if let Some(ref mut input_transport) = *input_guard {
            input_transport.cleanup().await?;
        }
        *input_guard = None;

        // Stop output transport
        let mut output_guard = self.output_transport.lock().await;
        if let Some(ref mut output_transport) = *output_guard {
            output_transport.cleanup().await?;
        }
        *output_guard = None;

        log::debug!("Local audio transport stopped successfully");
        Ok(())
    }

    /// Write an audio frame to the output transport.
    ///
    /// # Arguments
    ///
    /// * `frame` - The audio frame to write to the output device
    ///
    /// # Returns
    ///
    /// `true` if the audio frame was written successfully, `false` otherwise.
    pub async fn write_audio_frame(&self, frame: OutputAudioRawFrame) -> bool {
        let output_guard = self.output_transport.lock().await;
        if let Some(ref output_transport) = *output_guard {
            output_transport.write_audio_frame(frame).await
        } else {
            log::warn!("No output transport available");
            false
        }
    }

    /// Get the current input sample rate.
    ///
    /// # Returns
    ///
    /// The sample rate of the input transport, or 0 if not available.
    pub async fn input_sample_rate(&self) -> u32 {
        let input_guard = self.input_transport.lock().await;
        if let Some(ref input_transport) = *input_guard {
            input_transport.sample_rate()
        } else {
            0
        }
    }

    /// Check if the input transport is running.
    ///
    /// # Returns
    ///
    /// `true` if the input transport is running, `false` otherwise.
    pub async fn is_input_running(&self) -> bool {
        let input_guard = self.input_transport.lock().await;
        if let Some(ref input_transport) = *input_guard {
            input_transport.is_running()
        } else {
            false
        }
    }

    /// Check if the output transport is running.
    ///
    /// # Returns
    ///
    /// `true` if the output transport is running, `false` otherwise.
    pub async fn is_output_running(&self) -> bool {
        let output_guard = self.output_transport.lock().await;
        if let Some(ref output_transport) = *output_guard {
            output_transport.is_running()
        } else {
            false
        }
    }

    /// List available audio input devices.
    ///
    /// # Returns
    ///
    /// A vector of device names, or an error if enumeration fails.
    pub fn list_input_devices() -> Result<Vec<String>, String> {
        let host = cpal::default_host();
        let devices = host
            .input_devices()
            .map_err(|e| format!("Failed to enumerate input devices: {}", e))?;

        let mut device_names = Vec::new();
        for device in devices {
            if let Ok(name) = device.name() {
                device_names.push(name);
            }
        }

        Ok(device_names)
    }

    /// List available audio output devices.
    ///
    /// # Returns
    ///
    /// A vector of device names, or an error if enumeration fails.
    pub fn list_output_devices() -> Result<Vec<String>, String> {
        let host = cpal::default_host();
        let devices = host
            .output_devices()
            .map_err(|e| format!("Failed to enumerate output devices: {}", e))?;

        let mut device_names = Vec::new();
        for device in devices {
            if let Ok(name) = device.name() {
                device_names.push(name);
            }
        }

        Ok(device_names)
    }

    /// Get the default input device name.
    ///
    /// # Returns
    ///
    /// The name of the default input device, or an error if not available.
    pub fn default_input_device_name() -> Result<String, String> {
        let host = cpal::default_host();
        let device = host
            .default_input_device()
            .ok_or_else(|| "No default input device available".to_string())?;

        device
            .name()
            .map_err(|e| format!("Failed to get device name: {}", e))
    }

    /// Get the default output device name.
    ///
    /// # Returns
    ///
    /// The name of the default output device, or an error if not available.
    pub fn default_output_device_name() -> Result<String, String> {
        let host = cpal::default_host();
        let device = host
            .default_output_device()
            .ok_or_else(|| "No default output device available".to_string())?;

        device
            .name()
            .map_err(|e| format!("Failed to get device name: {}", e))
    }
}
