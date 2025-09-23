//! Rust implementation of the Python BaseOutputTransport class.
//!
//! This file provides a lightweight, placeholder implementation of an output
//! transport that integrates with the existing Rust frame processor.

use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::Mutex;

use crate::frames::{Frame, FrameType, OutputAudioRawFrame, OutputImageRawFrame};
use crate::processors::frame::{FrameDirection, FrameProcessor, FrameProcessorTrait};
use crate::task_manager::TaskManager;
use crate::transport::params::TransportParams;

/// Clock trait for timing operations
pub trait Clock: Send + Sync {
    /// Get current time in nanoseconds
    fn get_time(&self) -> u64;
}

/// Default clock implementation using system time
pub struct SystemClock {
    start_time: Instant,
}

impl SystemClock {
    pub fn new() -> Self {
        Self {
            start_time: Instant::now(),
        }
    }
}

impl Clock for SystemClock {
    fn get_time(&self) -> u64 {
        self.start_time.elapsed().as_nanos() as u64
    }
}

/// Trait for output transport functionality
#[async_trait]
pub trait BaseOutputTransportTrait: Send + Sync {
    /// Get the sample rate for audio output
    fn sample_rate(&self) -> u32;

    /// Get the audio chunk size
    fn audio_chunk_size(&self) -> usize;

