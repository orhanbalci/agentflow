//! Audio filter implementations for the AgentFlow framework.
//!
//! This module provides different types of audio filters for processing
//! audio data before VAD and downstream processing in input transports.

pub mod base;

pub use base::{BaseAudioFilter, FilterResult};
