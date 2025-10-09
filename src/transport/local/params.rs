//! Local transport configuration parameters.
//!
//! This module defines the configuration parameters specific to local audio transport,
//! extending the base transport parameters with local device-specific settings.

use crate::transport::params::TransportParams;

/// Configuration parameters for local audio transport.
///
/// Extends the base `TransportParams` with local audio device specific settings
/// for input and output device selection.
///
/// # Parameters
///
/// * `input_device_name` - Name of the audio input device to use. If None, uses default.
/// * `output_device_name` - Name of the audio output device to use. If None, uses default.
/// * `buffer_size` - Size of the audio buffer in frames. If None, uses default.
#[derive(Debug, Clone)]
pub struct LocalAudioTransportParams {
    /// Base transport parameters
    pub base: TransportParams,

    /// Input device name. If None, uses the system default input device.
    pub input_device_name: Option<String>,

    /// Output device name. If None, uses the system default output device.
    pub output_device_name: Option<String>,

    /// Audio buffer size in frames. If None, uses a reasonable default.
    pub buffer_size: Option<u32>,
}

impl LocalAudioTransportParams {
    /// Create new local audio transport parameters with default settings.
    ///
    /// # Arguments
    ///
    /// * `base` - Base transport parameters
    ///
    /// # Returns
    ///
    /// A new `LocalAudioTransportParams` instance with default device settings.
    pub fn new(base: TransportParams) -> Self {
        Self {
            base,
            input_device_name: None,
            output_device_name: None,
            buffer_size: None,
        }
    }

    /// Set the input device name.
    ///
    /// # Arguments
    ///
    /// * `device_name` - Name of the input device to use
    pub fn with_input_device(mut self, device_name: String) -> Self {
        self.input_device_name = Some(device_name);
        self
    }

    /// Set the output device name.
    ///
    /// # Arguments
    ///
    /// * `device_name` - Name of the output device to use
    pub fn with_output_device(mut self, device_name: String) -> Self {
        self.output_device_name = Some(device_name);
        self
    }

    /// Set the buffer size.
    ///
    /// # Arguments
    ///
    /// * `buffer_size` - Buffer size in frames
    pub fn with_buffer_size(mut self, buffer_size: u32) -> Self {
        self.buffer_size = Some(buffer_size);
        self
    }
}

impl Default for LocalAudioTransportParams {
    fn default() -> Self {
        Self::new(TransportParams::default())
    }
}
