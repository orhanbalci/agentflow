//! High-quality audio resampler implementation.
//!
//! This module provides an audio resampler that uses the Rubato library
//! for very high-quality audio sample rate conversion.
//!
//! When to use the HighQualityAudioResampler:
//! 1. For batch processing of complete audio files
//! 2. When you have all the audio data available at once
//! 3. When you need high-quality resampling without system dependencies

use async_trait::async_trait;
use bytemuck::cast_slice;
use rubato::{FftFixedIn, Resampler};

use super::base::{BaseAudioResampler, ResamplerResult};

/// High-quality audio resampler implementation using the Rubato library.
///
/// This resampler uses the Rubato library configured for high-quality resampling
/// with sinc interpolation, providing excellent audio quality.
pub struct RubatoAudioResampler;

impl RubatoAudioResampler {
    /// Create a new high-quality audio resampler.
    ///
    /// # Returns
    /// A new instance of `HighQualityAudioResampler`.
    ///
    /// # Examples
    /// ```rust
    /// use agentflow::audio::resampler::rubato::RubatoAudioResampler;
    ///
    /// let resampler = RubatoAudioResampler::new();
    /// ```
    pub fn new() -> Self {
        Self
    }
}

impl Default for RubatoAudioResampler {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl BaseAudioResampler for RubatoAudioResampler {
    /// Resample audio data using high-quality sinc interpolation.
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
    /// Returns an error if:
    /// - The input audio data length is not a multiple of 2 (for 16-bit samples)
    /// - The resampler fails to process the audio
    /// - Sample rates are invalid (zero or too high)
    ///
    /// # Examples
    /// ```rust
    /// use agentflow::audio::resampler::{BaseAudioResampler, RubatoAudioResampler};
    ///
    /// #[tokio::main]
    /// async fn main() {
    ///     let resampler = RubatoAudioResampler::new();
    ///     let input_audio = vec![0u8; 1024]; // 16-bit audio data
    ///     match resampler.resample(input_audio, 44100, 22050).await {
    ///         Ok(resampled) => println!("Resampling successful: {} bytes", resampled.len()),
    ///         Err(e) => println!("Resampling failed: {}", e),
    ///     }
    /// }
    /// ```
    async fn resample(
        &self,
        audio: Vec<u8>,
        in_rate: u32,
        out_rate: u32,
    ) -> ResamplerResult<Vec<u8>> {
        // If sample rates are the same, no resampling needed
        if in_rate == out_rate {
            return Ok(audio);
        }

        // Validate sample rates
        if in_rate == 0 || out_rate == 0 {
            return Err("Sample rates must be greater than zero".into());
        }

        // Validate input: audio data must be 16-bit (2 bytes per sample)
        if audio.len() % 2 != 0 {
            return Err("Audio data length must be a multiple of 2 for 16-bit samples".into());
        }

        if audio.is_empty() {
            return Ok(Vec::new());
        }

        // Convert bytes to i16 samples and then to f64 for processing
        let audio_samples: &[i16] = cast_slice(&audio);
        let mut audio_f64: Vec<f64> = audio_samples
            .iter()
            .map(|&s| s as f64 / i16::MAX as f64)
            .collect();

        // Calculate resampling parameters
        let num_samples = audio_f64.len();

        // Create FFT-based resampler for better performance with small inputs
        let chunk_size = num_samples.max(64); // Ensure minimum chunk size for FFT

        // Pad the input to match the required chunk size
        if audio_f64.len() < chunk_size {
            audio_f64.resize(chunk_size, 0.0);
        }

        let mut resampler = FftFixedIn::<f64>::new(
            in_rate as usize,
            out_rate as usize,
            chunk_size,
            2, // Sub chunks
            1, // Single channel (mono)
        )
        .map_err(|e| format!("Failed to create resampler: {}", e))?;

        // Process audio
        let input_data = vec![audio_f64];
        let resampled_data = resampler
            .process(&input_data, None)
            .map_err(|e| format!("Failed to resample audio: {}", e))?;

        // Convert f64 samples back to i16 and then to bytes
        let output_i16: Vec<i16> = resampled_data[0]
            .iter()
            .map(|&s| (s * i16::MAX as f64).clamp(i16::MIN as f64, i16::MAX as f64) as i16)
            .collect();

        let output_bytes: &[u8] = cast_slice(&output_i16);
        Ok(output_bytes.to_vec())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_resampler_same_rate() {
        let resampler = RubatoAudioResampler::new();
        let audio_data = vec![0u8, 1u8, 2u8, 3u8]; // 2 samples of 16-bit audio

        let result = resampler
            .resample(audio_data.clone(), 44100, 44100)
            .await
            .unwrap();
        assert_eq!(result, audio_data);
    }

    #[tokio::test]
    async fn test_resampler_downsample() {
        let resampler = RubatoAudioResampler::new();
        // Create some test audio data (4 samples = 8 bytes)
        let audio_data = vec![0u8, 0u8, 100u8, 0u8, 200u8, 0u8, 50u8, 0u8];

        let result = resampler.resample(audio_data, 44100, 22050).await.unwrap();

        // When downsampling by half, we expect roughly half the samples, but at least some output
        assert!(!result.is_empty());
        assert!(result.len() % 2 == 0); // Must be even for 16-bit samples
    }

    #[tokio::test]
    async fn test_resampler_upsample() {
        let resampler = RubatoAudioResampler::new();
        // Create some test audio data (2 samples = 4 bytes)
        let audio_data = vec![0u8, 0u8, 100u8, 0u8];

        let result = resampler.resample(audio_data, 22050, 44100).await.unwrap();

        // When upsampling by 2x, we expect some output
        assert!(!result.is_empty());
        assert!(result.len() % 2 == 0); // Must be even for 16-bit samples
    }

    #[tokio::test]
    async fn test_resampler_invalid_input() {
        let resampler = RubatoAudioResampler::new();
        // Odd number of bytes (invalid for 16-bit audio)
        let audio_data = vec![0u8, 1u8, 2u8];

        let result = resampler.resample(audio_data, 44100, 22050).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_resampler_zero_sample_rate() {
        let resampler = RubatoAudioResampler::new();
        let audio_data = vec![0u8, 0u8, 100u8, 0u8];

        let result = resampler.resample(audio_data.clone(), 0, 44100).await;
        assert!(result.is_err());

        let result = resampler.resample(audio_data, 44100, 0).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_resampler_empty_input() {
        let resampler = RubatoAudioResampler::new();
        let audio_data = vec![];

        let result = resampler.resample(audio_data, 44100, 22050).await.unwrap();
        assert!(result.is_empty());
    }
}
