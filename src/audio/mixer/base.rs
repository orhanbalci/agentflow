//! Audio mixer traits and implementations for the AgentFlow AI framework.
//!
//! This module provides the base trait for audio mixers that can be used with
//! output transports to mix transport audio with mixer-generated audio.

use crate::{audio::mixer::soundfile, frames::MixerControlFrame};
use std::collections::HashMap;

pub use soundfile::SoundfileMixer;

/// Result type for mixer operations
pub type MixerResult<T> = Result<T, Box<dyn std::error::Error + Send + Sync>>;

/// Base trait for output transport audio mixers.
///
/// This trait defines the interface for audio mixers that can be integrated with
/// output transports. If an audio mixer is provided to the output transport, it
/// will be used to mix the audio frames coming into the transport with the audio
/// generated from the mixer. Control frames can be used to update mixer settings
/// or to enable/disable the mixer at runtime.
#[async_trait::async_trait]
pub trait BaseAudioMixer: Send + Sync + std::fmt::Debug {
    /// Initialize the mixer when the output transport starts.
    ///
    /// This will be called from the output transport when the transport is
    /// started. It can be used to initialize the mixer. The output transport
    /// sample rate is provided so the mixer can adjust to that sample rate.
    ///
    /// # Arguments
    /// * `sample_rate` - The sample rate of the output transport in Hz.
    ///
    /// # Errors
    /// Returns an error if the mixer cannot be initialized with the given sample rate.
    async fn start(&mut self, sample_rate: u32) -> MixerResult<()>;

    /// Clean up the mixer when the output transport stops.
    ///
    /// This will be called from the output transport when the transport is
    /// stopping. Implementations should clean up any resources and stop any
    /// background processes.
    ///
    /// # Errors
    /// Returns an error if cleanup fails, though this is typically logged rather
    /// than propagated.
    async fn stop(&mut self) -> MixerResult<()>;

    /// Process mixer control frames from the transport.
    ///
    /// This will be called when the output transport receives a MixerControlFrame.
    /// The implementation should handle the control frame and update the mixer's
    /// state accordingly (e.g., enable/disable, change volume, update settings).
    ///
    /// # Arguments
    /// * `frame` - The mixer control frame to process.
    ///
    /// # Errors
    /// Returns an error if the control frame cannot be processed.
    async fn process_frame(&mut self, frame: &MixerControlFrame) -> MixerResult<()>;

    /// Mix transport audio with mixer-generated audio.
    ///
    /// This is called with the audio that is about to be sent from the output
    /// transport and should be mixed with the mixer audio if the mixer is enabled.
    /// The implementation should generate or retrieve mixer audio and combine it
    /// with the provided transport audio.
    ///
    /// # Arguments
    /// * `audio` - Raw audio bytes from the transport to mix.
    ///
    /// # Returns
    /// Mixed audio bytes combining transport and mixer audio.
    ///
    /// # Errors
    /// Returns an error if mixing fails or if audio format is incompatible.
    async fn mix(&mut self, audio: &[u8]) -> MixerResult<Vec<u8>>;

    /// Get the current mixer configuration.
    ///
    /// Returns the current settings of the mixer, including enabled state,
    /// volume level, and any custom settings.
    fn get_config(&self) -> HashMap<String, String> {
        HashMap::new()
    }

    /// Check if the mixer is currently enabled.
    ///
    /// # Returns
    /// `true` if the mixer is enabled and should process audio, `false` otherwise.
    fn is_enabled(&self) -> bool {
        true
    }

    /// Get the current volume level.
    ///
    /// # Returns
    /// Volume level as a float between 0.0 and 1.0, where 1.0 is full volume.
    fn get_volume(&self) -> f32 {
        1.0
    }
}