    /// Write an audio frame to the output
    async fn write_audio_frame(
        &self,
        frame: &OutputAudioRawFrame,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>>;

    /// Write a video frame to the output
    async fn write_video_frame(
        &self,
        frame: &OutputImageRawFrame,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>>;

    /// Get the clock instance
    fn get_clock(&self) -> Arc<dyn Clock>;

    /// Check if interruptions are allowed
    fn interruptions_allowed(&self) -> bool;

    /// Push a frame in the specified direction
    async fn push_frame(
        &self,
        frame: Box<dyn Frame + Send>,
        direction: FrameDirection,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>>;
}

/// Lightweight base output transport implementation.
///
/// This struct intentionally implements only the essential pieces of the
/// original Python BaseOutputTransport. Many methods are placeholders and
/// should be extended by concrete transports.
pub struct BaseOutputTransport {
    pub params: TransportParams,
    pub frame_processor: FrameProcessor,
    sample_rate: AtomicU32,
    audio_chunk_size: AtomicU32,
    /// Simple map of destinations -> placeholder. In a complete implementation
    /// this would hold MediaSender instances.
    media_senders: Mutex<HashMap<Option<String>, ()>>,
    task_manager: Arc<TaskManager>,
    clock: Arc<SystemClock>,
    stopped: AtomicBool,
}

impl BaseOutputTransport {
    /// Create a new BaseOutputTransport instance
    pub fn new(
        params: TransportParams,
        task_manager: Arc<TaskManager>,
        name: Option<String>,
    ) -> Self {
        let processor_name = name.unwrap_or_else(|| "BaseOutputTransport".to_string());
        let frame_processor = FrameProcessor::new(processor_name, Arc::clone(&task_manager));

        Self {
            params,
            frame_processor,
            sample_rate: AtomicU32::new(0),
            audio_chunk_size: AtomicU32::new(0),
            media_senders: Mutex::new(HashMap::new()),
            task_manager,
            clock: Arc::new(SystemClock::new()),
            stopped: AtomicBool::new(false),
        }
    }

    /// Get current sample rate
    pub fn sample_rate(&self) -> u32 {
        self.sample_rate.load(Ordering::Relaxed)
    }

    /// Get audio chunk size (in bytes)
    pub fn audio_chunk_size(&self) -> u32 {
        self.audio_chunk_size.load(Ordering::Relaxed)
    }

    /// Internal helper to create (placeholder) media senders for destinations.
    async fn ensure_media_senders(&self) {
        let mut senders = self.media_senders.lock().await;
        // default (None)
        senders.entry(None).or_insert(());

        for dest in &self.params.audio_out_destinations {
            senders.entry(Some(dest.clone())).or_insert(());
        }
        for dest in &self.params.video_out_destinations {
            senders.entry(Some(dest.clone())).or_insert(());
        }
    }

    /// Write audio frame - simplified version
    pub async fn write_audio_frame(
        &self,
        _frame: &OutputAudioRawFrame,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        // Placeholder implementation
        Ok(())
    }

    /// Write video frame - simplified version  
    pub async fn write_video_frame(
        &self,
        _frame: &OutputImageRawFrame,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        // Placeholder implementation
        Ok(())
    }

    /// Get the clock for timing
    pub fn get_clock(&self) -> Arc<dyn Clock> {
        self.clock.clone()
    }

    /// Check if interruptions are allowed
    pub fn interruptions_allowed(&self) -> bool {
        // Default to true since allow_interruptions field doesn't exist in params
        true
    }

    /// Push a frame (simplified implementation)
    pub async fn push_frame(
        &self,
        _frame: Box<dyn Frame + Send>,
        _direction: FrameDirection,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        // Placeholder implementation - would delegate to frame processor
        Ok(())
    }

    /// Create a task (simplified)
    pub fn create_task<F>(&self, future: F) -> tokio::task::JoinHandle<()>
    where
        F: std::future::Future<Output = ()> + Send + 'static,
    {
        tokio::spawn(future)
    }

    /// Cancel a task (simplified)
    pub async fn cancel_task(&self, handle: tokio::task::JoinHandle<()>) {
        handle.abort();
    }
}

#[async_trait]
impl FrameProcessorTrait for BaseOutputTransport {
    fn name(&self) -> &str {
        self.frame_processor.name()
    }

    async fn process_frame(
        &mut self,
        frame: FrameType,
        _direction: FrameDirection,
    ) -> Result<(), String> {
        // Basic processing similar to the Python implementation.
        match frame {
            FrameType::Start(start_frame) => {
                // Initialize sample rates and chunk sizes
                let sr = self
                    .params
                    .audio_out_sample_rate
                    .unwrap_or(start_frame.audio_out_sample_rate);
                self.sample_rate.store(sr, Ordering::Relaxed);

                let audio_bytes_10ms = (sr / 100) * (self.params.audio_out_channels as u32) * 2;
                let chunk = audio_bytes_10ms * (self.params.audio_out_10ms_chunks as u32);
                self.audio_chunk_size.store(chunk, Ordering::Relaxed);

                // Mark ready and create media senders placeholders
                self.ensure_media_senders().await;
            }
            FrameType::End(_) => {
                // Stop / cleanup
                self.stopped.store(true, Ordering::Relaxed);
            }
            FrameType::Cancel(_) => {
                self.stopped.store(true, Ordering::Relaxed);
            }
            FrameType::OutputAudioRaw(_) => {
                // Forward to media sender handling in a real implementation
            }
            FrameType::OutputImageRaw(_) => {
                // Forward to media sender handling in a real implementation
            }
            _ => {
                // Default: forward into frame processor chain
            }
        }

        Ok(())
    }
}

#[async_trait]
impl BaseOutputTransportTrait for BaseOutputTransport {
    fn sample_rate(&self) -> u32 {
        self.sample_rate.load(Ordering::Relaxed)
    }

    fn audio_chunk_size(&self) -> usize {
        self.audio_chunk_size.load(Ordering::Relaxed) as usize
    }

    async fn write_audio_frame(
        &self,
        frame: &OutputAudioRawFrame,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        // Placeholder implementation
        Ok(())
    }

    async fn write_video_frame(
        &self,
        frame: &OutputImageRawFrame,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        // Placeholder implementation
        Ok(())
    }

    fn get_clock(&self) -> Arc<dyn Clock> {
        self.clock.clone()
    }

    fn interruptions_allowed(&self) -> bool {
        // Default to true since allow_interruptions field doesn't exist
        true
    }

    async fn push_frame(
        &self,
        _frame: Box<dyn Frame + Send>,
        _direction: FrameDirection,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        // Placeholder implementation - would delegate to frame processor
        Ok(())
    }
}
