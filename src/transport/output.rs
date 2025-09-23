//! Rust implementation of the Python BaseOutputTransport class.
//!
//! This file provides a lightweight, placeholder implementation of an output
//! transport that integrates with the existing Rust frame processor.

use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Arc;
use tokio::sync::Mutex;

use crate::frames::{
    EndFrame, Frame, FrameType, OutputAudioRawFrame, OutputImageRawFrame,
    OutputTransportReadyFrame, StartFrame, TransportMessageFrame, TransportMessageUrgentFrame,
};
use crate::processors::frame::{FrameDirection, FrameProcessor, FrameProcessorTrait};
use crate::task_manager::TaskManager;
use crate::transport::params::TransportParams;
use crate::transport::sender::MediaSender;
use crate::{BaseClock, SystemClock};

// Type alias for media sender references
type MediaSenderRef = Arc<Mutex<MediaSender>>;

/// Enum for transport message frame types
#[derive(Debug, Clone)]
pub enum TransportMessageFrameType {
    Message(TransportMessageFrame),
    Urgent(TransportMessageUrgentFrame),
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
    fn get_clock(&self) -> Arc<dyn BaseClock>;

    /// Check if interruptions are allowed
    fn interruptions_allowed(&self) -> bool;

    /// Send a transport message
    async fn send_message(
        &self,
        frame: TransportMessageFrameType,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>>;

    async fn register_audio_destination(
        &self,
        destination: &str,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>>;

    async fn register_video_destination(
        &self,
        destination: &str,
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
    /// Map of destinations to MediaSender instances
    media_senders: Mutex<HashMap<Option<String>, MediaSenderRef>>,
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
    ) -> Arc<Self> {
        let processor_name = name.unwrap_or_else(|| "BaseOutputTransport".to_string());
        let frame_processor = FrameProcessor::new(processor_name, Arc::clone(&task_manager));

        Arc::new(Self {
            params,
            frame_processor,
            sample_rate: AtomicU32::new(0),
            audio_chunk_size: AtomicU32::new(0),
            media_senders: Mutex::new(HashMap::new()),
            task_manager,
            clock: Arc::new(SystemClock::new()),
            stopped: AtomicBool::new(false),
        })
    }

    /// Internal helper to create media senders for destinations when we have an Arc<Self>.
    async fn ensure_media_senders(&self, transport_ref: &Arc<BaseOutputTransport>) {
        let mut senders = self.media_senders.lock().await;

        // Create default sender (None destination)
        if !senders.contains_key(&None) {
            let sender = MediaSender::new(
                transport_ref.clone(),
                None,
                self.sample_rate.load(Ordering::Relaxed),
                self.audio_chunk_size.load(Ordering::Relaxed) as usize,
                self.params.clone(),
            );
            senders.insert(None, Arc::new(Mutex::new(sender)));
        }

        // Create senders for audio destinations
        for dest in &self.params.audio_out_destinations {
            if !senders.contains_key(&Some(dest.clone())) {
                let sender = MediaSender::new(
                    transport_ref.clone(),
                    Some(dest.clone()),
                    self.sample_rate.load(Ordering::Relaxed),
                    self.audio_chunk_size.load(Ordering::Relaxed) as usize,
                    self.params.clone(),
                );
                senders.insert(Some(dest.clone()), Arc::new(Mutex::new(sender)));
            }
        }

        // Create senders for video destinations
        for dest in &self.params.video_out_destinations {
            if !senders.contains_key(&Some(dest.clone())) {
                let sender = MediaSender::new(
                    transport_ref.clone(),
                    Some(dest.clone()),
                    self.sample_rate.load(Ordering::Relaxed),
                    self.audio_chunk_size.load(Ordering::Relaxed) as usize,
                    self.params.clone(),
                );
                senders.insert(Some(dest.clone()), Arc::new(Mutex::new(sender)));
            }
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
    pub fn get_clock(&self) -> Arc<dyn BaseClock> {
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

    /// Start the output transport and initialize components.
    ///
    /// Args:
    ///     frame: The start frame containing initialization parameters.
    pub async fn start(
        self: &Arc<Self>,
        frame: &StartFrame,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        // Set sample rate from params or frame
        let sample_rate = self
            .params
            .audio_out_sample_rate
            .unwrap_or(frame.audio_out_sample_rate);
        self.sample_rate.store(sample_rate, Ordering::Relaxed);

        // We will write 10ms*CHUNKS of audio at a time (where CHUNKS is the
        // `audio_out_10ms_chunks` parameter). If we receive long audio frames we
        // will chunk them. This will help with interruption handling.
        let audio_bytes_10ms =
            (sample_rate as f64 / 100.0) as u32 * (self.params.audio_out_channels as u32) * 2; // 2 bytes per sample (16-bit)
        let audio_chunk_size = audio_bytes_10ms * (self.params.audio_out_10ms_chunks as u32);
        self.audio_chunk_size
            .store(audio_chunk_size, Ordering::Relaxed);

        // Initialize media senders
        self.ensure_media_senders(self).await;

        Ok(())
    }

    /// Called when the transport is ready to stream.
    ///
    /// This function registers destinations and starts media senders,
    /// then signals that the output transport is ready to receive frames.
    ///
    /// Args:
    ///     frame: The start frame containing initialization parameters.
    pub async fn set_transport_ready(
        self: &Arc<Self>,
        frame: &StartFrame,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        // Note: The original Python code calls register_audio_destination and register_video_destination
        // but those methods don't exist in our Rust implementation. The functionality is already
        // handled in ensure_media_senders, so we'll skip those calls.

        // Start default media sender (destination=None)
        {
            let senders = self.media_senders.lock().await;
            if let Some(sender_ref) = senders.get(&None) {
                let mut sender = sender_ref.lock().await;
                sender.start(frame).await?;
            }
        }

        // Get unique destinations from both audio and video destinations
        let mut destinations = self.params.audio_out_destinations.clone();
        destinations.extend(self.params.video_out_destinations.clone());
        destinations.sort();
        destinations.dedup();

        // Start media senders for each destination
        {
            let senders = self.media_senders.lock().await;
            for destination in destinations {
                if let Some(sender_ref) = senders.get(&Some(destination.clone())) {
                    let mut sender = sender_ref.lock().await;
                    sender.start(frame).await?;
                }
            }
        }

        // Send a frame indicating that the output transport is ready and able to receive frames
        let ready_frame = OutputTransportReadyFrame::new();

        // Since push_frame expects Box<dyn Frame + Send>, we'll pass the ready_frame directly
        // Note: This is a placeholder implementation - the proper frame handling would be more complex
        self.push_frame(Box::new(ready_frame), FrameDirection::Upstream)
            .await?;

        Ok(())
    }

    /// Stop the output transport and cleanup resources.
    ///
    /// Args:
    ///     frame: The end frame signaling transport shutdown.
    pub async fn stop(
        &self,
        frame: &EndFrame,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        // Mark as stopped
        self.stopped.store(true, Ordering::Relaxed);

        // Stop all media senders - real implementation
        let senders = self.media_senders.lock().await;
        for (destination, sender_ref) in senders.iter() {
            log::debug!("Stopping media sender for destination: {:?}", destination);
            let mut sender = sender_ref.lock().await;
            if let Err(e) = sender.stop(frame).await {
                log::error!("Failed to stop media sender for {:?}: {}", destination, e);
                // Continue stopping other senders even if one fails
            }
        }

        Ok(())
    }

    /// Cancel the output transport and stop all processing.
    ///
    /// Args:
    ///     frame: The cancel frame signaling immediate cancellation.
    pub async fn cancel(
        &self,
        frame: &crate::CancelFrame,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        // Mark as stopped
        self.stopped.store(true, Ordering::Relaxed);

        // Cancel all media senders - real implementation
        let senders = self.media_senders.lock().await;
        for (destination, sender_ref) in senders.iter() {
            log::debug!("Cancelling media sender for destination: {:?}", destination);
            let mut sender = sender_ref.lock().await;
            if let Err(e) = sender.cancel(frame).await {
                log::error!("Failed to cancel media sender for {:?}: {}", destination, e);
                // Continue cancelling other senders even if one fails
            }
        }

        // Clear the senders map immediately on cancel
        drop(senders);
        let mut senders = self.media_senders.lock().await;
        senders.clear();

        Ok(())
    }

    /// Send a transport message.
    ///
    /// Args:
    ///     frame: The transport message frame to send.
    pub async fn send_message(
        &self,
        frame: TransportMessageFrameType,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        // should be implemented by concrete transports
        Ok(())
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
        _frame: &OutputAudioRawFrame,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        // Placeholder implementation
        Ok(())
    }

    async fn write_video_frame(
        &self,
        _frame: &OutputImageRawFrame,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        // Placeholder implementation
        Ok(())
    }

    fn get_clock(&self) -> Arc<dyn BaseClock> {
        self.clock.clone()
    }

    fn interruptions_allowed(&self) -> bool {
        // Default to true since allow_interruptions field doesn't exist
        true
    }

    async fn send_message(
        &self,
        frame: TransportMessageFrameType,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        // Implementation for sending transport messages       
        Ok(())
    }

    async fn register_audio_destination(
        &self,
        _destination: &str,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        // Placeholder: In a real implementation, this would register the audio destination.
        Ok(())
    }

    async fn register_video_destination(
        &self,
        _destination: &str,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        // Placeholder: In a real implementation, this would register the video destination.
        Ok(())
    }
}
