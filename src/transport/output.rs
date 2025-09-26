//! Rust implementation of the Python BaseOutputTransport class.
//!
//! This file provides a lightweight, placeholder implementation of an output
//! transport that integrates with the existing Rust frame processor.

use async_trait::async_trait;
use delegate::delegate;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Arc;
use tokio::sync::Mutex;

use crate::audio::mixer::BaseAudioMixer;
use crate::frames::{
    CancelFrame, EndFrame, Frame, FrameType, OutputAudioRawFrame, OutputImageRawFrame,
    OutputTransportReadyFrame, SpriteFrame, StartFrame, TransportMessageFrame,
    TransportMessageUrgentFrame,
};
use crate::processors::frame::{
    BaseInterruptionStrategy, FrameCallback, FrameDirection, FrameProcessor, FrameProcessorMetrics,
    FrameProcessorSetup, FrameProcessorTrait,
};
use crate::task_manager::TaskManager;
use crate::transport::params::TransportParams;
use crate::{BaseClock, SystemClock};
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

// Type alias for frame queues per destination
type AudioQueue = mpsc::UnboundedSender<Box<dyn Frame + Send>>;
type VideoQueue = mpsc::UnboundedSender<OutputImageRawFrame>;
type ClockQueue = mpsc::UnboundedSender<(u64, u64, Box<dyn Frame + Send>)>;

/// Convert nanoseconds to seconds
/// Placeholder for audio resampler trait
#[async_trait::async_trait]
trait AudioResampler: Send + Sync {
    async fn resample(
        &mut self,
        audio_data: &[u8],
        input_rate: u32,
        output_rate: u32,
    ) -> Result<Vec<u8>, Box<dyn std::error::Error + Send + Sync>>;
}

/// Placeholder implementation for audio resampler
struct DefaultAudioResampler;

#[async_trait::async_trait]
impl AudioResampler for DefaultAudioResampler {
    async fn resample(
        &mut self,
        audio_data: &[u8],
        _input_rate: u32,
        _output_rate: u32,
    ) -> Result<Vec<u8>, Box<dyn std::error::Error + Send + Sync>> {
        // Placeholder implementation - just return the input data
        Ok(audio_data.to_vec())
    }
}

/// Video image cycling for output
enum VideoImageCycle {
    Single(OutputImageRawFrame),
    Multiple(Vec<OutputImageRawFrame>, usize), // images and current index
}

impl VideoImageCycle {
    fn next(&mut self) -> &OutputImageRawFrame {
        match self {
            VideoImageCycle::Single(image) => image,
            VideoImageCycle::Multiple(images, index) => {
                let image = &images[*index];
                *index = (*index + 1) % images.len();
                image
            }
        }
    }
}

/// Per-destination media processing state
struct DestinationState {
    // Audio processing
    audio_buffer: Vec<u8>,
    audio_resampler: Box<dyn AudioResampler>,
    audio_mixer: Option<Box<dyn BaseAudioMixer>>,

    // Video processing
    video_images: Option<VideoImageCycle>,

    // Tasks and queues
    audio_task: Option<JoinHandle<()>>,
    video_task: Option<JoinHandle<()>>,
    clock_task: Option<JoinHandle<()>>,
    audio_queue: Option<AudioQueue>,
    video_queue: Option<VideoQueue>,
}

/// Enum for transport message frame types
#[derive(Debug, Clone)]
pub enum TransportMessageFrameType {
    Message(TransportMessageFrame),
    Urgent(TransportMessageUrgentFrame),
}

