//! Frame processing module for handling audio and video frame pipelines
//!
//! This module provides the core frame processing infrastructure including
//! frame processors, pipeline management, and frame flow control mechanisms.

pub mod frame_processor;

// Re-export all public items for convenience
pub use frame_processor::*;
