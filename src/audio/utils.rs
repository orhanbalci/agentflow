//! Audio utility functions.
//!
//! This module provides utility functions for audio processing operations.

use crate::audio::resampler::{BaseAudioResampler, RubatoStreamAudioResampler};

/// Threshold for detecting silence vs. speech in 16-bit PCM audio.
///
/// Normal speech typically produces amplitude values between ±500 to ±5000,
/// depending on factors like loudness and microphone gain. This threshold
/// is set well below typical speech levels to reliably detect silence vs. speech.
const SPEAKING_THRESHOLD: i16 = 20;

/// Create a stream audio resampler instance.
///
/// This function creates a new streaming audio resampler that maintains internal
/// state for efficient processing of audio chunks with the same sample rates.
///
/// # Returns
/// A boxed `BaseAudioResampler` trait object containing a `RubatoStreamAudioResampler`.
///
/// # Examples
/// ```rust
/// use agentflow::audio::utils::create_stream_resampler;
///
/// let resampler = create_stream_resampler();
/// ```
pub fn create_stream_resampler() -> Box<dyn BaseAudioResampler> {
    Box::new(RubatoStreamAudioResampler::new())
}

/// Determine if an audio sample contains silence by checking amplitude levels.
///
/// This function analyzes raw PCM audio data to detect silence by comparing
/// the maximum absolute amplitude against a predefined threshold. The audio
/// is expected to be clean speech or complete silence without background noise.
///
/// # Arguments
/// * `pcm_bytes` - Raw PCM audio data as bytes (16-bit signed integers).
///
/// # Returns
/// `true` if the audio sample is considered silence (below threshold),
/// `false` otherwise.
///
/// # Examples
/// ```rust
/// use agentflow::audio::utils::is_silence;
///
/// let silent_audio = vec![0u8; 1024]; // Silent audio
/// assert!(is_silence(&silent_audio));
///
/// let speaking_audio = vec![255u8, 127u8, 255u8, 127u8]; // Loud audio (32767 amplitude)
/// assert!(!is_silence(&speaking_audio));
/// ```
pub fn is_silence(pcm_bytes: &[u8]) -> bool {
    // Validate input: audio data must be 16-bit (2 bytes per sample)
    if pcm_bytes.len() % 2 != 0 {
        return false; // Invalid input, not silence
    }

    if pcm_bytes.is_empty() {
        return true; // Empty audio is considered silence
    }

    // Convert bytes to i16 samples and find maximum absolute amplitude
    let mut max_amplitude: i16 = 0;

    for chunk in pcm_bytes.chunks_exact(2) {
        let sample = i16::from_le_bytes([chunk[0], chunk[1]]);
        let abs_sample = sample.abs();
        if abs_sample > max_amplitude {
            max_amplitude = abs_sample;
        }
    }

    // If max amplitude is lower than SPEAKING_THRESHOLD, consider it as silence
    max_amplitude <= SPEAKING_THRESHOLD
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_create_stream_resampler() {
        let resampler = create_stream_resampler();
        let audio_data = vec![0u8; 2048]; // 16-bit PCM data

        // Test that the resampler works
        let result = resampler.resample(audio_data, 44100, 22050).await.unwrap();
        assert!(!result.is_empty());
        assert!(result.len() % 2 == 0); // Must be even for 16-bit samples
    }

    #[test]
    fn test_is_silence_empty_audio() {
        let empty_audio = vec![];
        assert!(is_silence(&empty_audio));
    }

    #[test]
    fn test_is_silence_silent_audio() {
        let silent_audio = vec![0u8; 1024]; // All zeros
        assert!(is_silence(&silent_audio));
    }

    #[test]
    fn test_is_silence_below_threshold() {
        // Create audio with amplitude below threshold (10 < 20)
        let mut audio = vec![0u8; 1024];
        for i in (0..audio.len()).step_by(2) {
            audio[i] = 10u8; // Low amplitude (10 < 20)
            audio[i + 1] = 0u8;
        }
        assert!(is_silence(&audio));
    }

    #[test]
    fn test_is_silence_above_threshold() {
        // Create audio with amplitude above threshold (300 > 200)
        let mut audio = vec![0u8; 1024];
        for i in (0..audio.len()).step_by(2) {
            audio[i] = 44u8; // 300 in little-endian i16
            audio[i + 1] = 1u8;
        }
        assert!(!is_silence(&audio));
    }

    #[test]
    fn test_is_silence_at_threshold() {
        // Create audio with amplitude exactly at threshold (20)
        let mut audio = vec![0u8; 1024];
        for i in (0..audio.len()).step_by(2) {
            audio[i] = 20u8; // 20 in little-endian i16
            audio[i + 1] = 0u8;
        }
        assert!(is_silence(&audio)); // Should be true since <= threshold
    }

    #[test]
    fn test_is_silence_invalid_input() {
        let invalid_audio = vec![0u8, 1u8, 2u8]; // Odd number of bytes
        assert!(!is_silence(&invalid_audio)); // Invalid input should not be considered silence
    }

    #[test]
    fn test_is_silence_mixed_amplitudes() {
        // Create audio with some samples above and some below threshold
        let mut audio = vec![0u8; 1024];
        // First half below threshold
        for i in (0..512).step_by(2) {
            audio[i] = 10u8; // 10 (below threshold)
            audio[i + 1] = 0u8;
        }
        // Second half above threshold
        for i in (512..1024).step_by(2) {
            audio[i] = 44u8; // 300 (above threshold)
            audio[i + 1] = 1u8;
        }
        assert!(!is_silence(&audio)); // Should not be silence due to high amplitude samples
    }
}