/// Enum for image frame types
#[derive(Debug)]
pub enum ImageFrameType {
    OutputImageRaw(OutputImageRawFrame),
    Sprite(SpriteFrame),
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
    /// Map of destinations to their processing state
    destination_states: Mutex<HashMap<Option<String>, DestinationState>>,
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
            destination_states: Mutex::new(HashMap::new()),
            task_manager,
            clock: Arc::new(SystemClock::new()),
            stopped: AtomicBool::new(false),
        })
    }

    /// Get the sample rate for audio output
    pub fn sample_rate(&self) -> u32 {
        self.sample_rate.load(Ordering::Relaxed)
    }

    /// Get the audio chunk size
    pub fn audio_chunk_size(&self) -> usize {
        self.audio_chunk_size.load(Ordering::Relaxed) as usize
    }

    /// Start the output transport and initialize components.
    ///
    /// Args:
    ///     frame: The start frame containing initialization parameters.
    pub async fn start(
        &self,
        _frame: &StartFrame,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        // Set sample rate from params or frame
        let sample_rate = self
            .params
            .audio_out_sample_rate
            .unwrap_or(_frame.audio_out_sample_rate);
        self.sample_rate.store(sample_rate, Ordering::Relaxed);

        // We will write 10ms*CHUNKS of audio at a time (where CHUNKS is the
        // `audio_out_10ms_chunks` parameter). If we receive long audio frames we
        // will chunk them. This will help with interruption handling.
        let audio_bytes_10ms =
            (sample_rate as f64 / 100.0) as u32 * (self.params.audio_out_channels as u32) * 2; // 2 bytes per sample (16-bit)
        let audio_chunk_size = audio_bytes_10ms * (self.params.audio_out_10ms_chunks as u32);
        self.audio_chunk_size
            .store(audio_chunk_size, Ordering::Relaxed);

        // Initialize destination states
        self.ensure_destination_states().await;

        Ok(())
    }

    /// Stop the output transport and cleanup resources.
    ///
    /// Args:
    ///     frame: The end frame signaling transport shutdown.
    pub async fn stop(
        &self,
        _frame: &EndFrame,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        // Mark as stopped
        self.stopped.store(true, Ordering::Relaxed);

        // Stop all destination states
        let mut states = self.destination_states.lock().await;
        for (destination, state) in states.iter_mut() {
            log::debug!("Stopping destination: {:?}", destination);

            // Stop audio mixer
            if let Some(mixer) = &mut state.audio_mixer {
                if let Err(e) = mixer.stop().await {
                    log::error!("Failed to stop mixer for {:?}: {}", destination, e);
                }
            }

            // Cancel tasks
            if let Some(task) = state.audio_task.take() {
                task.abort();
                let _ = task.await;
            }
            if let Some(task) = state.video_task.take() {
                task.abort();
                let _ = task.await;
            }
            if let Some(task) = state.clock_task.take() {
                task.abort();
                let _ = task.await;
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
        _frame: &CancelFrame,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        // Mark as stopped
        self.stopped.store(true, Ordering::Relaxed);

        // Cancel all destination states
        let mut states = self.destination_states.lock().await;
        for (destination, state) in states.iter_mut() {
            log::debug!("Cancelling destination: {:?}", destination);

            // Cancel tasks immediately
            if let Some(task) = state.audio_task.take() {
                task.abort();
            }
            if let Some(task) = state.video_task.take() {
                task.abort();
            }
            if let Some(task) = state.clock_task.take() {
                task.abort();
            }
        }

        // Clear the states map immediately on cancel
        states.clear();

        Ok(())
    }

    /// Called when the transport is ready to stream.
    ///
    /// This function starts processing tasks for destinations,
    /// then signals that the output transport is ready to receive frames.
    ///
    /// Args:
    ///     frame: The start frame containing initialization parameters.
    pub async fn set_transport_ready(
        &self,
        _frame: &StartFrame,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        // Start tasks for default destination (destination=None)
        self.start_destination_tasks(&None).await?;

        // Get unique destinations from both audio and video destinations
        let mut destinations = self.params.audio_out_destinations.clone();
        destinations.extend(self.params.video_out_destinations.clone());
        destinations.sort();
        destinations.dedup();

        // Start tasks for each destination
        for destination in destinations {
            self.start_destination_tasks(&Some(destination)).await?;
        }

        // Send a frame indicating that the output transport is ready and able to receive frames
        let ready_frame = OutputTransportReadyFrame::new();

        // Use FrameType for the delegated push_frame method
        self.push_frame(
            FrameType::OutputTransportReady(ready_frame),
            FrameDirection::Upstream,
        )
        .await
        .map_err(|e| {
            Box::new(std::io::Error::new(std::io::ErrorKind::Other, e))
                as Box<dyn std::error::Error + Send + Sync>
        })?;

        Ok(())
    }

    /// Send a transport message.
    ///
    /// Args:
    ///     frame: The transport message frame to send.
    pub async fn send_message(
        &self,
        _frame: TransportMessageFrameType,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        // should be implemented by concrete transports
        Ok(())
    }

    /// Register a video destination
    pub async fn register_video_destination(
        &self,
        _destination: &str,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        // Placeholder: In a real implementation, this would register the video destination.
        Ok(())
    }

    /// Register an audio destination
    pub async fn register_audio_destination(
        &self,
        _destination: &str,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        // Placeholder: In a real implementation, this would register the audio destination.
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

    /// Write audio frame - simplified version
    pub async fn write_audio_frame(
        &self,
        _frame: &OutputAudioRawFrame,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        // Placeholder implementation
        Ok(())
    }

    /// Send an audio frame downstream.
    ///
    /// Args:
    ///     frame: The audio frame to send.
    pub async fn send_audio(
        &self,
        frame: OutputAudioRawFrame,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        // Send the audio frame downstream via the frame processor using FrameType
        self.push_frame(FrameType::OutputAudioRaw(frame), FrameDirection::Downstream)
            .await
            .map_err(|e| {
                Box::new(std::io::Error::new(std::io::ErrorKind::Other, e))
                    as Box<dyn std::error::Error + Send + Sync>
            })
    }

    /// Send an image frame downstream.
    ///
    /// Args:
    ///     frame: The image frame to send.
    pub async fn send_image(
        &self,
        frame: ImageFrameType,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        // Send the image frame downstream via the frame processor using FrameType
        let result = match frame {
            ImageFrameType::OutputImageRaw(img_frame) => {
                self.push_frame(
                    FrameType::OutputImageRaw(img_frame),
                    FrameDirection::Downstream,
                )
                .await
            }
            ImageFrameType::Sprite(sprite_frame) => {
                self.push_frame(FrameType::Sprite(sprite_frame), FrameDirection::Downstream)
                    .await
            }
        };
        result.map_err(|e| {
            Box::new(std::io::Error::new(std::io::ErrorKind::Other, e))
                as Box<dyn std::error::Error + Send + Sync>
        })
    }

    /// Internal helper to initialize destination states.
    async fn ensure_destination_states(&self) {
        let mut states = self.destination_states.lock().await;

        // Create default destination state (None destination)
        if !states.contains_key(&None) {
            states.insert(
                None,
                DestinationState {
                    audio_buffer: Vec::new(),
                    audio_resampler: Box::new(DefaultAudioResampler),
                    audio_mixer: None,
                    video_images: None,
                    audio_task: None,
                    video_task: None,
                    clock_task: None,
                    audio_queue: None,
                    video_queue: None,
                },
            );
        }

        // Create states for audio destinations
        for dest in &self.params.audio_out_destinations {
            if !states.contains_key(&Some(dest.clone())) {
                states.insert(
                    Some(dest.clone()),
                    DestinationState {
                        audio_buffer: Vec::new(),
                        audio_resampler: Box::new(DefaultAudioResampler),
                        audio_mixer: None,
                        video_images: None,
                        audio_task: None,
                        video_task: None,
                        clock_task: None,
                        audio_queue: None,
                        video_queue: None,
                    },
                );
            }
        }

        // Create states for video destinations
        for dest in &self.params.video_out_destinations {
            if !states.contains_key(&Some(dest.clone())) {
                states.insert(
                    Some(dest.clone()),
                    DestinationState {
                        audio_buffer: Vec::new(),
                        audio_resampler: Box::new(DefaultAudioResampler),
                        audio_mixer: None,
                        video_images: None,
                        audio_task: None,
                        video_task: None,
                        clock_task: None,
                        audio_queue: None,
                        video_queue: None,
                    },
                );
            }
        }
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

    /// Start processing tasks for a specific destination.
    async fn start_destination_tasks(
        &self,
        destination: &Option<String>,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let mut states = self.destination_states.lock().await;
        if let Some(state) = states.get_mut(destination) {
            // Start audio mixer if configured
            if self.params.audio_out_mixer.is_some() && state.audio_mixer.is_some() {
                if let Some(mixer) = &mut state.audio_mixer {
                    mixer
                        .start(self.sample_rate.load(Ordering::Relaxed))
                        .await?;
                }
            }

            // Create and start tasks (placeholder implementation)
            // In a full implementation, these would create the actual processing tasks
            // similar to what was in MediaSender::start()
        }
        Ok(())
    }

    /// Handle frames by delegating to appropriate destination processing.
    ///
    /// Args:
    ///     frame: The frame to handle.
    async fn _handle_frame(
        &self,
        frame: FrameType,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        // Get the transport destination from the frame
        let transport_destination = frame.transport_destination().map(|s| s.to_string());

        // Check if we have a destination state for this destination
        let states = self.destination_states.lock().await;
        if !states.contains_key(&transport_destination) {
            log::warn!(
                "Destination {:?} not registered for frame {}",
                transport_destination,
                frame.name()
            );
            return Ok(());
        }

        // Route frame to appropriate handler based on frame type
        match frame {
            FrameType::BotInterruption(_) => {
                log::debug!(
                    "Handling bot interruption frame for destination: {:?}",
                    transport_destination
                );
                // In a real implementation, this would handle the interruption
            }
            FrameType::StartInterruption(_) => {
                log::debug!(
                    "Handling start interruption frame for destination: {:?}",
                    transport_destination
                );
                // In a real implementation, this would handle the interruption
            }
            FrameType::StopInterruption(_) => {
                log::debug!(
                    "Handling stop interruption frame for destination: {:?}",
                    transport_destination
                );
                // In a real implementation, this would handle the interruption
            }
            FrameType::OutputAudioRaw(audio_frame) => {
                log::debug!(
                    "Handling audio frame for destination: {:?}",
                    transport_destination
                );
                self.handle_audio_frame(&audio_frame, &transport_destination)
                    .await?;
            }
            FrameType::OutputImageRaw(image_frame) => {
                log::debug!(
                    "Handling image frame for destination: {:?}",
                    transport_destination
                );
                self.handle_image_frame(&image_frame, &transport_destination)
                    .await?;
            }
            FrameType::Sprite(sprite_frame) => {
                log::debug!(
                    "Handling sprite frame for destination: {:?}",
                    transport_destination
                );
                self.handle_sprite_frame(&sprite_frame, &transport_destination)
                    .await?;
            }
            _ => {
                // Handle timed frames or sync frames based on PTS
                if frame.pts().is_some() {
                    log::debug!(
                        "Handling timed frame for destination: {:?}",
                        transport_destination
                    );
                    // In a real implementation, this would handle the timed frame
                } else {
                    log::debug!(
                        "Handling sync frame for destination: {:?}",
                        transport_destination
                    );
                    // In a real implementation, this would handle the sync frame
                }
            }
        }

        Ok(())
    }

    /// Handle incoming audio frames by buffering and chunking.
    async fn handle_audio_frame(
        &self,
        frame: &OutputAudioRawFrame,
        destination: &Option<String>,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        if !self.params.audio_out_enabled {
            return Ok(());
        }

        let mut states = self.destination_states.lock().await;
        if let Some(state) = states.get_mut(destination) {
            // Resample if incoming audio doesn't match the transport sample rate
            let resampled = state
                .audio_resampler
                .resample(
                    &frame.audio_frame.audio,
                    frame.audio_frame.sample_rate,
                    self.sample_rate.load(Ordering::Relaxed),
                )
                .await?;

            state.audio_buffer.extend_from_slice(&resampled);

            let audio_chunk_size = self.audio_chunk_size.load(Ordering::Relaxed) as usize;
            while state.audio_buffer.len() >= audio_chunk_size {
                let chunk_data = state
                    .audio_buffer
                    .drain(..audio_chunk_size)
                    .collect::<Vec<_>>();

                let mut chunk = OutputAudioRawFrame::new(
                    chunk_data,
                    self.sample_rate.load(Ordering::Relaxed),
                    frame.audio_frame.num_channels,
                );
                chunk.set_transport_destination(destination.clone());

                if let Some(audio_sender) = &state.audio_queue {
                    let _ = audio_sender.send(Box::new(chunk));
                }
            }
        }

        Ok(())
    }

    /// Handle incoming image frames for video output.
    async fn handle_image_frame(
        &self,
        frame: &OutputImageRawFrame,
        destination: &Option<String>,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        if !self.params.video_out_enabled {
            return Ok(());
        }

        let mut states = self.destination_states.lock().await;
        if let Some(state) = states.get_mut(destination) {
            if self.params.video_out_is_live {
                if let Some(video_sender) = &state.video_queue {
                    let _ = video_sender.send(frame.clone());
                }
            } else {
                state.video_images = Some(VideoImageCycle::Single(frame.clone()));
            }
        }

        Ok(())
    }

    /// Handle sprite frames for video output.
    async fn handle_sprite_frame(
        &self,
        frame: &SpriteFrame,
        destination: &Option<String>,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        if !self.params.video_out_enabled {
            return Ok(());
        }

        let mut states = self.destination_states.lock().await;
        if let Some(state) = states.get_mut(destination) {
            if !frame.images.is_empty() {
                state.video_images = Some(VideoImageCycle::Multiple(frame.images.clone(), 0));
            }
        }

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
        direction: FrameDirection,
    ) -> Result<(), String> {
        // System frames (like InterruptionFrame) are pushed immediately. Other
        // frames require order so they are put in the sink queue.
        match &frame {
            FrameType::Start(start_frame) => {
                // Push StartFrame before start(), because we want StartFrame to be
                // processed by every processor before any other frame is processed.
                self.push_frame(FrameType::Start(start_frame.clone()), direction)
                    .await
                    .map_err(|e| e.to_string())?;

                // Call start method directly
                self.start(start_frame).await.map_err(|e| e.to_string())?;
            }
            FrameType::Cancel(cancel_frame) => {
                self.cancel(cancel_frame).await.map_err(|e| e.to_string())?;
                self.push_frame(FrameType::Cancel(cancel_frame.clone()), direction)
                    .await
                    .map_err(|e| e.to_string())?;
            }
            FrameType::BotInterruption(interruption_frame) => {
                self.push_frame(
                    FrameType::BotInterruption(interruption_frame.clone()),
                    direction,
                )
                .await
                .map_err(|e| e.to_string())?;
                self._handle_frame(frame.clone())
                    .await
                    .map_err(|e| e.to_string())?;
            }
            FrameType::StartInterruption(interruption_frame) => {
                self.push_frame(
                    FrameType::StartInterruption(interruption_frame.clone()),
                    direction,
                )
                .await
                .map_err(|e| e.to_string())?;
                self._handle_frame(frame.clone())
                    .await
                    .map_err(|e| e.to_string())?;
            }
            FrameType::StopInterruption(interruption_frame) => {
                self.push_frame(
                    FrameType::StopInterruption(interruption_frame.clone()),
                    direction,
                )
                .await
                .map_err(|e| e.to_string())?;
                self._handle_frame(frame.clone())
                    .await
                    .map_err(|e| e.to_string())?;
            }
            FrameType::TransportMessageUrgent(urgent_frame) => {
                let frame_type = TransportMessageFrameType::Urgent(urgent_frame.clone());
                self.send_message(frame_type)
                    .await
                    .map_err(|e| e.to_string())?;
            }
            // System frames
            FrameType::OutputTransportReady(ready_frame) => {
                self.push_frame(
                    FrameType::OutputTransportReady(ready_frame.clone()),
                    direction,
                )
                .await
                .map_err(|e| e.to_string())?;
            }
            // Control frames
            FrameType::End(end_frame) => {
                self.stop(end_frame).await.map_err(|e| e.to_string())?;
                // Keep pushing EndFrame down so all the pipeline stops nicely.
                self.push_frame(FrameType::End(end_frame.clone()), direction)
                    .await
                    .map_err(|e| e.to_string())?;
            }
            // Other frames
            FrameType::OutputAudioRaw(_) => {
                self._handle_frame(frame.clone())
                    .await
                    .map_err(|e| e.to_string())?;
            }
            FrameType::OutputImageRaw(_) => {
                self._handle_frame(frame.clone())
                    .await
                    .map_err(|e| e.to_string())?;
            }
            FrameType::Sprite(_) => {
                self._handle_frame(frame.clone())
                    .await
                    .map_err(|e| e.to_string())?;
            }
            // TODO(aleix): Images and audio should support presentation timestamps.
            _ => {
                // Check if frame has PTS (presentation timestamp)
                // For now, we'll check if the frame has a pts value
                if frame.pts().is_some() {
                    self._handle_frame(frame.clone())
                        .await
                        .map_err(|e| e.to_string())?;
                } else if direction == FrameDirection::Upstream {
                    // Push frame upstream - use the frame directly as FrameType
                    self.push_frame(frame.clone(), direction)
                        .await
                        .map_err(|e| e.to_string())?;
                } else {
                    self._handle_frame(frame.clone())
                        .await
                        .map_err(|e| e.to_string())?;
                }
            }
        }

        Ok(())
    }
}

impl BaseOutputTransport {}

impl BaseOutputTransport {
    // Delegate basic FrameProcessor functionality to the inner frame_processor field
    delegate! {
        to self.frame_processor {
            pub fn id(&self) -> u64;
            pub fn is_started(&self) -> bool;
            pub fn is_cancelling(&self) -> bool;
            pub fn set_allow_interruptions(&mut self, allow: bool);
            pub fn set_enable_metrics(&mut self, enable: bool);
            pub fn set_enable_usage_metrics(&mut self, enable: bool);
            pub fn set_report_only_initial_ttfb(&mut self, report: bool);
            pub fn add_processor(&mut self, processor: Arc<Mutex<FrameProcessor>>);
            pub fn clear_processors(&mut self);
            pub fn is_compound_processor(&self) -> bool;
            pub fn processor_count(&self) -> usize;
            pub fn get_processor(&self, index: usize) -> Option<&Arc<Mutex<FrameProcessor>>>;
            pub fn processors(&self) -> &Vec<Arc<Mutex<FrameProcessor>>>;
        }
    }

    delegate! {
        to self.frame_processor {
            pub fn set_clock(&mut self, clock: Arc<dyn BaseClock>);
            pub fn set_task_manager(&mut self, task_manager: Arc<TaskManager>);
            pub fn add_interruption_strategy(&mut self, strategy: Arc<dyn BaseInterruptionStrategy>);
        }
    }

    delegate! {
        to self.frame_processor {
            pub async fn get_metrics(&self) -> FrameProcessorMetrics;
            pub async fn setup_all_processors(&self, setup: FrameProcessorSetup) -> Result<(), String>;
            pub async fn cleanup_all_processors(&self) -> Result<(), String>;
        }
    }

    delegate! {
        to self.frame_processor {
            pub fn link(&mut self, next: Arc<Mutex<FrameProcessor>>);
        }
    }

    delegate! {
        to self.frame_processor {
            pub async fn setup(&mut self, setup: FrameProcessorSetup) -> Result<(), String>;
        }
    }

    // Delegate the actual push_frame functionality to the inner frame_processor
    // Note: This overrides the placeholder push_frame method above
    delegate! {
        to self.frame_processor {
            pub async fn push_frame(&self, frame: FrameType, direction: FrameDirection) -> Result<(), String>;
            pub async fn push_frame_with_callback(&mut self, frame: FrameType, direction: FrameDirection, callback: Option<FrameCallback>) -> Result<(), String>;
        }
    }
}
