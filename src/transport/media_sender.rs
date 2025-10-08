//! MediaSender implementation for handling media streaming to specific destinations.
//!
//! This module provides the MediaSender struct that manages audio and video output
//! processing including buffering, timing, mixing, and frame delivery for individual
//! output destinations using a task-based approach.

use std::sync::Arc;
use tokio::sync::mpsc;
use tokio::sync::Mutex;

use crate::audio::mixer::BaseAudioMixer;
use crate::audio::resampler::{BaseAudioResampler, RubatoAudioResampler};
use crate::audio::utils::is_silence;
use crate::frames::{
    BotSpeakingFrame, BotStartedSpeakingFrame, BotStoppedSpeakingFrame, EndFrame, Frame, FrameType,
    OutputAudioRawFrame, OutputImageRawFrame, SpriteFrame, StartFrame,
};
use crate::processors::frame::{FrameDirection, FrameProcessorInterface};
use crate::task_manager::TaskHandle;
use crate::transport::output::BaseOutputTransport;
use crate::transport::output::TransportMessageFrameType;
use crate::transport::params::TransportParams;

// Type alias for frame queues per destination
type AudioQueue = mpsc::UnboundedSender<FrameType>;
type VideoQueue = mpsc::UnboundedSender<OutputImageRawFrame>;
type ClockQueue = mpsc::UnboundedSender<(u64, u64, FrameType)>;

/// Internal shared state for MediaSender
#[allow(dead_code)]
pub struct MediaSenderInner {
    pub destination: Option<String>,
    pub sample_rate: u32,
    pub audio_chunk_size: u32,
    pub params: TransportParams,

    // Audio processing state
    pub audio_buffer: Vec<u8>,
    pub audio_resampler: Box<dyn BaseAudioResampler>,
    pub audio_mixer: Option<Box<dyn BaseAudioMixer>>,

    // Video processing state
    pub video_start_time: Option<std::time::Instant>,
    pub video_frame_index: u64,
    pub video_frame_duration: f64,
    pub video_frame_reset: f64,
    pub video_images: Option<std::iter::Cycle<std::vec::IntoIter<OutputImageRawFrame>>>,

    // Bot speaking state
    pub bot_speaking: bool,

    // Task handles
    pub audio_task: Option<TaskHandle>,
    pub video_task: Option<TaskHandle>,
    pub clock_task: Option<TaskHandle>,

    // Queues
    pub audio_queue: Option<AudioQueue>,
    pub video_queue: Option<VideoQueue>,
    pub clock_queue: Option<ClockQueue>,

    // Transport reference for direct transport calls
    pub transport: std::sync::Weak<BaseOutputTransport>,
}

/// MediaSender handles media streaming for a specific destination.
///
/// Manages audio and video output processing including buffering, timing,
/// mixing, and frame delivery for a single output destination.
pub struct MediaSender {
    pub inner: Arc<Mutex<MediaSenderInner>>,
    pub transport: std::sync::Weak<BaseOutputTransport>,
}

impl MediaSender {
    /// Create a new MediaSender for a specific destination
    pub fn new(
        destination: Option<String>,
        sample_rate: u32,
        audio_chunk_size: u32,
        params: TransportParams,
        transport: std::sync::Weak<BaseOutputTransport>,
    ) -> Self {
        let video_frame_duration = 1.0 / params.video_out_framerate as f64;

        let inner = MediaSenderInner {
            destination: destination.clone(),
            sample_rate,
            audio_chunk_size,
            params: params.clone(),
            audio_buffer: Vec::new(),
            audio_resampler: Box::new(RubatoAudioResampler::new()),
            audio_mixer: None,
            video_start_time: None,
            video_frame_index: 0,
            video_frame_duration,
            video_frame_reset: video_frame_duration * 5.0,
            video_images: None,
            bot_speaking: false,
            audio_task: None,
            video_task: None,
            clock_task: None,
            audio_queue: None,
            video_queue: None,
            clock_queue: None,
            transport: transport.clone(),
        };

        Self {
            inner: Arc::new(Mutex::new(inner)),
            transport,
        }
    }

    /// Start the media sender and spawn worker tasks
    pub async fn start(
        &mut self,
        _frame: &StartFrame,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        {
            let mut inner_guard = self.inner.lock().await;
            inner_guard.audio_buffer.clear();

            // Create all tasks following the Python implementation pattern
            if let Some(transport) = self.transport.upgrade() {
                // Create video task
                self.create_video_task(&transport, &mut inner_guard).await?;

                // Create clock task
                self.create_clock_task(&transport, &mut inner_guard).await?;

                // Create audio task
                self.create_audio_task(&transport, &mut inner_guard).await?;
            }

            // Setup audio mixer if configured (following Python implementation)
            if let Some(_mixer_config) = &inner_guard.params.audio_out_mixer {
                // Check if we have an audio mixer for our destination
                // In Python: if isinstance(self._params.audio_out_mixer, Mapping):
                //     self._mixer = self._params.audio_out_mixer.get(self._destination, None)
                // elif not self._destination:
                //     # Only use the default mixer if we are the default destination.
                //     self._mixer = self._params.audio_out_mixer
                if inner_guard.destination.is_none() {
                    // Only use the default mixer if we are the default destination
                    // inner_guard.audio_mixer would be set here based on mixer_config
                }
            }

            // Start audio mixer if configured
            let sample_rate = inner_guard.sample_rate;
            if let Some(mixer) = &mut inner_guard.audio_mixer {
                mixer.start(sample_rate).await?;
            }
        }

        Ok(())
    }

