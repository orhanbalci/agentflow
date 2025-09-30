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
use crate::audio::resampler::{BaseAudioResampler, RubatoAudioResampler};
use crate::frames::{
    BotSpeakingFrame, BotStartedSpeakingFrame, BotStoppedSpeakingFrame, CancelFrame, EndFrame,
    Frame, FrameType, OutputAudioRawFrame, OutputImageRawFrame, OutputTransportReadyFrame,
    SpriteFrame, StartFrame, TransportMessageFrame, TransportMessageUrgentFrame,
};
use crate::processors::frame::{
    BaseInterruptionStrategy, FrameCallback, FrameDirection, FrameProcessor,
    FrameProcessorInterface, FrameProcessorMetrics, FrameProcessorSetup, FrameProcessorTrait,
};
use crate::task_manager::{TaskHandle, TaskManager};
use crate::transport::params::TransportParams;
use crate::{BaseClock, SystemClock};
use tokio::sync::mpsc;

// Type alias for frame queues per destination
type AudioQueue = mpsc::UnboundedSender<FrameType>;
type VideoQueue = mpsc::UnboundedSender<OutputImageRawFrame>;
type ClockQueue = mpsc::UnboundedSender<(u64, u64, FrameType)>;

/// Video image cycling for output
#[allow(dead_code)]
enum VideoImageCycle {
    Single(OutputImageRawFrame),
    Multiple(Vec<OutputImageRawFrame>, usize), // images and current index
}

impl VideoImageCycle {
    #[allow(dead_code)]
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
#[allow(dead_code)]
struct DestinationState {
    // Audio processing
    audio_buffer: Vec<u8>,
    audio_resampler: Box<dyn BaseAudioResampler>,
    audio_mixer: Option<Box<dyn BaseAudioMixer>>,

    // Video processing
    video_images: Option<VideoImageCycle>,
    video_start_time: Option<std::time::Instant>,
    video_frame_index: u64,
    video_frame_duration: f64,
    video_frame_reset: f64,

    // Tasks and queues
    audio_task: Option<TaskHandle>,
    video_task: Option<TaskHandle>,
    clock_task: Option<TaskHandle>,
    audio_queue: Option<AudioQueue>,
    video_queue: Option<VideoQueue>,
    clock_queue: Option<ClockQueue>,
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
    pub frame_processor: Arc<Mutex<FrameProcessor>>,
    sample_rate: AtomicU32,
    audio_chunk_size: AtomicU32,
    /// Map of destinations to their processing state
    destination_states: Mutex<HashMap<Option<String>, DestinationState>>,
    #[allow(dead_code)]
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
            frame_processor: Arc::new(Mutex::new(frame_processor)),
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
        frame: &EndFrame,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        // Mark as stopped
        self.stopped.store(true, Ordering::Relaxed);

