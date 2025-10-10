//! Base pipeline trait for implementing pipeline functionality.

use async_trait::async_trait;

use crate::FrameProcessorTrait;

/// Base trait for all pipeline implementations.
///
/// This trait defines the common interface that all pipeline types must implement,
/// providing core functionality for frame processing, setup, and cleanup.
#[async_trait]
pub trait BasePipeline: Send + Sync + FrameProcessorTrait {}