    /// Stop the media sender by cancelling tasks
    pub async fn stop(
        &mut self,
        frame: &EndFrame,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        {
            let inner_guard = self.inner.lock().await;

            // Let the sink tasks process the queue until they reach this EndFrame.
            // Send EndFrame to clock queue with infinite timestamp (Python: await self._clock_queue.put((float("inf"), frame.id, frame)))
            if let Some(ref clock_sender) = inner_guard.clock_queue {
                let _ = clock_sender.send((u64::MAX, frame.id(), FrameType::End(frame.clone())));
            }

            // Send EndFrame to audio queue (Python: await self._audio_queue.put(frame))
            if let Some(ref audio_sender) = inner_guard.audio_queue {
                let _ = audio_sender.send(FrameType::End(frame.clone()));
            }
        }

        // Get transport reference for task management
        let transport = self.transport.upgrade();

        // At this point we have enqueued an EndFrame and we need to wait for
        // that EndFrame to be processed by the audio and clock tasks.
        // Since we don't have a wait_for_task method, we'll use a short delay
        // to allow the tasks to process the EndFrame naturally.
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        {
            let mut inner_guard = self.inner.lock().await;

            // Stop audio mixer (Python: if self._mixer: await self._mixer.stop())
            if let Some(mixer) = &mut inner_guard.audio_mixer {
                let _ = mixer.stop().await;
            }

            // We can now cancel the video task (Python: await self._cancel_video_task())
            if let Some(transport) = &transport {
                if let Some(ref video_task) = inner_guard.video_task {
                    let _ = transport
                        .cancel_task(video_task, Some(std::time::Duration::from_millis(1000)))
                        .await;
                }
            }

            // Clear task handles
            inner_guard.audio_task = None;
            inner_guard.video_task = None;
            inner_guard.clock_task = None;
        }

        Ok(())
    }

    /// Cancel the media sender immediately
    pub async fn cancel(
        &mut self,
        _frame: &crate::frames::CancelFrame,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        // Since we are cancelling everything it doesn't matter what task we cancel first.
        self.cancel_audio_task().await?;
        self.cancel_clock_task().await?;
        self.cancel_video_task().await?;

        Ok(())
    }

    /// Cancel the audio processing task
    async fn cancel_audio_task(&mut self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let mut inner_guard = self.inner.lock().await;

        if let Some(transport) = self.transport.upgrade() {
            if let Some(ref audio_task) = inner_guard.audio_task {
                let _ = transport
                    .cancel_task(audio_task, Some(std::time::Duration::from_millis(100)))
                    .await;
            }
        }

        // Clear audio task handle
        inner_guard.audio_task = None;

        Ok(())
    }

    /// Cancel the clock processing task
    async fn cancel_clock_task(&mut self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let mut inner_guard = self.inner.lock().await;

        if let Some(transport) = self.transport.upgrade() {
            if let Some(ref clock_task) = inner_guard.clock_task {
                let _ = transport
                    .cancel_task(clock_task, Some(std::time::Duration::from_millis(100)))
                    .await;
            }
        }

        // Clear clock task handle
        inner_guard.clock_task = None;

        Ok(())
    }

    /// Cancel the video processing task
    async fn cancel_video_task(&mut self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let mut inner_guard = self.inner.lock().await;

        if let Some(transport) = self.transport.upgrade() {
            if let Some(ref video_task) = inner_guard.video_task {
                let _ = transport
                    .cancel_task(video_task, Some(std::time::Duration::from_millis(100)))
                    .await;
            }
        }

        // Clear video task handle
        inner_guard.video_task = None;

        Ok(())
    }

    /// Handle audio frames by sending to audio queue
    pub async fn handle_audio_frame(
        &self,
        frame: &OutputAudioRawFrame,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let mut inner_guard = self.inner.lock().await;

        if !inner_guard.params.audio_out_enabled {
            return Ok(());
        }

        // We might need to resample if incoming audio doesn't match the
        // transport sample rate.
        let resampled = inner_guard
            .audio_resampler
            .resample(
                frame.audio_frame.audio.clone(),
                frame.audio_frame.sample_rate,
                inner_guard.sample_rate,
            )
            .await?;

        // Add resampled audio to buffer
        inner_guard.audio_buffer.extend(resampled);

        // Process audio in chunks
        while inner_guard.audio_buffer.len() >= inner_guard.audio_chunk_size as usize {
            // Extract chunk from buffer
            let chunk_size = inner_guard.audio_chunk_size as usize;
            let chunk_data: Vec<u8> = inner_guard.audio_buffer.drain(..chunk_size).collect();

            // Create new audio frame chunk with transport destination
            let mut chunk = OutputAudioRawFrame::new(
                chunk_data,
                inner_guard.sample_rate,
                frame.audio_frame.num_channels,
            );
            chunk.set_transport_destination(inner_guard.destination.clone());

            // Send chunk to audio queue if available
            if let Some(ref audio_sender) = inner_guard.audio_queue {
                let _ = audio_sender.send(FrameType::OutputAudioRaw(chunk));
            }
        }

        Ok(())
    }

    /// Handle incoming image frames for video output (OutputImageRawFrame or SpriteFrame)
    pub async fn handle_image_frame(
        &self,
        frame: FrameType,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let inner_guard = self.inner.lock().await;

        if !inner_guard.params.video_out_enabled {
            return Ok(());
        }

        match frame {
            FrameType::OutputImageRaw(image_frame) => {
                if inner_guard.params.video_out_is_live {
                    // Live mode: send frame to video queue (Python: await self._video_queue.put(frame))
                    if let Some(ref video_sender) = inner_guard.video_queue {
                        let _ = video_sender.send(image_frame);
                    }
                } else {
                    // Non-live mode: set as single video image (Python: await self._set_video_image(frame))
                    drop(inner_guard); // Release lock before calling set_video_image
                    self.set_video_image(image_frame).await;
                }
            }
            FrameType::Sprite(sprite_frame) => {
                // Set multiple video images (Python: await self._set_video_images(frame.images))
                drop(inner_guard); // Release lock before calling set_video_images
                self.set_video_images(sprite_frame.images).await;
            }
            _ => {
                // Ignore other frame types
            }
        }

        Ok(())
    }

