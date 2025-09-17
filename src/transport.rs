/// Transport module for AgentFlow
///
/// This module provides the core transport infrastructure for media streaming applications.
/// It includes base transport functionality, configuration parameters, input transport
/// handling, and specialized transport implementations.
///
/// ## Module Organization
///
/// - `base`: Core transport traits and base implementation
/// - `params`: Transport configuration parameters and types
/// - `input`: Input transport implementation for audio/video processing
///
/// The transport system follows modern Rust module organization practices, with each
/// submodule focused on specific functionality while maintaining clear interfaces
/// between components.
pub mod base;
pub mod input;
pub mod params;

// Re-export commonly used types for convenience
pub use base::{BaseTransport, BaseTransportBuilder, BaseTransportImpl};
pub use input::BaseInputTransport;
pub use params::{AudioFilterHandle, AudioMixerHandle, TransportParams};
