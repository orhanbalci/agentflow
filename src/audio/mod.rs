//! Audio processing and mixing functionality for the AgentFlow AI framework.
//!
//! This module provides audio processing capabilities including mixers for
//! combining audio streams and resamplers for sample rate conversion in
//! real-time applications.

pub mod mixer;
pub mod resampler;
pub mod utils;

pub use mixer::{BaseAudioMixer, MixerResult};
pub use resampler::{
    BaseAudioResampler, ResamplerResult, RubatoAudioResampler, RubatoStreamAudioResampler,
};
pub use utils::{create_stream_resampler, is_silence};