    /// Handle interruption events by restarting tasks and clearing buffers
    pub async fn handle_interruptions(
        &mut self,
        _frame: FrameType,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        // Check if interruptions are allowed on the transport
        if let Some(transport) = self.transport.upgrade() {
            if !transport.interruptions_allowed().await {
                return Ok(());
            }
        } else {
            return Ok(());
        }

        // Cancel tasks
        self.cancel_audio_task().await?;
        self.cancel_clock_task().await?;
        self.cancel_video_task().await?;

        // Create tasks
        if let Some(transport) = self.transport.upgrade() {
            let mut inner_guard = self.inner.lock().await;

            self.create_video_task(&transport, &mut inner_guard).await?;
            self.create_clock_task(&transport, &mut inner_guard).await?;
            self.create_audio_task(&transport, &mut inner_guard).await?;
        }

        // Let's send a bot stopped speaking if we have to
        Self::bot_stopped_speaking(&self.inner, &self.transport).await;

        Ok(())
    }

    /// Handle mixer control frames
    pub async fn handle_mixer_control_frame(
        &self,
        _frame: FrameType,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let inner_guard = self.inner.lock().await;

        // Pass frame to audio mixer if available (Python: if self._mixer: await self._mixer.process_frame(frame))
        if let Some(_mixer) = &inner_guard.audio_mixer {
            // TODO: Add MixerControl variant to FrameType enum and process_frame method to BaseAudioMixer trait
            // In the Python implementation, this would be:
            // await self._mixer.process_frame(frame)

            // For now, we'll handle mixer-related frames generically
            log::debug!("Processing mixer control frame with mixer");
            // Once the proper variants and methods are added, this would be:
            // mixer.process_frame(frame).await?;
        } else {
            log::debug!("No mixer available to process mixer control frame");
        }

        Ok(())
    }

    /// Handle frames with presentation timestamps
    pub async fn handle_timed_frame(
        &self,
        frame: FrameType,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let inner_guard = self.inner.lock().await;

        // Send frame to clock queue with its presentation timestamp (Python: await self._clock_queue.put((frame.pts, frame.id, frame)))
        if let Some(ref clock_sender) = inner_guard.clock_queue {
            let pts = frame.pts().unwrap_or(0); // Use frame's presentation timestamp
            let frame_id = frame.id();
            let _ = clock_sender.send((pts, frame_id, frame));
        }

        Ok(())
    }

    /// Handle frames that need synchronized processing
    pub async fn handle_sync_frame(
        &self,
        frame: FrameType,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let inner_guard = self.inner.lock().await;

        // Send frame to audio queue for synchronized processing (Python: await self._audio_queue.put(frame))
        if let Some(ref audio_sender) = inner_guard.audio_queue {
            let _ = audio_sender.send(frame);
        }

        Ok(())
    }

    /// Set a single video image for cycling output (equivalent to Python _set_video_image)
    pub async fn set_video_image(&self, image: OutputImageRawFrame) {
        let mut inner_guard = self.inner.lock().await;
        inner_guard.video_images = Some(vec![image].into_iter().cycle());
    }

    /// Set multiple video images for cycling output (equivalent to Python _set_video_images)
    pub async fn set_video_images(&self, images: Vec<OutputImageRawFrame>) {
        let mut inner_guard = self.inner.lock().await;
        inner_guard.video_images = Some(images.into_iter().cycle());
    }

    /// Send a timed frame to the clock queue for scheduled delivery
    /// This is equivalent to putting frames into the Python _clock_queue
    pub async fn send_timed_frame(
        &self,
        timestamp: u64,
        frame: FrameType,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let inner_guard = self.inner.lock().await;

        if let Some(ref clock_sender) = inner_guard.clock_queue {
            // Use timestamp as priority (earlier timestamps have higher priority)
            let priority = timestamp;
            clock_sender
                .send((timestamp, priority, frame))
                .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)?;
        }

