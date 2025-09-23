//! Clock implementations for timing operations in the AgentFlow framework.
//!
//! This module provides different types of clocks for synchronization,
//! scheduling, and time-based processing in real-time applications.

pub mod base;
pub mod system;

pub use base::{BaseClock, ClockResult};
pub use system::SystemClock;
