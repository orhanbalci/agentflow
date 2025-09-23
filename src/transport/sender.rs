//! Output transport implementation for media streaming.
//!
//! This module provides the MediaSender implementation that handles media streaming
//! for specific destinations, including audio and video output processing with
//! buffering, timing, mixing, and frame delivery.

use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tokio::time::sleep;

use crate::audio::mixer::BaseAudioMixer;
use crate::transport::{params::TransportParams, BaseOutputTransport};
use crate::{
    BotSpeakingFrame, BotStartedSpeakingFrame, BotStoppedSpeakingFrame, CancelFrame, EndFrame,
    Frame, FrameDirection, MixerControlFrame, OutputAudioRawFrame, OutputImageRawFrame,
    SpriteFrame, StartFrame, StartInterruptionFrame,
};

/// Convert nanoseconds to seconds
pub fn nanoseconds_to_seconds(nanos: u64) -> f64 {
    nanos as f64 / 1_000_000_000.0
}

/// Check if audio data contains only silence
pub fn is_silence(audio_data: &[u8]) -> bool {
    // Simple silence detection - check if all bytes are near zero
    audio_data.iter().all(|&b| b.abs_diff(128) < 10) // Assuming 8-bit audio centered at 128
}

/// Bot VAD stop timeout in seconds
const BOT_VAD_STOP_SECS: f64 = 0.5;

/// Handles media streaming for a specific destination.
///
/// Manages audio and video output processing including buffering, timing,
/// mixing, and frame delivery for a single output destination.
pub struct MediaSender {
    /// Reference to the parent transport
    transport: Arc<BaseOutputTransport>,

    /// Destination identifier for this sender
    destination: Option<String>,

    /// Audio sample rate in Hz
    sample_rate: u32,

    /// Size of audio chunks in bytes
    audio_chunk_size: usize,

    /// Transport configuration parameters
    params: TransportParams,

    /// Buffer for incoming audio data
    audio_buffer: Vec<u8>,

    /// Audio resampler for sample rate conversion
    resampler: Box<dyn AudioResampler>,

    /// Audio mixer for combining audio streams
    mixer: Option<Box<dyn BaseAudioMixer>>,

    /// Video images for cycling output
    video_images: Option<VideoImageCycle>,

    /// Indicates if the bot is currently speaking
    bot_speaking: bool,

    /// Audio processing task handle
    audio_task: Option<JoinHandle<()>>,

    /// Video processing task handle
    video_task: Option<JoinHandle<()>>,

    /// Clock processing task handle
    clock_task: Option<JoinHandle<()>>,

    /// Audio frame queue
    audio_queue: Option<mpsc::UnboundedSender<Box<dyn Frame>>>,

    /// Video frame queue
    video_queue: Option<mpsc::UnboundedSender<OutputImageRawFrame>>,

    /// Clock frame queue
    clock_queue: Option<mpsc::UnboundedSender<(u64, u64, Box<dyn Frame>)>>,
}

/// Placeholder for audio resampler trait
#[async_trait::async_trait]
pub trait AudioResampler: Send + Sync {
    async fn resample(
        &mut self,
        audio_data: &[u8],
        input_rate: u32,
        output_rate: u32,
    ) -> Result<Vec<u8>, Box<dyn std::error::Error + Send + Sync>>;
}

/// Placeholder implementation for audio resampler
pub struct DefaultAudioResampler;

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

/// Create a default stream resampler
pub fn create_stream_resampler() -> Box<dyn AudioResampler> {
    Box::new(DefaultAudioResampler)
}

/// Video image cycling for output
pub enum VideoImageCycle {
    Single(OutputImageRawFrame),
    Multiple(Vec<OutputImageRawFrame>, usize), // images and current index
}