        Ok(())
    }

    /// Create the video processing task if video output is enabled
    async fn create_video_task(
        &self,
        transport: &Arc<BaseOutputTransport>,
        inner_guard: &mut MediaSenderInner,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        if !inner_guard.video_task.is_some() && inner_guard.params.video_out_enabled {
            let (video_queue_tx, video_queue_rx) = mpsc::unbounded_channel();
            inner_guard.video_queue = Some(video_queue_tx);

            let inner_clone = Arc::clone(&self.inner);
            let video_task = transport
                .create_task(
                    move |_ctx| async move {
                        Self::video_task_handler(inner_clone, video_queue_rx).await
                    },
                    Some(format!("VideoTask-{:?}", inner_guard.destination)),
                )
                .await
                .map_err(|e| {
                    Box::new(std::io::Error::new(std::io::ErrorKind::Other, e))
                        as Box<dyn std::error::Error + Send + Sync>
                })?;
            inner_guard.video_task = Some(video_task);
        }
        Ok(())
    }

    /// Create the clock processing task
    async fn create_clock_task(
        &self,
        transport: &Arc<BaseOutputTransport>,
        inner_guard: &mut MediaSenderInner,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        if !inner_guard.clock_task.is_some() {
            let (clock_queue_tx, clock_queue_rx) = mpsc::unbounded_channel();
            inner_guard.clock_queue = Some(clock_queue_tx);

            let inner_clone = Arc::clone(&self.inner);
            let transport_weak = Arc::downgrade(transport);
            let clock_task = transport
                .create_task(
                    move |_ctx| async move {
                        Self::clock_task_handler(inner_clone, clock_queue_rx, transport_weak).await
                    },
                    Some(format!("ClockTask-{:?}", inner_guard.destination)),
                )
                .await
                .map_err(|e| {
                    Box::new(std::io::Error::new(std::io::ErrorKind::Other, e))
                        as Box<dyn std::error::Error + Send + Sync>
                })?;
            inner_guard.clock_task = Some(clock_task);
        }
        Ok(())
    }

    /// Create the audio processing task if audio output is enabled
    async fn create_audio_task(
        &self,
        transport: &Arc<BaseOutputTransport>,
        inner_guard: &mut MediaSenderInner,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        if !inner_guard.audio_task.is_some() && inner_guard.params.audio_out_enabled {
            let (audio_queue_tx, audio_queue_rx) = mpsc::unbounded_channel();
            inner_guard.audio_queue = Some(audio_queue_tx);

            let inner_clone = Arc::clone(&self.inner);
            let transport_weak = Arc::downgrade(transport);
            let audio_task = transport
                .create_task(
                    move |_ctx| async move {
                        Self::audio_task_handler(inner_clone, audio_queue_rx, transport_weak).await
                    },
                    Some(format!("AudioTask-{:?}", inner_guard.destination)),
                )
                .await
                .map_err(|e| {
                    Box::new(std::io::Error::new(std::io::ErrorKind::Other, e))
                        as Box<dyn std::error::Error + Send + Sync>
                })?;
            inner_guard.audio_task = Some(audio_task);
        }
        Ok(())
    }

    /// Main video processing task handler
    ///
    /// This handler implements two modes matching the Python implementation:
    /// 1. Live mode (video_out_is_live=true): Reads frames from video_queue_rx using video_is_live_handler
    /// 2. Non-live mode (video_out_is_live=false): Cycles through pre-set images using video_images iterator
    async fn video_task_handler(
        inner: Arc<Mutex<MediaSenderInner>>,
        mut video_queue_rx: mpsc::UnboundedReceiver<OutputImageRawFrame>,
    ) {
        // Initialize video frame timing following Python implementation
        {
            let mut inner_guard = inner.lock().await;
            inner_guard.video_start_time = None;
            inner_guard.video_frame_index = 0;
            inner_guard.video_frame_duration = 1.0 / inner_guard.params.video_out_framerate as f64;
            inner_guard.video_frame_reset = inner_guard.video_frame_duration * 5.0;
        }

        loop {
            let (should_break, is_live) = {
                let inner_guard = inner.lock().await;
                if !inner_guard.params.video_out_enabled {
                    (true, false)
                } else {
                    (false, inner_guard.params.video_out_is_live)
                }
            };

            if should_break {
                break;
            }

            if is_live {
                // Live mode: Get frames from video queue (Python: await self._video_queue.get())
                Self::video_is_live_handler(&inner, &mut video_queue_rx).await;
            } else {
                // Non-live mode: Cycle through pre-set images (Python: elif self._video_images)
                let has_images = {
                    let inner_guard = inner.lock().await;
                    inner_guard.video_images.is_some()
                };

                if has_images {
                    // Get next image from cycling iterator (Python: image = next(self._video_images))
                    let image = {
                        let mut inner_guard = inner.lock().await;
                        if let Some(ref mut video_images) = inner_guard.video_images {
                            video_images.next()
                        } else {
                            None
                        }
                    };

                    if let Some(image) = image {
                        // Draw the image (Python: await self._draw_image(image))
                        Self::draw_image(&inner, image).await;
                    }
                }

                // Always sleep for frame duration (Python: await asyncio.sleep(self._video_frame_duration))
                let frame_duration = {
                    let inner_guard = inner.lock().await;
                    inner_guard.video_frame_duration
                };
                tokio::time::sleep(std::time::Duration::from_secs_f64(frame_duration)).await;
            }
        }
    }

    /// Handle video output for live streaming
    async fn video_is_live_handler(
        inner: &Arc<Mutex<MediaSenderInner>>,
        video_queue_rx: &mut mpsc::UnboundedReceiver<OutputImageRawFrame>,
    ) {
        // Get image from video queue (equivalent to await self._video_queue.get())
        let image = match video_queue_rx.recv().await {
            Some(img) => img,
            None => {
                // No frame available, just wait for frame duration and return
                let frame_duration = {
                    let inner_guard = inner.lock().await;
                    inner_guard.video_frame_duration
                };
                tokio::time::sleep(std::time::Duration::from_secs_f64(frame_duration)).await;
                return;
            }
        };

        let (frame_duration, frame_reset) = {
            let inner_guard = inner.lock().await;
            (
                inner_guard.video_frame_duration,
                inner_guard.video_frame_reset,
            )
        };

        let now = std::time::Instant::now();

        // We get the start time as soon as we get the first image
        let (should_reset, delay_time) = {
            let mut inner_guard = inner.lock().await;

            if inner_guard.video_start_time.is_none() {
                inner_guard.video_start_time = Some(now);
                inner_guard.video_frame_index = 0;
                (false, 0.0)
            } else {
                let start_time = inner_guard.video_start_time.unwrap();

                // Calculate how much time we need to wait before rendering next image
                let real_elapsed_time = now.duration_since(start_time).as_secs_f64();
                let real_render_time = inner_guard.video_frame_index as f64 * frame_duration;
                let delay_time = frame_duration + real_render_time - real_elapsed_time;

                // Check if we need to reset timing
                if delay_time.abs() > frame_reset {
                    (true, 0.0)
                } else {
                    (false, delay_time)
                }
            }
        };

        // Reset timing if needed
        if should_reset {
            let mut inner_guard = inner.lock().await;
            inner_guard.video_start_time = Some(now);
            inner_guard.video_frame_index = 0;
        } else if delay_time > 0.0 {
            // Wait for the calculated delay time
            tokio::time::sleep(std::time::Duration::from_secs_f64(delay_time)).await;

            // Increment frame index (only when delay_time > 0, following Python)
            let mut inner_guard = inner.lock().await;
            inner_guard.video_frame_index += 1;
        }

        // Render image
        Self::draw_image(inner, image).await;

        log::debug!("Live video frame processing completed");
    }

    /// Draw/render an image frame with resizing if needed
    async fn draw_image(inner: &Arc<Mutex<MediaSenderInner>>, frame: OutputImageRawFrame) {
        // Get the desired output size and transport reference
        let (desired_width, desired_height, transport_weak) = {
            let inner_guard = inner.lock().await;
            (
                inner_guard.params.video_out_width,
                inner_guard.params.video_out_height,
                inner_guard.transport.clone(),
            )
        };

        let resized_frame = tokio::task::spawn_blocking(move || {
            Self::resize_frame(frame, (desired_width, desired_height))
        })
        .await
        .unwrap_or_else(|e| {
            log::error!("Failed to execute blocking resize task: {}", e);
            // Fall back to original frame
            OutputImageRawFrame::new(Vec::new(), (desired_width, desired_height), None)
        });

        // Write the frame to the transport
        if let Some(transport_arc) = transport_weak.upgrade() {
            match transport_arc.write_video_frame(&resized_frame).await {
                Ok(_) => {
                    log::debug!("Successfully wrote video frame to transport");
                }
                Err(e) => {
                    log::error!("Failed to write video frame to transport: {}", e);
                }
            }
        } else {
            log::warn!("Transport reference is no longer valid");
        }
    }

    /// Resize image frame implementation (blocking operation)
    fn resize_frame(frame: OutputImageRawFrame, desired_size: (u32, u32)) -> OutputImageRawFrame {
        // TODO: we should refactor in the future to support dynamic resolutions
        // which is kind of what happens in P2P connections.
        // We need to add support for that inside the DailyTransport
        if frame.image_frame.size != desired_size {
            log::warn!(
                "Frame size {:?} does not match expected size {:?}, resizing",
                frame.image_frame.size,
                desired_size
            );

            match Self::resize_frame_impl(&frame, desired_size) {
                Ok(resized_frame) => resized_frame,
                Err(e) => {
                    log::error!("Failed to resize image: {}, using original frame", e);
                    // Fall back to creating a new frame with desired size but original data
                    OutputImageRawFrame::new(
                        frame.image_frame.image,
                        desired_size,
                        frame.image_frame.format,
                    )
                }
            }
        } else {
            frame
        }
    }

    /// Internal resize implementation using the image crate
    fn resize_frame_impl(
        frame: &OutputImageRawFrame,
        desired_size: (u32, u32),
    ) -> Result<OutputImageRawFrame, Box<dyn std::error::Error + Send + Sync>> {
        let (original_width, original_height) = frame.image_frame.size;
        let (desired_width, desired_height) = desired_size;

        // Handle different image formats and color spaces
        let img = match frame.image_frame.format.as_deref() {
            Some("RGB") => {
                // Assume 3 bytes per pixel for RGB
                if frame.image_frame.image.len() != (original_width * original_height * 3) as usize
                {
                    return Err("RGB image data length doesn't match expected size".into());
                }
                let rgb_buffer: image::ImageBuffer<image::Rgb<u8>, Vec<u8>> =
                    image::ImageBuffer::from_raw(
                        original_width,
                        original_height,
                        frame.image_frame.image.clone(),
                    )
                    .ok_or("Failed to create RGB image buffer")?;
                image::DynamicImage::ImageRgb8(rgb_buffer)
            }
            Some("RGBA") => {
                // Assume 4 bytes per pixel for RGBA
                if frame.image_frame.image.len() != (original_width * original_height * 4) as usize
                {
                    return Err("RGBA image data length doesn't match expected size".into());
                }
                let rgba_buffer: image::ImageBuffer<image::Rgba<u8>, Vec<u8>> =
                    image::ImageBuffer::from_raw(
                        original_width,
                        original_height,
                        frame.image_frame.image.clone(),
                    )
                    .ok_or("Failed to create RGBA image buffer")?;
                image::DynamicImage::ImageRgba8(rgba_buffer)
            }
            _ => {
                // Try to load as generic image data - determine format based on magic bytes
                let image_format = match frame.image_frame.format.as_deref() {
                    Some("JPEG") | Some("JPG") => image::ImageFormat::Jpeg,
                    Some("PNG") => image::ImageFormat::Png,
                    _ => {
                        // Try to guess the format or default to PNG
                        image::guess_format(&frame.image_frame.image)
                            .unwrap_or(image::ImageFormat::Png)
                    }
                };
                image::load_from_memory_with_format(&frame.image_frame.image, image_format)?
            }
        };

        // Resize the image using Lanczos3 filter for good quality
        let resized_img = img.resize(
            desired_width,
            desired_height,
            image::imageops::FilterType::Lanczos3,
        );

        // Convert back to raw bytes
        let resized_bytes = match frame.image_frame.format.as_deref() {
            Some("RGB") => resized_img.to_rgb8().into_raw(),
            Some("RGBA") => resized_img.to_rgba8().into_raw(),
            _ => {
                // For other formats, convert to RGB
                resized_img.to_rgb8().into_raw()
            }
        };

        // Create new frame with resized data
        Ok(OutputImageRawFrame::new(
            resized_bytes,
            desired_size,
            frame.image_frame.format.clone(),
        ))
    }

    /// Main clock/timing task handler for timed frame delivery (equivalent to Python _clock_task_handler)
    async fn clock_task_handler(
        _inner: Arc<Mutex<MediaSenderInner>>,
        mut clock_queue_rx: mpsc::UnboundedReceiver<(u64, u64, FrameType)>,
        transport_weak: std::sync::Weak<BaseOutputTransport>,
    ) {
        let mut running = true;

        while running {
            // Get timed frame from clock queue (Python: timestamp, _, frame = await self._clock_queue.get())
            let (timestamp, _priority, frame) = match clock_queue_rx.recv().await {
                Some(timed_frame) => timed_frame,
                None => {
                    // Channel closed, exit
                    break;
                }
            };

            // Check if we hit an EndFrame (Python: running = not isinstance(frame, EndFrame))
            running = !matches!(frame, FrameType::End(_));

            if running {
                // Get current time from transport clock
                if let Some(transport) = transport_weak.upgrade() {
                    // Get the current time from transport clock (Python: self._transport.get_clock().get_time())
                    let current_time = match transport.get_clock().await.get_time() {
                        Ok(time) => time,
                        Err(e) => {
                            log::error!("Failed to get clock time: {:?}", e);
                            // Fall back to system time
                            std::time::SystemTime::now()
                                .duration_since(std::time::UNIX_EPOCH)
                                .unwrap_or_default()
                                .as_nanos() as u64
                        }
                    };

                    // If timestamp is in the future, wait until it's time to process
                    if timestamp > current_time {
                        let wait_time_nanos = timestamp - current_time;
                        let wait_time_secs = wait_time_nanos as f64 / 1_000_000_000.0;
                        tokio::time::sleep(std::time::Duration::from_secs_f64(wait_time_secs))
                            .await;
                    }

                    // Push frame downstream (Python: await self._transport.push_frame(frame))
                    match transport
                        .push_frame(frame, FrameDirection::Downstream)
                        .await
                    {
                        Ok(_) => {
                            log::debug!("Successfully pushed frame downstream");
                        }
                        Err(e) => {
                            log::error!("Failed to push frame downstream: {}", e);
                        }
                    }
                } else {
                    log::warn!("Transport reference is no longer valid in clock task");
                    break;
                }
            }

            // Mark task as done (Python: self._clock_queue.task_done())
            // In our mpsc implementation, this is handled automatically when the loop continues
        }

        log::debug!("Clock task handler finished");
    }

    /// Main audio processing task handler (equivalent to Python _audio_task_handler)
    async fn audio_task_handler(
        inner: Arc<Mutex<MediaSenderInner>>,
        audio_queue_rx: mpsc::UnboundedReceiver<FrameType>,
        transport_weak: std::sync::Weak<BaseOutputTransport>,
    ) {
        // Push a BotSpeakingFrame every 200ms, we don't really need to push it
        // at every audio chunk. If the audio chunk is bigger than 200ms, push at
        // every audio chunk.
        let (_total_chunk_ms, bot_speaking_chunk_period) = {
            let inner_guard = inner.lock().await;
            let total_chunk_ms = inner_guard.params.audio_out_10ms_chunks * 10;
            let bot_speaking_chunk_period = std::cmp::max(200 / total_chunk_ms, 1);
            (total_chunk_ms, bot_speaking_chunk_period)
        };

        let mut bot_speaking_counter = 0;
        let mut speech_last_speaking_time = 0.0;
        const BOT_VAD_STOP_SECS: f64 = 0.35; // TODO: This should be configurable

        // Create frame generator using _next_frame equivalent
        let mut frame_stream =
            Self::next_frame(inner.clone(), audio_queue_rx, transport_weak.clone()).await;

        // Main audio processing loop (Python: async for frame in self._next_frame())
        while let Some(frame) = frame_stream.recv().await {
            // Check if we should exit on EndFrame
            if matches!(frame, FrameType::End(_)) {
                break;
            }

            // Notify the bot started speaking upstream if necessary and that it's actually speaking
            let mut is_speaking = false;

            match &frame {
                FrameType::TTSAudioRaw(_) => {
                    is_speaking = true;
                }
                FrameType::SpeechOutputAudioRaw(speech_frame) => {
                    if !is_silence(&speech_frame.output_audio_frame.audio_frame.audio) {
                        is_speaking = true;
                        speech_last_speaking_time = std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .unwrap_or_default()
                            .as_secs_f64();
                    } else {
                        let current_time = std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .unwrap_or_default()
                            .as_secs_f64();
                        let silence_duration = current_time - speech_last_speaking_time;
                        if silence_duration > BOT_VAD_STOP_SECS {
                            Self::bot_stopped_speaking(&inner, &transport_weak).await;
                        }
                    }
                }
                _ => {}
            }

            if is_speaking {
                Self::bot_started_speaking(&inner, &transport_weak).await;
                if bot_speaking_counter % bot_speaking_chunk_period == 0 {
                    // Push BotSpeakingFrame downstream and upstream
                    if let Some(transport) = transport_weak.upgrade() {
                        let destination = {
                            let inner_guard = inner.lock().await;
                            inner_guard.destination.clone()
                        };

                        // Create BotSpeakingFrame instances
                        let mut downstream_frame = BotSpeakingFrame::new();
                        downstream_frame.set_transport_destination(destination.clone());
                        let mut upstream_frame = BotSpeakingFrame::new();
                        upstream_frame.set_transport_destination(destination.clone());

                        // Push frames downstream and upstream
                        if let Err(e) = transport
                            .push_frame(
                                FrameType::BotSpeaking(downstream_frame),
                                FrameDirection::Downstream,
                            )
                            .await
                        {
                            log::error!("Failed to push BotSpeakingFrame downstream: {}", e);
                        }
                        if let Err(e) = transport
                            .push_frame(
                                FrameType::BotSpeaking(upstream_frame),
                                FrameDirection::Upstream,
                            )
                            .await
                        {
                            log::error!("Failed to push BotSpeakingFrame upstream: {}", e);
                        }
                    }
                    bot_speaking_counter = 0;
                }
                bot_speaking_counter += 1;
            }

            // Handle frame processing
            Self::handle_frame(&inner, &transport_weak, &frame).await;

            // If we are not able to write to the transport we shouldn't push downstream
            let push_downstream;

            // Try to send audio to the transport
            if let Some(transport) = transport_weak.upgrade() {
                match &frame {
                    FrameType::OutputAudioRaw(audio_frame) => {
                        match transport.write_audio_frame(audio_frame).await {
                            Ok(success) => {
                                push_downstream = success;
                            }
                            Err(e) => {
                                log::error!("Error writing audio frame to transport: {}", e);
                                push_downstream = false;
                            }
                        }
                    }
                    _ => {
                        // For non-audio frames, we can still push them downstream
                        push_downstream = true;
                    }
                }

                // If we were able to send to the transport, push the frame downstream
                // in case anyone else needs it
                if push_downstream {
                    if let Err(e) = transport
                        .push_frame(frame, FrameDirection::Downstream)
                        .await
                    {
                        log::error!("Failed to push frame downstream: {}", e);
                    }
                }
            } else {
                log::warn!("Transport reference is no longer valid in audio task");
                break;
            }
        }

        log::debug!("Audio task handler finished");
    }

    /// Generate the next frame for audio processing (equivalent to Python _next_frame)
    ///
    /// Returns a receiver that yields frames, handling timeouts and mixer logic
    async fn next_frame(
        inner: Arc<Mutex<MediaSenderInner>>,
        audio_queue_rx: mpsc::UnboundedReceiver<FrameType>,
        transport_weak: std::sync::Weak<BaseOutputTransport>,
    ) -> mpsc::UnboundedReceiver<FrameType> {
        let (frame_tx, frame_rx) = mpsc::unbounded_channel();

        // Spawn the frame generation task
        tokio::spawn(async move {
            const BOT_VAD_STOP_SECS: f64 = 0.35; // TODO: This should be configurable

            // Check if we have a mixer
            let has_mixer = {
                let inner_guard = inner.lock().await;
                inner_guard.audio_mixer.is_some()
            };

            if has_mixer {
                // with_mixer implementation
                Self::with_mixer_frame_generator(
                    inner,
                    audio_queue_rx,
                    transport_weak,
                    frame_tx,
                    BOT_VAD_STOP_SECS,
                )
                .await;
            } else {
                // without_mixer implementation
                Self::without_mixer_frame_generator(
                    inner,
                    audio_queue_rx,
                    transport_weak,
                    frame_tx,
                    BOT_VAD_STOP_SECS,
                )
                .await;
            }
        });

        frame_rx
    }

    /// Frame generator without mixer (equivalent to Python without_mixer)
    async fn without_mixer_frame_generator(
        inner: Arc<Mutex<MediaSenderInner>>,
        mut audio_queue_rx: mpsc::UnboundedReceiver<FrameType>,
        transport_weak: std::sync::Weak<BaseOutputTransport>,
        frame_tx: mpsc::UnboundedSender<FrameType>,
        vad_stop_secs: f64,
    ) {
        loop {
            // Try to get frame with timeout (Python: await asyncio.wait_for(self._audio_queue.get(), timeout=vad_stop_secs))
            let timeout_duration = std::time::Duration::from_secs_f64(vad_stop_secs);

            match tokio::time::timeout(timeout_duration, audio_queue_rx.recv()).await {
                Ok(Some(frame)) => {
                    // Got a frame, yield it
                    if frame_tx.send(frame).is_err() {
                        break; // Channel closed
                    }
                    // Python: self._audio_queue.task_done() - handled automatically by mpsc
                }
                Ok(None) => {
                    // Channel closed
                    break;
                }
                Err(_) => {
                    // Timeout occurred - notify bot stopped speaking
                    Self::bot_stopped_speaking(&inner, &transport_weak).await;
                }
            }
        }
    }

    /// Frame generator with mixer (equivalent to Python with_mixer)
    async fn with_mixer_frame_generator(
        inner: Arc<Mutex<MediaSenderInner>>,
        mut audio_queue_rx: mpsc::UnboundedReceiver<FrameType>,
        transport_weak: std::sync::Weak<BaseOutputTransport>,
        frame_tx: mpsc::UnboundedSender<FrameType>,
        vad_stop_secs: f64,
    ) {
        let mut last_frame_time = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs_f64();

        loop {
            // Try to get frame immediately (Python: self._audio_queue.get_nowait())
            match audio_queue_rx.try_recv() {
                Ok(frame) => {
                    // Process frame with mixer if it's an OutputAudioRawFrame
                    let processed_frame = match &frame {
                        FrameType::OutputAudioRaw(audio_frame) => {
                            let mut inner_guard = inner.lock().await;
                            if let Some(ref mut mixer) = inner_guard.audio_mixer {
                                // Python: frame.audio = await self._mixer.mix(frame.audio)
                                match mixer.mix(&audio_frame.audio_frame.audio).await {
                                    Ok(mixed_audio) => {
                                        // Create new frame with mixed audio
                                        let mut new_audio_frame = audio_frame.clone();
                                        new_audio_frame.audio_frame.audio = mixed_audio;
                                        FrameType::OutputAudioRaw(new_audio_frame)
                                    }
                                    Err(e) => {
                                        log::error!("Failed to mix audio: {}", e);
                                        frame // Use original frame on error
                                    }
                                }
                            } else {
                                frame
                            }
                        }
                        _ => frame,
                    };

                    last_frame_time = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_secs_f64();

                    if frame_tx.send(processed_frame).is_err() {
                        break; // Channel closed
                    }
                    // Python: self._audio_queue.task_done() - handled automatically by mpsc
                }
                Err(mpsc::error::TryRecvError::Empty) => {
                    // No frame available - check for VAD timeout
                    let current_time = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_secs_f64();

                    let diff_time = current_time - last_frame_time;
                    if diff_time > vad_stop_secs {
                        Self::bot_stopped_speaking(&inner, &transport_weak).await;
                    }

                    // Generate an audio frame with only the mixer's part
                    let (audio_chunk_size, sample_rate, audio_out_channels) = {
                        let inner_guard = inner.lock().await;
                        (
                            inner_guard.audio_chunk_size as usize,
                            inner_guard.sample_rate,
                            inner_guard.params.audio_out_channels,
                        )
                    };

                    let silence = vec![0u8; audio_chunk_size];
                    let mut inner_guard = inner.lock().await;
                    if let Some(ref mut mixer) = inner_guard.audio_mixer {
                        match mixer.mix(&silence).await {
                            Ok(mixed_silence) => {
                                let frame = OutputAudioRawFrame::new(
                                    mixed_silence,
                                    sample_rate,
                                    audio_out_channels as u16,
                                );
                                if frame_tx.send(FrameType::OutputAudioRaw(frame)).is_err() {
                                    break; // Channel closed
                                }
                            }
                            Err(e) => {
                                log::error!("Failed to mix silence: {}", e);
                            }
                        }
                    }

                    // Allow other asyncio tasks to execute by adding a small sleep
                    tokio::time::sleep(std::time::Duration::from_millis(1)).await;
                }
                Err(mpsc::error::TryRecvError::Disconnected) => {
                    // Channel closed
                    break;
                }
            }
        }
    }

    /// Handle bot started speaking event
    async fn bot_started_speaking(
        inner: &Arc<Mutex<MediaSenderInner>>,
        transport_weak: &std::sync::Weak<BaseOutputTransport>,
    ) {
        let inner_guard = inner.lock().await;
        if !inner_guard.bot_speaking {
            let destination = inner_guard.destination.clone();

            log::debug!(
                "Bot{} started speaking",
                if let Some(ref dest) = destination {
                    format!(" [{}]", dest)
                } else {
                    String::new()
                }
            );

            drop(inner_guard); // Release lock before async operations

            if let Some(transport) = transport_weak.upgrade() {
                // Create BotStartedSpeakingFrame instances
                let mut downstream_frame = BotStartedSpeakingFrame::new();
                downstream_frame.set_transport_destination(destination.clone());
                let mut upstream_frame = BotStartedSpeakingFrame::new();
                upstream_frame.set_transport_destination(destination.clone());

                // Push frames downstream and upstream
                if let Err(e) = transport
                    .push_frame(
                        FrameType::BotStartedSpeaking(downstream_frame),
                        FrameDirection::Downstream,
                    )
                    .await
                {
                    log::error!("Failed to push BotStartedSpeakingFrame downstream: {}", e);
                }
                if let Err(e) = transport
                    .push_frame(
                        FrameType::BotStartedSpeaking(upstream_frame),
                        FrameDirection::Upstream,
                    )
                    .await
                {
                    log::error!("Failed to push BotStartedSpeakingFrame upstream: {}", e);
                }
            }

            // Set bot_speaking to true at the end, matching Python implementation
            let mut inner_guard = inner.lock().await;
            inner_guard.bot_speaking = true;
        }
    }

    /// Handle bot stopped speaking event
    async fn bot_stopped_speaking(
        inner: &Arc<Mutex<MediaSenderInner>>,
        transport_weak: &std::sync::Weak<BaseOutputTransport>,
    ) {
        let mut inner_guard = inner.lock().await;
        if inner_guard.bot_speaking {
            let destination = inner_guard.destination.clone();

            log::debug!(
                "Bot{} stopped speaking",
                if let Some(ref dest) = destination {
                    format!(" [{}]", dest)
                } else {
                    String::new()
                }
            );

            // Create BotStoppedSpeakingFrame instances

            inner_guard.bot_speaking = false;

            // Clean audio buffer (there could be tiny left overs if not multiple
            // to our output chunk size).
            inner_guard.audio_buffer.clear();

            drop(inner_guard); // Release lock before async operations

            if let Some(transport) = transport_weak.upgrade() {
                // Create BotStoppedSpeakingFrame instances
                let mut downstream_frame = BotStoppedSpeakingFrame::new();
                downstream_frame.set_transport_destination(destination.clone());
                let mut upstream_frame = BotStoppedSpeakingFrame::new();
                upstream_frame.set_transport_destination(destination.clone());

                // Push frames downstream and upstream
                if let Err(e) = transport
                    .push_frame(
                        FrameType::BotStoppedSpeaking(downstream_frame),
                        FrameDirection::Downstream,
                    )
                    .await
                {
                    log::error!("Failed to push BotStoppedSpeakingFrame downstream: {}", e);
                }
                if let Err(e) = transport
                    .push_frame(
                        FrameType::BotStoppedSpeaking(upstream_frame),
                        FrameDirection::Upstream,
                    )
                    .await
                {
                    log::error!("Failed to push BotStoppedSpeakingFrame upstream: {}", e);
                }
            }
        }
    }

    /// Handle frame processing (equivalent to Python _handle_frame)
    async fn handle_frame(
        inner: &Arc<Mutex<MediaSenderInner>>,
        transport_weak: &std::sync::Weak<BaseOutputTransport>,
        frame: &FrameType,
    ) {
        // Handle various frame types with appropriate processing (only specific types from Python)
        match frame {
            FrameType::OutputImageRaw(image_frame) => {
                // Set single video image (Python: await self._set_video_image(frame))
                let mut inner_guard = inner.lock().await;
                inner_guard.video_images = Some(vec![image_frame.clone()].into_iter().cycle());
                log::debug!("Set video image from OutputImageRaw frame");
            }
            FrameType::Sprite(sprite_frame) => {
                // Set multiple video images (Python: await self._set_video_images(frame.images))
                let mut inner_guard = inner.lock().await;
                inner_guard.video_images = Some(sprite_frame.images.clone().into_iter().cycle());
                log::debug!(
                    "Set video images from Sprite frame with {} images",
                    sprite_frame.images.len()
                );
            }
            FrameType::TransportMessage(message_frame) => {
                // Send transport message (Python: await self._transport.send_message(frame))
                if let Some(transport) = transport_weak.upgrade() {
                    let frame_type = TransportMessageFrameType::Message(message_frame.clone());
                    let _ = transport.send_message(frame_type).await;
                    log::debug!("Sent TransportMessage frame");
                } else {
                    log::warn!("Transport not available for TransportMessage");
                }
            }
            FrameType::TransportMessageUrgent(urgent_frame) => {
                // Send urgent transport message (Python: await self._transport.send_message(frame))
                if let Some(transport) = transport_weak.upgrade() {
                    let frame_type = TransportMessageFrameType::Urgent(urgent_frame.clone());
                    let _ = transport.send_message(frame_type).await;
                    log::debug!("Sent TransportMessageUrgent frame");
                } else {
                    log::warn!("Transport not available for TransportMessageUrgent");
                }
            }
            _ => {
                // Do nothing for other frame types - only handle the specific types from Python
            }
        }
    }

    /// Handle TTS audio frames by sending to audio queue
    pub async fn handle_tts_audio_frame(
        &self,
        frame: &crate::frames::TTSAudioRawFrame,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let inner_guard = self.inner.lock().await;

        if inner_guard.params.audio_out_enabled {
            // Send frame to audio queue if available
            if let Some(ref audio_sender) = inner_guard.audio_queue {
                let _ = audio_sender.send(FrameType::TTSAudioRaw(frame.clone()));
            }
        }

        Ok(())
    }

    /// Handle speech output audio frames by sending to audio queue
    pub async fn handle_speech_audio_frame(
        &self,
        frame: &crate::frames::SpeechOutputAudioRawFrame,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let inner_guard = self.inner.lock().await;

        if inner_guard.params.audio_out_enabled {
            // Send frame to audio queue if available
            if let Some(ref audio_sender) = inner_guard.audio_queue {
                let _ = audio_sender.send(FrameType::SpeechOutputAudioRaw(frame.clone()));
            }
        }

        Ok(())
    }
}
