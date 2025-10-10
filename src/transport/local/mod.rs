//! Local audio transport implementation for AgentFlow.
//!
//! This module provides a local audio transport that uses CPAL for real-time
//! audio input and output through the system's default audio devices.
//!
//! ## Features
//!
//! - Real-time audio capture from system input devices
//! - Real-time audio playback to system output devices  
//! - Cross-platform support via CPAL
//! - Device enumeration and selection
//! - Configurable buffer sizes and audio parameters
//!
//! ## Usage
//!
//! ```rust,no_run
//! use agentflow::transport::local::{LocalAudioTransport, LocalAudioTransportParams};
//! use agentflow::transport::params::TransportParams;
//! use agentflow::task_manager::{TaskManager, TaskManagerConfig};
//! use agentflow::StartFrame;
//! use std::sync::Arc;
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! let config = TaskManagerConfig::default();
//! let task_manager = Arc::new(TaskManager::new(config));
//!
//! // Create transport parameters
//! let base_params = TransportParams::default();
//! let local_params = LocalAudioTransportParams::new(base_params)
//!     .with_buffer_size(1024);
//!
//! // Create and start the transport
//! let transport = LocalAudioTransport::new(local_params, task_manager);
//! let start_frame = StartFrame::new().with_sample_rates(16000, 16000);
//! transport.start(&start_frame).await?;
//!
//! // ... use the transport for audio I/O ...
//!
//! // Stop the transport
//! transport.stop().await?;
//! # Ok(())
//! # }
//! ```
//!
//! ## Device Selection
//!
//! You can list available devices and select specific ones:
//!
//! ```rust,no_run
//! use agentflow::transport::local::LocalAudioTransport;
//!
//! # fn example() -> Result<(), Box<dyn std::error::Error>> {
//! // List available devices
//! let input_devices = LocalAudioTransport::list_input_devices()?;
//! let output_devices = LocalAudioTransport::list_output_devices()?;
//!
//! println!("Input devices: {:?}", input_devices);
//! println!("Output devices: {:?}", output_devices);
//!
//! // Get default devices
//! let default_input = LocalAudioTransport::default_input_device_name()?;
//! let default_output = LocalAudioTransport::default_output_device_name()?;
//!
//! println!("Default input: {}", default_input);
//! println!("Default output: {}", default_output);
//! # Ok(())
//! # }
//! ```

pub mod input;
pub mod output;
pub mod params;
pub mod transport;

// Re-export commonly used types for convenience
pub use input::LocalAudioInputTransport;
pub use output::LocalAudioOutputTransport;
pub use params::LocalAudioTransportParams;
pub use transport::LocalAudioTransport;