impl VideoImageCycle {
    pub fn next(&mut self) -> &OutputImageRawFrame {
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

impl MediaSender {
    /// Initialize the media sender.
    ///
    /// # Arguments
    /// * `transport` - The parent transport instance
    /// * `destination` - The destination identifier for this sender
    /// * `sample_rate` - The audio sample rate in Hz
    /// * `audio_chunk_size` - The size of audio chunks in bytes
    /// * `params` - Transport configuration parameters
    pub fn new(
        transport: Arc<BaseOutputTransport>,
        destination: Option<String>,
        sample_rate: u32,
        audio_chunk_size: usize,
        params: TransportParams,
    ) -> Self {
        Self {
            transport,
            destination,
            sample_rate,
            audio_chunk_size,
            params,
            audio_buffer: Vec::new(),
            resampler: create_stream_resampler(),
            mixer: None,
            video_images: None,
            bot_speaking: false,
            audio_task: None,
            video_task: None,
            clock_task: None,
            audio_queue: None,
            video_queue: None,
            clock_queue: None,
        }
    }

    /// Get the audio sample rate.
    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    /// Get the audio chunk size.
    pub fn audio_chunk_size(&self) -> usize {
        self.audio_chunk_size
    }

    /// Start the media sender and initialize components.
    pub async fn start(
        &mut self,
        _frame: &StartFrame,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        self.audio_buffer.clear();

        // Create all tasks
        self.create_video_task().await;
        self.create_clock_task().await;
        self.create_audio_task().await;

        // Check if we have an audio mixer for our destination
        if self.params.audio_out_mixer.is_some() {
            // Placeholder for mixer initialization logic
            // This would need to be implemented based on the mixer configuration type
        }

        // Start audio mixer
        if let Some(mixer) = &mut self.mixer {
            mixer.start(self.sample_rate).await?;
        }

        Ok(())
    }

    /// Stop the media sender and cleanup resources.
    pub async fn stop(
        &mut self,
        frame: &EndFrame,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        // Let the sink tasks process the queue until they reach this EndFrame
        if let Some(clock_sender) = &self.clock_queue {
            let _ = clock_sender.send((
                u64::MAX,
                crate::frames::Frame::id(frame),
                Box::new(frame.clone()),
            ));
        }

        if let Some(audio_sender) = &self.audio_queue {
            let _ = audio_sender.send(Box::new(frame.clone()));
        }

        // Wait for audio and clock tasks to complete
        if let Some(audio_task) = self.audio_task.take() {
            let _ = audio_task.await;
        }

        if let Some(clock_task) = self.clock_task.take() {
            let _ = clock_task.await;
        }

        // Stop audio mixer
        if let Some(mixer) = &mut self.mixer {
            mixer.stop().await?;
        }

        // Cancel the video task
        self.cancel_video_task().await;

        Ok(())
    }

    /// Cancel the media sender and stop all processing.
    pub async fn cancel(
        &mut self,
        _frame: &CancelFrame,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        self.cancel_audio_task().await;
        self.cancel_clock_task().await;
        self.cancel_video_task().await;
        Ok(())
    }

    /// Handle interruption events by restarting tasks and clearing buffers.
    pub async fn handle_interruptions(
        &mut self,
        _frame: &StartInterruptionFrame,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        if !self.transport.interruptions_allowed() {
            return Ok(());
        }

        // Cancel tasks
        self.cancel_audio_task().await;
        self.cancel_clock_task().await;
        self.cancel_video_task().await;

        // Create tasks
        self.create_video_task().await;
        self.create_clock_task().await;
        self.create_audio_task().await;

        // Send bot stopped speaking if necessary
        self.bot_stopped_speaking().await?;

        Ok(())
    }

    /// Handle incoming audio frames by buffering and chunking.
    pub async fn handle_audio_frame(
        &mut self,
        frame: &OutputAudioRawFrame,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        if !self.params.audio_out_enabled {
            return Ok(());
        }

        // Resample if incoming audio doesn't match the transport sample rate
        let resampled = self
            .resampler
            .resample(
                &frame.audio_frame.audio,
                frame.audio_frame.sample_rate,
                self.sample_rate,
            )
            .await?;

        self.audio_buffer.extend_from_slice(&resampled);

        while self.audio_buffer.len() >= self.audio_chunk_size {
            let chunk_data = self
                .audio_buffer
                .drain(..self.audio_chunk_size)
                .collect::<Vec<_>>();

            let mut chunk = OutputAudioRawFrame::new(
                chunk_data,
                self.sample_rate,
                frame.audio_frame.num_channels,
            );
            chunk.set_transport_destination(self.destination.clone());

            if let Some(audio_sender) = &self.audio_queue {
                let _ = audio_sender.send(Box::new(chunk));
            }
        }

        Ok(())
    }

    /// Handle incoming image frames for video output.
    pub async fn handle_image_frame(
        &mut self,
        frame: &OutputImageRawFrame,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        if !self.params.video_out_enabled {
            return Ok(());
        }

        if self.params.video_out_is_live {
            if let Some(video_sender) = &self.video_queue {
                let _ = video_sender.send(frame.clone());
            }
        } else {
            self.set_video_image(frame.clone()).await;
        }

        Ok(())
    }

    /// Handle sprite frames for video output.
    pub async fn handle_sprite_frame(
        &mut self,
        frame: &SpriteFrame,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        if !self.params.video_out_enabled {
            return Ok(());
        }

        self.set_video_images(frame.images.clone()).await;
        Ok(())
    }

    /// Handle frames with presentation timestamps.
    pub async fn handle_timed_frame(
        &mut self,
        frame: Box<dyn Frame>,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        if let Some(clock_sender) = &self.clock_queue {
            let pts = frame.pts().unwrap_or(0);
            let _ = clock_sender.send((pts, frame.id(), frame));
        }
        Ok(())
    }

    /// Handle frames that need synchronized processing.
    pub async fn handle_sync_frame(
        &mut self,
        frame: Box<dyn Frame>,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        if let Some(audio_sender) = &self.audio_queue {
            let _ = audio_sender.send(frame);
        }
        Ok(())
    }

    /// Handle audio mixer control frames.
    pub async fn handle_mixer_control_frame(
        &mut self,
        frame: &MixerControlFrame,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        if let Some(mixer) = &mut self.mixer {
            mixer.process_frame(frame).await?;
        }
        Ok(())
    }

    // Audio handling methods

    /// Create the audio processing task.
    async fn create_audio_task(&mut self) {
        if self.audio_task.is_none() {
            let (sender, receiver) = mpsc::unbounded_channel();
            self.audio_queue = Some(sender);

            let transport = Arc::clone(&self.transport);
            let destination = self.destination.clone();
            let params = self.params.clone();
            let sample_rate = self.sample_rate;
            let audio_chunk_size = self.audio_chunk_size;

            self.audio_task = Some(self.transport.create_task(async move {
                Self::audio_task_handler(
                    transport,
                    destination,
                    params,
                    sample_rate,
                    audio_chunk_size,
                    receiver,
                )
                .await;
            }));
        }
    }

    /// Cancel and cleanup the audio processing task.
    async fn cancel_audio_task(&mut self) {
        if let Some(audio_task) = self.audio_task.take() {
            self.transport.cancel_task(audio_task).await;
        }
        self.audio_queue = None;
    }

    /// Handle bot started speaking event.
    async fn bot_started_speaking(
        &mut self,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        if !self.bot_speaking {
            println!(
                "Bot{} started speaking",
                if let Some(dest) = &self.destination {
                    format!(" [{}]", dest)
                } else {
                    String::new()
                }
            );

            let mut downstream_frame = BotStartedSpeakingFrame::new();
            downstream_frame.set_transport_destination(self.destination.clone());

            let mut upstream_frame = BotStartedSpeakingFrame::new();
            upstream_frame.set_transport_destination(self.destination.clone());

            self.transport
                .push_frame(Box::new(downstream_frame), FrameDirection::Downstream)
                .await?;
            self.transport
                .push_frame(Box::new(upstream_frame), FrameDirection::Upstream)
                .await?;

            self.bot_speaking = true;
        }
        Ok(())
    }

    /// Handle bot stopped speaking event.
    async fn bot_stopped_speaking(
        &mut self,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        if self.bot_speaking {
            println!(
                "Bot{} stopped speaking",
                if let Some(dest) = &self.destination {
                    format!(" [{}]", dest)
                } else {
                    String::new()
                }
            );

            let mut downstream_frame = BotStoppedSpeakingFrame::new();
            downstream_frame.set_transport_destination(self.destination.clone());

            let mut upstream_frame = BotStoppedSpeakingFrame::new();
            upstream_frame.set_transport_destination(self.destination.clone());

            self.transport
                .push_frame(Box::new(downstream_frame), FrameDirection::Downstream)
                .await?;
            self.transport
                .push_frame(Box::new(upstream_frame), FrameDirection::Upstream)
                .await?;

            self.bot_speaking = false;
            self.audio_buffer.clear();
        }
        Ok(())
    }

    /// Main audio processing task handler.
    async fn audio_task_handler(
        transport: Arc<BaseOutputTransport>,
        destination: Option<String>,
        params: TransportParams,
        sample_rate: u32,
        audio_chunk_size: usize,
        mut receiver: mpsc::UnboundedReceiver<Box<dyn Frame>>,
    ) {
        let total_chunk_ms = params.audio_out_10ms_chunks * 10;
        let bot_speaking_chunk_period = std::cmp::max(200 / total_chunk_ms, 1);
        let mut bot_speaking_counter = 0;
        let mut speech_last_speaking_time = Instant::now();

        while let Some(frame) = receiver.recv().await {
            // Check if bot is speaking
            let mut is_speaking = false;

            // Placeholder for frame type checking - would need proper trait implementations
            // if frame.is_tts_audio() {
            //     is_speaking = true;
            // } else if frame.is_speech_output_audio() && !is_silence(frame.audio()) {
            //     is_speaking = true;
            //     speech_last_speaking_time = Instant::now();
            // } else if frame.is_speech_output_audio() {
            //     let silence_duration = speech_last_speaking_time.elapsed();
            //     if silence_duration.as_secs_f64() > BOT_VAD_STOP_SECS {
            //         // Handle bot stopped speaking
            //     }
            // }

            if is_speaking {
                // Handle bot started speaking
                if bot_speaking_counter % bot_speaking_chunk_period == 0 {
                    let speaking_frame = BotSpeakingFrame::new();
                    let _ = transport
                        .push_frame(Box::new(speaking_frame), FrameDirection::Downstream)
                        .await;
                    let speaking_frame_up = BotSpeakingFrame::new();
                    let _ = transport
                        .push_frame(Box::new(speaking_frame_up), FrameDirection::Upstream)
                        .await;
                    bot_speaking_counter = 0;
                }
                bot_speaking_counter += 1;
            }

            // Check for end frame
            if frame.name() == "EndFrame" {
                break;
            }

            // Handle frame and push downstream
            let _ = transport
                .push_frame(frame, FrameDirection::Downstream)
                .await;
        }
    }

    // Video handling methods

    /// Create the video processing task if video output is enabled.
    async fn create_video_task(&mut self) {
        if self.video_task.is_none() && self.params.video_out_enabled {
            let (sender, receiver) = mpsc::unbounded_channel();
            self.video_queue = Some(sender);

            let transport = Arc::clone(&self.transport);
            let params = self.params.clone();

            self.video_task = Some(self.transport.create_task(async move {
                Self::video_task_handler(transport, params, receiver).await;
            }));
        }
    }

    /// Cancel and cleanup the video processing task.
    async fn cancel_video_task(&mut self) {
        if let Some(video_task) = self.video_task.take() {
            self.transport.cancel_task(video_task).await;
        }
        self.video_queue = None;
    }

    /// Set a single video image for cycling output.
    async fn set_video_image(&mut self, image: OutputImageRawFrame) {
        self.video_images = Some(VideoImageCycle::Single(image));
    }

    /// Set multiple video images for cycling output.
    async fn set_video_images(&mut self, images: Vec<OutputImageRawFrame>) {
        if !images.is_empty() {
            self.video_images = Some(VideoImageCycle::Multiple(images, 0));
        }
    }

    /// Main video processing task handler.
    async fn video_task_handler(
        transport: Arc<BaseOutputTransport>,
        params: TransportParams,
        mut receiver: mpsc::UnboundedReceiver<OutputImageRawFrame>,
    ) {
        let frame_duration = Duration::from_secs_f64(1.0 / params.video_out_framerate as f64);
        let mut video_start_time: Option<Instant> = None;
        let mut video_frame_index = 0;
        let frame_reset_duration = frame_duration * 5;

        while let Some(image) = receiver.recv().await {
            if params.video_out_is_live {
                // Get start time on first image
                if video_start_time.is_none() {
                    video_start_time = Some(Instant::now());
                    video_frame_index = 0;
                }

                if let Some(start_time) = video_start_time {
                    // Calculate timing
                    let real_elapsed_time = start_time.elapsed();
                    let real_render_time = frame_duration * video_frame_index;
                    let delay_time = frame_duration + real_render_time - real_elapsed_time;

                    if delay_time > frame_reset_duration
                        || delay_time.checked_sub(frame_reset_duration).is_none()
                    {
                        video_start_time = Some(Instant::now());
                        video_frame_index = 0;
                    } else if !delay_time.is_zero() && delay_time > Duration::ZERO {
                        sleep(delay_time).await;
                        video_frame_index += 1;
                    }
                }

                // Render image
                let _ = transport.write_video_frame(&image).await;
            } else {
                // Handle non-live video
                let _ = transport.write_video_frame(&image).await;
                sleep(frame_duration).await;
            }
        }
    }

    // Clock handling methods

    /// Create the clock/timing processing task.
    async fn create_clock_task(&mut self) {
        if self.clock_task.is_none() {
            let (sender, receiver) = mpsc::unbounded_channel();
            self.clock_queue = Some(sender);

            let transport = Arc::clone(&self.transport);

            self.clock_task = Some(self.transport.create_task(async move {
                Self::clock_task_handler(transport, receiver).await;
            }));
        }
    }

    /// Cancel and cleanup the clock processing task.
    async fn cancel_clock_task(&mut self) {
        if let Some(clock_task) = self.clock_task.take() {
            self.transport.cancel_task(clock_task).await;
        }
        self.clock_queue = None;
    }

    /// Main clock/timing task handler for timed frame delivery.
    async fn clock_task_handler(
        transport: Arc<BaseOutputTransport>,
        mut receiver: mpsc::UnboundedReceiver<(u64, u64, Box<dyn Frame>)>,
    ) {
        let mut frames = Vec::new();

        while let Some((timestamp, frame_id, frame)) = receiver.recv().await {
            // If we hit an EndFrame, we can finish right away
            if frame.name() == "EndFrame" {
                break;
            }

            // Insert frame in sorted order by timestamp
            let insert_pos = frames
                .binary_search_by_key(&timestamp, |(ts, _, _)| *ts)
                .unwrap_or_else(|pos| pos);
            frames.insert(insert_pos, (timestamp, frame_id, frame));

            // Process frames that are ready
            let clock = transport.get_clock();
            let current_time = clock.get_time().unwrap_or(0);

            while let Some((ts, _, _)) = frames.first() {
                if *ts <= current_time {
                    let (_, _, frame) = frames.remove(0);
                    let _ = transport
                        .push_frame(frame, FrameDirection::Downstream)
                        .await;
                } else {
                    // Wait for the next frame's time
                    let wait_time = nanoseconds_to_seconds(*ts - current_time);
                    sleep(Duration::from_secs_f64(wait_time)).await;
                    break;
                }
            }
        }
    }
}

// Placeholder trait implementations that would need to be added to the Frame trait
trait FrameExt {
    fn is_end_frame(&self) -> bool;
    fn pts(&self) -> u64;
    fn id(&self) -> u64;
}

impl<T: Frame> FrameExt for T {
    fn is_end_frame(&self) -> bool {
        // Placeholder implementation
        false
    }

    fn pts(&self) -> u64 {
        // Placeholder implementation
        0
    }

    fn id(&self) -> u64 {
        // Placeholder implementation
        0
    }
}