        // Stop all destination states
        let mut states = self.destination_states.lock().await;
        for (destination, state) in states.iter_mut() {
            log::debug!("Stopping destination: {:?}", destination);

            // Let the sink tasks process the queue until they reach this EndFrame.
            // Put EndFrame in clock queue with infinite priority to ensure it's processed last
            if let Some(clock_sender) = &state.clock_queue {
                let _ = clock_sender.send((u64::MAX, frame.id(), FrameType::End(frame.clone())));
            }

            // Put EndFrame in audio queue
            if let Some(audio_sender) = &state.audio_queue {
                let _ = audio_sender.send(FrameType::End(frame.clone()));
            }

            // At this point we have enqueued an EndFrame and we need to wait for
            // that EndFrame to be processed by the audio and clock tasks. We
            // also need to wait for these tasks before cancelling the video task
            // because it might be still rendering.
            if let Some(task) = state.audio_task.take() {
                let _ = self
                    .cancel_task(&task, Some(std::time::Duration::from_secs(5)))
                    .await;
            }
            if let Some(task) = state.clock_task.take() {
                let _ = self
                    .cancel_task(&task, Some(std::time::Duration::from_secs(5)))
                    .await;
            }

            // Stop audio mixer after tasks are done
            if let Some(mixer) = &mut state.audio_mixer {
                if let Err(e) = mixer.stop().await {
                    log::error!("Failed to stop mixer for {:?}: {}", destination, e);
                }
            }

            // We can now cancel the video task.
            if let Some(task) = state.video_task.take() {
                let _ = self
                    .cancel_task(&task, Some(std::time::Duration::from_secs(5)))
                    .await;
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

            // Cancel tasks immediately using proper task management
            if let Some(task) = state.audio_task.take() {
                let _ = self
                    .cancel_task(&task, Some(std::time::Duration::from_secs(1)))
                    .await;
            }
            if let Some(task) = state.video_task.take() {
                let _ = self
                    .cancel_task(&task, Some(std::time::Duration::from_secs(1)))
                    .await;
            }
            if let Some(task) = state.clock_task.take() {
                let _ = self
                    .cancel_task(&task, Some(std::time::Duration::from_secs(1)))
                    .await;
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
            let video_frame_duration = 1.0 / self.params.video_out_framerate as f64;
            states.insert(
                None,
                DestinationState {
                    audio_buffer: Vec::new(),
                    audio_resampler: Box::new(RubatoAudioResampler::new()),
                    audio_mixer: None,
                    video_images: None,
                    video_start_time: None,
                    video_frame_index: 0,
                    video_frame_duration,
                    video_frame_reset: video_frame_duration * 5.0,
                    audio_task: None,
                    video_task: None,
                    clock_task: None,
                    audio_queue: None,
                    video_queue: None,
                    clock_queue: None,
                },
            );
        }

        // Create states for audio destinations
        for dest in &self.params.audio_out_destinations {
            if !states.contains_key(&Some(dest.clone())) {
                let video_frame_duration = 1.0 / self.params.video_out_framerate as f64;
                states.insert(
                    Some(dest.clone()),
                    DestinationState {
                        audio_buffer: Vec::new(),
                        audio_resampler: Box::new(RubatoAudioResampler::new()),
                        audio_mixer: None,
                        video_images: None,
                        video_start_time: None,
                        video_frame_index: 0,
                        video_frame_duration,
                        video_frame_reset: video_frame_duration * 5.0,
                        audio_task: None,
                        video_task: None,
                        clock_task: None,
                        audio_queue: None,
                        video_queue: None,
                        clock_queue: None,
                    },
                );
            }
        }

        // Create states for video destinations
        for dest in &self.params.video_out_destinations {
            if !states.contains_key(&Some(dest.clone())) {
                let video_frame_duration = 1.0 / self.params.video_out_framerate as f64;
                states.insert(
                    Some(dest.clone()),
                    DestinationState {
                        audio_buffer: Vec::new(),
                        audio_resampler: Box::new(RubatoAudioResampler::new()),
                        audio_mixer: None,
                        video_images: None,
                        video_start_time: None,
                        video_frame_index: 0,
                        video_frame_duration,
                        video_frame_reset: video_frame_duration * 5.0,
                        audio_task: None,
                        video_task: None,
                        clock_task: None,
                        audio_queue: None,
                        video_queue: None,
                        clock_queue: None,
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

    /// Start processing tasks for a specific destination.
    async fn start_destination_tasks(
        &self,
        destination: &Option<String>,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let mut states = self.destination_states.lock().await;
        if let Some(state) = states.get_mut(destination) {
            // Create all tasks first
            self.create_video_task(destination, state).await?;
            self.create_clock_task(destination, state).await?;
            self.create_audio_task(destination, state).await?;

            // Check if we have an audio mixer for our destination
            if let Some(_mixer_config) = &self.params.audio_out_mixer {
                // In a real implementation, this would check if mixer_config is a mapping
                // and get the mixer for this specific destination, or use default mixer
                // if destination is None. For now, we'll use a simplified approach.
                if destination.is_none() {
                    // Only use the default mixer if we are the default destination
                    // state.audio_mixer would be set here based on mixer_config
                }
            }

            // Start audio mixer if configured
            if let Some(mixer) = &mut state.audio_mixer {
                mixer
                    .start(self.sample_rate.load(Ordering::Relaxed))
                    .await?;
            }
        }
        Ok(())
    }

    /// Create the audio processing task for a destination.
    async fn create_audio_task(
        &self,
        destination: &Option<String>,
        state: &mut DestinationState,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        if state.audio_task.is_none() {
            // Create the audio queue
            let (audio_sender, audio_receiver) = mpsc::unbounded_channel::<FrameType>();
            state.audio_queue = Some(audio_sender);

            // Get parameters needed for the audio task
            let audio_out_10ms_chunks = self.params.audio_out_10ms_chunks;
            let destination_clone = destination.clone();
            let frame_processor = self.frame_processor.clone();

            // Create the audio task using FrameProcessor's create_task method
            let task_handle = self
                .create_task(
                    move |_ctx| async move {
                        log::debug!(
                            "Audio task started for destination: {:?}",
                            destination_clone
                        );

                        // Audio task handler implementation
                        Self::audio_task_handler(
                            audio_receiver,
                            audio_out_10ms_chunks,
                            destination_clone.clone(),
                            frame_processor,
                        )
                        .await;

                        log::debug!(
                            "Audio task finished for destination: {:?}",
                            destination_clone
                        );
                    },
                    Some("audio".to_string()),
                )
                .await
                .map_err(|e| {
                    Box::new(std::io::Error::new(std::io::ErrorKind::Other, e))
                        as Box<dyn std::error::Error + Send + Sync>
                })?;

            state.audio_task = Some(task_handle);
        }
        Ok(())
    }

    /// Create the clock/timing processing task for a destination.
    async fn create_clock_task(
        &self,
        destination: &Option<String>,
        state: &mut DestinationState,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        if state.clock_task.is_none() {
            // Create the clock priority queue
            let (clock_sender, mut clock_receiver) =
                mpsc::unbounded_channel::<(u64, u64, FrameType)>();
            state.clock_queue = Some(clock_sender);

            // Create the clock task using FrameProcessor's create_task method
            let destination_clone = destination.clone();
            let clock = self.get_clock();
            let frame_processor = self.frame_processor.clone();

            let task_handle = self
                .create_task(
                    move |_ctx| async move {
                        log::debug!(
                            "Clock task started for destination: {:?}",
                            destination_clone
                        );
                        // Main clock/timing task handler for timed frame delivery
                        let mut running = true;
                        while running {
                            if let Some((timestamp, _frame_id, frame)) = clock_receiver.recv().await {
                                // If we hit an EndFrame, we can finish right away
                                running = !matches!(frame, FrameType::End(_));

                                // If we have a frame we check its presentation timestamp. If it
                                // has already passed we process it, otherwise we wait until it's
                                // time to process it.
                                if running {
                                    match clock.get_time() {
                                        Ok(current_time) => {
                                            if timestamp > current_time {
                                                let wait_time_ns = timestamp - current_time;
                                                let wait_time_secs = wait_time_ns as f64 / 1_000_000_000.0;
                                                tokio::time::sleep(tokio::time::Duration::from_secs_f64(wait_time_secs)).await;
                                            }

                                            // Push frame downstream through the frame processor
                                            if let Err(e) = frame_processor.push_frame(frame, FrameDirection::Downstream).await {
                                                log::error!("Failed to push frame downstream in clock task: {}", e);
                                            }
                                        }
                                        Err(e) => {
                                            log::error!("Failed to get current time in clock task: {}", e);
                                        }
                                    }
                                }
                            } else {
                                // Channel closed, exit the loop
                                running = false;
                            }
                        }
                        log::debug!(
                            "Clock task finished for destination: {:?}",
                            destination_clone
                        );
                    },
                    Some("clock".to_string()),
                )
                .await
                .map_err(|e| {
                    Box::new(std::io::Error::new(std::io::ErrorKind::Other, e))
                        as Box<dyn std::error::Error + Send + Sync>
                })?;

            state.clock_task = Some(task_handle);
        }
        Ok(())
    }

    /// Create the video processing task for a destination.
    async fn create_video_task(
        &self,
        destination: &Option<String>,
        state: &mut DestinationState,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        if state.video_task.is_none() {
            // Create the video queue
            let (video_sender, mut video_receiver) =
                mpsc::unbounded_channel::<OutputImageRawFrame>();
            state.video_queue = Some(video_sender);

            // Get timing parameters from state
            let video_frame_duration = state.video_frame_duration;
            let video_frame_reset = state.video_frame_reset;

            // Create the video task using FrameProcessor's create_task method
            let destination_clone = destination.clone();
            let is_live = self.params.video_out_is_live;
            let desired_size = (self.params.video_out_width, self.params.video_out_height);

            // For now, we'll handle timing within the task without accessing shared state
            // In a full implementation, we'd need a different architecture to share timing state

            let task_handle = self
                .create_task(
                    move |_ctx| async move {
                        log::debug!(
                            "Video task started for destination: {:?}",
                            destination_clone
                        );

                        // Initialize video timing state
                        let mut video_start_time: Option<std::time::Instant> = None;
                        let mut video_frame_index: u64 = 0;
                        let frame_duration =
                            std::time::Duration::from_secs_f64(video_frame_duration);
                        let frame_reset_duration =
                            std::time::Duration::from_secs_f64(video_frame_reset);

                        let mut running = true;

                        while running {
                            if is_live {
                                // Live video mode - process frames from the queue
                                if let Some(frame) = video_receiver.recv().await {
                                    // Initialize start time on first frame
                                    if video_start_time.is_none() {
                                        video_start_time = Some(std::time::Instant::now());
                                    }

                                    // Draw the received frame with resizing if needed
                                    if let Err(e) = Self::draw_image_with_resize(
                                        frame,
                                        desired_size,
                                        &destination_clone,
                                    )
                                    .await
                                    {
                                        log::error!("Failed to draw image frame: {}", e);
                                    }

                                    // Update frame index
                                    video_frame_index += 1;

                                    // Control frame rate based on frame duration
                                    if let Some(start_time) = video_start_time {
                                        let expected_time = start_time
                                            + frame_duration * (video_frame_index as u32);
                                        let current_time = std::time::Instant::now();

                                        if current_time < expected_time {
                                            tokio::time::sleep(expected_time - current_time).await;
                                        }

                                        // Reset timing if we're too far behind (frame reset logic)
                                        if current_time > start_time + frame_reset_duration {
                                            video_start_time = Some(current_time);
                                            video_frame_index = 0;
                                        }
                                    }
                                } else {
                                    // Channel closed, exit
                                    running = false;
                                }
                            } else {
                                // Static video mode - use local timing and create placeholder frames
                                // Initialize start time on first iteration
                                if video_start_time.is_none() {
                                    video_start_time = Some(std::time::Instant::now());
                                }

                                // Create a simple placeholder frame
                                let placeholder_frame = OutputImageRawFrame::new(
                                    vec![0u8; 1024 * 768 * 3], // Simple RGB placeholder
                                    (1024, 768),
                                    Some("RGB".to_string()),
                                );

                                // Draw the frame with resizing if needed
                                if let Err(e) = Self::draw_image_with_resize(
                                    placeholder_frame,
                                    desired_size,
                                    &destination_clone,
                                )
                                .await
                                {
                                    log::error!("Failed to draw placeholder image frame: {}", e);
                                }

                                // Update frame index and timing
                                video_frame_index += 1;

                                // Check for frame reset logic
                                if let Some(start_time) = video_start_time {
                                    let current_time = std::time::Instant::now();
                                    if current_time > start_time + frame_reset_duration {
                                        video_start_time = Some(current_time);
                                        video_frame_index = 0;
                                    }
                                }

                                // Wait for the next frame interval
                                tokio::time::sleep(frame_duration).await;
                            }
                        }

                        log::debug!(
                            "Video task finished for destination: {:?}",
                            destination_clone
                        );
                    },
                    Some("video".to_string()),
                )
                .await
                .map_err(|e| {
                    Box::new(std::io::Error::new(std::io::ErrorKind::Other, e))
                        as Box<dyn std::error::Error + Send + Sync>
                })?;

            state.video_task = Some(task_handle);
        }
        Ok(())
    }

    /// Static helper for drawing image with resizing (for use in video tasks)
    ///
    /// This static method can be called from async tasks that don't have access to self
    pub async fn draw_image_with_resize(
        frame: OutputImageRawFrame,
        desired_size: (u32, u32),
        destination: &Option<String>,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let current_size = frame.image_frame.size;

        let processed_frame = if current_size != desired_size {
            log::debug!(
                "Frame size {}x{} does not match desired size {}x{}, resizing needed for destination: {:?}",
                current_size.0, current_size.1, desired_size.0, desired_size.1, destination
            );

            // Use async resize with thread pool for CPU-intensive work
            let resized_data = Self::resize_frame_data_async(
                frame.image_frame.image.clone(),
                current_size,
                desired_size,
                frame.image_frame.format.clone(),
            )
            .await?;

            OutputImageRawFrame::new(resized_data, desired_size, frame.image_frame.format.clone())
        } else {
            frame
        };

        // Log the processed frame (in a real implementation, this would write to output)
        log::debug!(
            "Processed image frame for destination: {:?}, final size: {}x{}, format: {:?}",
            destination,
            processed_frame.image_frame.size.0,
            processed_frame.image_frame.size.1,
            processed_frame.image_frame.format
        );

        Ok(())
    }

    /// Draw/render an image frame with resizing if needed.
    ///
    /// Args:
    ///     frame: The image frame to draw.
    pub async fn _draw_image(
        &self,
        frame: OutputImageRawFrame,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        // Get desired output size from params
        let desired_size = (self.params.video_out_width, self.params.video_out_height);
        let current_size = frame.image_frame.size;

        let processed_frame = if current_size != desired_size {
            // TODO: we should refactor in the future to support dynamic resolutions
            // which is kind of what happens in P2P connections.
            // We need to add support for that inside the DailyTransport

            log::debug!(
                "Frame size {}x{} does not match desired size {}x{}, resizing needed",
                current_size.0,
                current_size.1,
                desired_size.0,
                desired_size.1
            );

            // Use async resize with thread pool for CPU-intensive work
            let resized_data = Self::resize_frame_data_async(
                frame.image_frame.image.clone(),
                current_size,
                desired_size,
                frame.image_frame.format.clone(),
            )
            .await?;

            OutputImageRawFrame::new(resized_data, desired_size, frame.image_frame.format.clone())
        } else {
            frame
        };

        // Write the processed frame to the video output
        self.write_video_frame(&processed_frame).await?;

        Ok(())
    }

    /// Resize frame data using the image crate (real implementation using thread pool)
    ///
    /// This function performs actual image resizing using the `image` crate
    /// with high-quality algorithms, running on a background thread pool
    /// to avoid blocking the async runtime.
    pub async fn resize_frame_data_async(
        data: Vec<u8>,
        current_size: (u32, u32),
        desired_size: (u32, u32),
        format: Option<String>,
    ) -> Result<Vec<u8>, Box<dyn std::error::Error + Send + Sync>> {
        log::debug!(
            "Async resizing frame data from {}x{} to {}x{}, original size: {} bytes, format: {:?}",
            current_size.0,
            current_size.1,
            desired_size.0,
            desired_size.1,
            data.len(),
            format
        );

        // Use tokio::task::spawn_blocking to run CPU-intensive work on a thread pool
        let resized_data = tokio::task::spawn_blocking(move || {
            Self::resize_frame_data_blocking(data, current_size, desired_size, format)
        })
        .await
        .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)??;

        log::debug!(
            "Async resize completed: output size {} bytes",
            resized_data.len()
        );

        Ok(resized_data)
    }

    /// Blocking image resize operation using the image crate (runs on thread pool)
    ///
    /// This performs the actual image processing work using the `image` crate.
    /// Equivalent to Python: image = Image.frombytes(format, size, data); resized = image.resize(desired_size)
    fn resize_frame_data_blocking(
        data: Vec<u8>,
        current_size: (u32, u32),
        desired_size: (u32, u32),
        format: Option<String>,
    ) -> Result<Vec<u8>, Box<dyn std::error::Error + Send + Sync>> {
        let (width, height) = current_size;
        let input_size = data.len();

        // Step 1: Python equivalent: image = Image.frombytes(frame.format, frame.size, frame.image)
        let dynamic_img = match format.as_deref().unwrap_or("RGB") {
            "RGB" => {
                let img =
                    image::RgbImage::from_raw(width, height, data).ok_or("Invalid RGB data")?;
                image::DynamicImage::ImageRgb8(img)
            }
            "RGBA" => {
                let img =
                    image::RgbaImage::from_raw(width, height, data).ok_or("Invalid RGBA data")?;
                image::DynamicImage::ImageRgba8(img)
            }
            "BGR" => {
                let mut rgb_data = data;
                rgb_data
                    .chunks_exact_mut(3)
                    .for_each(|chunk| chunk.swap(0, 2));
                let img =
                    image::RgbImage::from_raw(width, height, rgb_data).ok_or("Invalid BGR data")?;
                image::DynamicImage::ImageRgb8(img)
            }
            "BGRA" => {
                let mut rgba_data = data;
                rgba_data
                    .chunks_exact_mut(4)
                    .for_each(|chunk| chunk.swap(0, 2));
                let img = image::RgbaImage::from_raw(width, height, rgba_data)
                    .ok_or("Invalid BGRA data")?;
                image::DynamicImage::ImageRgba8(img)
            }
            _ => return Err(format!("Unsupported format: {:?}", format).into()),
        };

        // Step 2: Python equivalent: resized_image = image.resize(desired_size)
        let resized = dynamic_img.resize(
            desired_size.0,
            desired_size.1,
            image::imageops::FilterType::Lanczos3,
        );

        // Step 3: Convert back to original format (Python handles this automatically)
        let output_data = match format.as_deref().unwrap_or("RGB") {
            "RGB" => resized.to_rgb8().into_raw(),
            "RGBA" => resized.to_rgba8().into_raw(),
            "BGR" => {
                let mut data = resized.to_rgb8().into_raw();
                data.chunks_exact_mut(3).for_each(|chunk| chunk.swap(0, 2));
                data
            }
            "BGRA" => {
                let mut data = resized.to_rgba8().into_raw();
                data.chunks_exact_mut(4).for_each(|chunk| chunk.swap(0, 2));
                data
            }
            _ => return Err("Unsupported output format".into()),
        };

        log::debug!(
            "Image resize: {} -> {} bytes",
            input_size,
            output_data.len()
        );
        Ok(output_data)
    }

    /// Audio task handler implementation equivalent to Python _audio_task_handler
    async fn audio_task_handler(
        mut audio_receiver: mpsc::UnboundedReceiver<FrameType>,
        audio_out_10ms_chunks: u32,
        destination: Option<String>,
        frame_processor: Arc<Mutex<FrameProcessor>>,
    ) {
        // Constants for bot speaking detection
        const BOT_VAD_STOP_SECS: f64 = 0.5; // 500ms of silence before bot stops speaking
        let total_chunk_ms = audio_out_10ms_chunks * 10;
        let bot_speaking_chunk_period = std::cmp::max((200.0 / total_chunk_ms as f64) as u32, 1);

        let mut bot_speaking_counter = 0u32;
        let mut speech_last_speaking_time = std::time::Instant::now();
        let mut is_bot_speaking = false;

        log::debug!(
            "Audio task handler started for destination: {:?}",
            destination
        );

        while let Some(frame) = audio_receiver.recv().await {
            // Determine if bot is speaking based on frame type
            let mut is_speaking = false;

            // Check for audio frames that indicate the bot is speaking
            // TTSAudioRawFrame specifically indicates TTS-generated speech
            match &frame {
                FrameType::TTSAudioRaw(_) => {
                    // TTS frames always indicate speaking
                    is_speaking = true;
                    speech_last_speaking_time = std::time::Instant::now();
                }
                FrameType::SpeechOutputAudioRaw(_) => {
                    // Speech frames always indicate speaking
                    is_speaking = true;
                    speech_last_speaking_time = std::time::Instant::now();
                }
                FrameType::OutputAudioRaw(_) => {
                    // Regular audio frames - assume speaking for now
                    // In practice, you'd check for silence using VAD
                    is_speaking = true;
                    speech_last_speaking_time = std::time::Instant::now();
                }
                _ => {
                    // Other frame types don't indicate speaking
                }
            }

            // Handle bot speaking state changes
            if is_speaking && !is_bot_speaking {
                Self::bot_started_speaking(&frame_processor, &destination).await;
                is_bot_speaking = true;
            }

            // Send periodic BotSpeaking frames
            if is_speaking {
                if bot_speaking_counter % bot_speaking_chunk_period == 0 {
                    Self::send_bot_speaking_frame(&frame_processor).await;
                    bot_speaking_counter = 0;
                }
                bot_speaking_counter += 1;
            } else if is_bot_speaking {
                // Check if we should stop speaking due to silence
                let silence_duration = speech_last_speaking_time.elapsed().as_secs_f64();
                if silence_duration > BOT_VAD_STOP_SECS {
                    Self::bot_stopped_speaking(&frame_processor, &destination).await;
                    is_bot_speaking = false;
                }
            }

            // Check for EndFrame
            if matches!(frame, FrameType::End(_)) {
                log::debug!(
                    "Audio task handler received EndFrame for destination: {:?}",
                    destination
                );
                break;
            }

            // Handle the frame processing
            if let Err(e) = Self::handle_audio_frame_in_task(&frame, &frame_processor).await {
                log::error!(
                    "Error handling audio frame in task for destination {:?}: {}",
                    destination,
                    e
                );
            }
        }

        log::debug!(
            "Audio task handler finished for destination: {:?}",
            destination
        );
    }

    /// Handle bot started speaking event
    async fn bot_started_speaking(
        frame_processor: &Arc<Mutex<FrameProcessor>>,
        destination: &Option<String>,
    ) {
        log::debug!("Bot started speaking for destination: {:?}", destination);
        let frame = BotStartedSpeakingFrame::new();
        if let Err(e) = frame_processor
            .push_frame(
                FrameType::BotStartedSpeaking(frame.clone()),
                FrameDirection::Upstream,
            )
            .await
        {
            log::error!("Failed to push BotStartedSpeaking frame upstream: {}", e);
        }
    }

    /// Handle bot stopped speaking event
    async fn bot_stopped_speaking(
        frame_processor: &Arc<Mutex<FrameProcessor>>,
        destination: &Option<String>,
    ) {
        log::debug!("Bot stopped speaking for destination: {:?}", destination);
        let frame = BotStoppedSpeakingFrame::new();
        if let Err(e) = frame_processor
            .push_frame(
                FrameType::BotStoppedSpeaking(frame.clone()),
                FrameDirection::Upstream,
            )
            .await
        {
            log::error!("Failed to push BotStoppedSpeaking frame upstream: {}", e);
        }
    }

    /// Send periodic bot speaking frame
    async fn send_bot_speaking_frame(frame_processor: &Arc<Mutex<FrameProcessor>>) {
        let frame = BotSpeakingFrame::new();

        // Push downstream
        if let Err(e) = frame_processor
            .push_frame(
                FrameType::BotSpeaking(frame.clone()),
                FrameDirection::Downstream,
            )
            .await
        {
            log::error!("Failed to push BotSpeaking frame downstream: {}", e);
        }

        // Push upstream
        if let Err(e) = frame_processor
            .push_frame(FrameType::BotSpeaking(frame), FrameDirection::Upstream)
            .await
        {
            log::error!("Failed to push BotSpeaking frame upstream: {}", e);
        }
    }

    /// Handle audio frame processing within the task
    async fn handle_audio_frame_in_task(
        frame: &FrameType,
        _frame_processor: &Arc<Mutex<FrameProcessor>>,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        // Simplified implementation - in a real implementation this would
        // handle writing to the transport and pushing frames downstream

        // For now, just log the frame processing
        log::debug!(
            "Processing audio frame: {} (id: {})",
            frame.name(),
            frame.id()
        );

        // In the Python implementation, this would:
        // 1. Try to write the audio frame to the transport
        // 2. Based on success/failure, decide whether to push downstream
        // 3. Push the frame downstream if writing succeeded

        // For this simplified implementation, we assume success
        let _push_downstream = true;

        Ok(())
    }

    /// Handle frames by delegating to appropriate destination processing.
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
        drop(states); // Release the lock early

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
            FrameType::TTSAudioRaw(tts_frame) => {
                log::debug!(
                    "Handling TTS audio frame for destination: {:?}",
                    transport_destination
                );
                // TTS frames wrap OutputAudioRawFrame, so we can delegate to the same handler
                self.handle_audio_frame(&tts_frame.output_audio_frame, &transport_destination)
                    .await?;
            }
            FrameType::SpeechOutputAudioRaw(speech_frame) => {
                log::debug!(
                    "Handling speech audio frame for destination: {:?}",
                    transport_destination
                );
                // Speech frames wrap OutputAudioRawFrame, so we can delegate to the same handler
                self.handle_audio_frame(&speech_frame.output_audio_frame, &transport_destination)
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

    /// Handle interruption frames - simplified implementation
    async fn handle_interruption_frame(
        &self,
        frame: &FrameType,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        log::debug!("Handling interruption frame: {}", frame.name());
        // Simplified implementation - in practice this would handle interruptions
        // such as stopping current audio/video processing, clearing buffers, etc.
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
                    frame.audio_frame.audio.clone(),
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
                    let _ = audio_sender.send(FrameType::OutputAudioRaw(chunk));
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

    // ...existing code...
}

#[async_trait]
impl FrameProcessorTrait for BaseOutputTransport {
    fn name(&self) -> &str {
        // For async access to the frame processor name, we'll need to handle this differently
        // For now, return a default name - in practice you might want to cache this
        "BaseOutputTransport"
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
                // Handle the frame by delegating to appropriate destination processing
                // This is a simplified implementation - in practice would call _handle_frame
                if let Err(e) = self.handle_interruption_frame(&frame).await {
                    log::error!("Error handling interruption frame: {}", e);
                }
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
            FrameType::TTSAudioRaw(_) => {
                self._handle_frame(frame.clone())
                    .await
                    .map_err(|e| e.to_string())?;
            }
            FrameType::SpeechOutputAudioRaw(_) => {
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

// Implement the FrameProcessorInterface trait for BaseOutputTransport
// Use delegate macro to simplify delegation to frame_processor
#[async_trait]
impl FrameProcessorInterface for BaseOutputTransport {
    delegate! {
        to self.frame_processor {
            fn id(&self) -> u64;
            fn name(&self) -> &str;
            fn is_started(&self) -> bool;
            fn is_cancelling(&self) -> bool;
            fn set_allow_interruptions(&mut self, allow: bool);
            fn set_enable_metrics(&mut self, enable: bool);
            fn set_enable_usage_metrics(&mut self, enable: bool);
            fn set_report_only_initial_ttfb(&mut self, report: bool);
            fn set_clock(&mut self, clock: Arc<dyn BaseClock>);
            fn set_task_manager(&mut self, task_manager: Arc<TaskManager>);
            fn add_processor(&mut self, processor: Arc<Mutex<FrameProcessor>>);
            fn clear_processors(&mut self);
            fn is_compound_processor(&self) -> bool;
            fn processor_count(&self) -> usize;
            fn get_processor(&self, index: usize) -> Option<&Arc<Mutex<FrameProcessor>>>;
            fn processors(&self) -> &Vec<Arc<Mutex<FrameProcessor>>>;
            fn link(&mut self, next: Arc<Mutex<FrameProcessor>>);
            fn add_interruption_strategy(&mut self, strategy: Arc<dyn BaseInterruptionStrategy>);
        }
    }

    // Async methods must be implemented manually as delegate macro doesn't support async traits
    async fn create_task<F, Fut>(
        &self,
        future: F,
        name: Option<String>,
    ) -> Result<TaskHandle, String>
    where
        F: FnOnce(crate::task_manager::TaskContext) -> Fut + Send + 'static,
        Fut: std::future::Future<Output = ()> + Send + 'static,
    {
        self.frame_processor.create_task(future, name).await
    }

    async fn cancel_task(
        &self,
        task: &TaskHandle,
        timeout: Option<std::time::Duration>,
    ) -> Result<(), String> {
        self.frame_processor.cancel_task(task, timeout).await
    }

    async fn get_metrics(&self) -> FrameProcessorMetrics {
        self.frame_processor.get_metrics().await
    }

    async fn setup(&mut self, setup: FrameProcessorSetup) -> Result<(), String> {
        self.frame_processor.setup(setup).await
    }

    async fn setup_all_processors(&self, setup: FrameProcessorSetup) -> Result<(), String> {
        self.frame_processor.setup_all_processors(setup).await
    }

    async fn cleanup_all_processors(&self) -> Result<(), String> {
        self.frame_processor.cleanup_all_processors().await
    }

    async fn push_frame(&self, frame: FrameType, direction: FrameDirection) -> Result<(), String> {
        self.frame_processor.push_frame(frame, direction).await
    }

    async fn push_frame_with_callback(
        &mut self,
        frame: FrameType,
        direction: FrameDirection,
        callback: Option<FrameCallback>,
    ) -> Result<(), String> {
        self.frame_processor
            .push_frame_with_callback(frame, direction, callback)
            .await
    }
}
