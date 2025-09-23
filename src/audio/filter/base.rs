//! Base audio filter trait for input transport audio processing.
//!
//! This module provides the base trait for implementing audio filters
//! that process audio data before VAD and downstream processing in input transports.

use crate::frames::FilterControlFrame;

/// Result type for filter operations
pub type FilterResult<T> = Result<T, Box<dyn std::error::Error + Send + Sync>>;

/// Base trait for input transport audio filters.
///
/// This is a base trait for input transport audio filters. If an audio
/// filter is provided to the input transport it will be used to process audio
/// before VAD and before pushing it downstream. There are control frames to
/// update filter settings or to enable or disable the filter at runtime.
#[async_trait::async_trait]
pub trait BaseAudioFilter: Send + Sync + std::fmt::Debug {
    /// Initialize the filter when the input transport starts.
    ///
    /// This will be called from the input transport when the transport is
    /// started. It can be used to initialize the filter. The input transport
    /// sample rate is provided so the filter can adjust to that sample rate.
    ///
    /// # Arguments
    /// * `sample_rate` - The sample rate of the input transport in Hz.
    ///
    /// # Errors
    /// Returns an error if the filter cannot be initialized with the given sample rate.
    async fn start(&mut self, sample_rate: u32) -> FilterResult<()>;

    /// Clean up the filter when the input transport stops.
    ///
    /// This will be called from the input transport when the transport is
    /// stopping. Implementations should clean up any resources and stop any
    /// background processes.
    ///
    /// # Errors
    /// Returns an error if cleanup fails, though this is typically logged rather
    /// than propagated.
    async fn stop(&mut self) -> FilterResult<()>;

    /// Process control frames for runtime filter configuration.
    ///
    /// This will be called when the input transport receives a
    /// FilterControlFrame. The implementation should handle the control frame
    /// and update the filter's state accordingly (e.g., enable/disable, change
    /// parameters, update settings).
    ///
    /// # Arguments
    /// * `frame` - The control frame containing filter commands or settings.
    ///
    /// # Errors
    /// Returns an error if the control frame cannot be processed.
    async fn process_frame(&mut self, frame: &FilterControlFrame) -> FilterResult<()>;

    /// Apply the audio filter to the provided audio data.
    ///
    /// This is called with raw audio data that needs to be filtered before
    /// VAD processing and downstream propagation. The implementation should
    /// apply the configured filter algorithms and return the processed audio.
    ///
    /// # Arguments
    /// * `audio` - Raw audio data as bytes to be filtered.
    ///
    /// # Returns
    /// Filtered audio data as bytes.
    ///
    /// # Errors
    /// Returns an error if filtering fails or if audio format is incompatible.
    async fn filter(&mut self, audio: &[u8]) -> FilterResult<Vec<u8>>;

    /// Get the current filter configuration.
    ///
    /// Returns the current settings of the filter, including enabled state,
    /// parameters, and any custom settings.
    fn get_config(&self) -> std::collections::HashMap<String, String> {
        std::collections::HashMap::new()
    }

    /// Check if the filter is currently enabled.
    ///
    /// # Returns
    /// `true` if the filter is enabled and should process audio, `false` otherwise.
    fn is_enabled(&self) -> bool {
        true
    }

    /// Get the current filter parameters.
    ///
    /// Returns a map of parameter names to their current values as strings.
    /// This can be used for debugging or UI display purposes.
    fn get_parameters(&self) -> std::collections::HashMap<String, String> {
        std::collections::HashMap::new()
    }
}
