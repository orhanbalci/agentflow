/// Result type for resampler operations
pub type ResamplerResult<T> = Result<T, Box<dyn std::error::Error + Send + Sync>>;

/// Base trait for audio resamplers.
///
/// This trait defines the interface for audio resamplers that can convert
/// audio data from one sample rate to another while maintaining audio quality.
#[async_trait::async_trait]
pub trait BaseAudioResampler: Send + Sync {
    /// Resample audio data from input sample rate to output sample rate.
    ///
    /// # Arguments
    /// * `audio` - Input audio data as raw bytes (16-bit signed integers).
    /// * `in_rate` - Original sample rate in Hz.
    /// * `out_rate` - Target sample rate in Hz.
    ///
    /// # Returns
    /// Resampled audio data as raw bytes (16-bit signed integers).
    ///
    /// # Errors
    /// Returns an error if the resampling operation fails.
    async fn resample(
        &self,
        audio: Vec<u8>,
        in_rate: u32,
        out_rate: u32,
    ) -> ResamplerResult<Vec<u8>>;
}
