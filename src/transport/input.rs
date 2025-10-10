// Base input transport implementation for AgentFlow
//
// This module provides the BaseInputTransport struct which handles audio and video
// input processing, interruption management, and frame flow control.

use async_trait::async_trait;
use delegate::delegate;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio::sync::Mutex;
use tokio::time::{timeout, Duration};

use crate::processors::frame::{
    BaseInterruptionStrategy, FrameCallback, FrameDirection, FrameProcessor, FrameProcessorMetrics,
    FrameProcessorSetup, FrameProcessorTrait,
};
use crate::task_manager::{TaskHandle, TaskManager};
use crate::transport::params::TransportParams;
use crate::BaseClock;
use crate::{
    FrameType, InputAudioRawFrame, InputImageRawFrame, StartFrame, StartInterruptionFrame,
    StopInterruptionFrame, UserStartedSpeakingFrame, UserStoppedSpeakingFrame,
};

const AUDIO_INPUT_TIMEOUT_SECS: u64 = 1; // Using 1 second instead of 0.5 for better handling

/// Base class for input transport implementations.
///
/// Handles audio and video input processing including frame management,
/// audio filtering, and user interaction management. Supports interruption
/// handling and provides hooks for transport-specific implementations.
pub struct BaseInputTransport {
    /// Transport configuration parameters
    params: TransportParams,

    /// Frame processor implementation
    frame_processor: FrameProcessor,

    /// Input sample rate, initialized on StartFrame
    sample_rate: AtomicU32,

    /// Track bot speaking state for interruption logic
    bot_speaking: AtomicBool,

    /// Track user speaking state for interruption logic
    user_speaking: AtomicBool,

    /// Audio input queue for processing frames
    audio_in_queue: Option<mpsc::UnboundedSender<InputAudioRawFrame>>,

    /// Audio processing task handle
    audio_task: Option<TaskHandle>,

    /// If the transport is stopped with StopFrame, we don't want to push
    /// frames downstream until we get another StartFrame
    paused: AtomicBool,
}

impl BaseInputTransport {
    /// Initialize a new base input transport.
    ///
    /// # Arguments
    ///
    /// * `params` - Transport configuration parameters
    /// * `task_manager` - Task manager for background tasks
    /// * `name` - Optional name for the processor
    pub fn new(
        params: TransportParams,
        task_manager: Arc<TaskManager>,
        name: Option<String>,
    ) -> Self {
        let processor_name = name.unwrap_or_else(|| "BaseInputTransport".to_string());
        let frame_processor = FrameProcessor::new(processor_name, task_manager);

        Self {
            params,
            frame_processor,
            sample_rate: AtomicU32::new(0),
            bot_speaking: AtomicBool::new(false),
            user_speaking: AtomicBool::new(false),
            audio_in_queue: None,
            audio_task: None,
            paused: AtomicBool::new(false),
        }
    }

    /// Enable or disable audio streaming on transport start.
    ///
    /// # Arguments
    ///
    /// * `enabled` - Whether to start audio streaming immediately on transport start
    pub fn enable_audio_in_stream_on_start(&mut self, enabled: bool) {
        log::debug!("Enabling audio on start: {}", enabled);
        self.params.audio_in_stream_on_start = enabled;
    }

    /// Start audio input streaming.
    ///
    /// Override in subclasses to implement transport-specific audio streaming.
    pub async fn start_audio_in_streaming(&self) -> Result<(), String> {
        // Default implementation - override in subclasses
        Ok(())
    }

    /// Get the current audio sample rate.
    pub fn sample_rate(&self) -> u32 {
        self.sample_rate.load(Ordering::Relaxed)
    }

    /// Start the input transport and initialize components.
    ///
    /// # Arguments
    ///
    /// * `frame` - The start frame containing initialization parameters
    pub async fn start(&mut self, frame: &StartFrame) -> Result<(), String> {
        self.paused.store(false, Ordering::Relaxed);
        self.user_speaking.store(false, Ordering::Relaxed);

        let sample_rate = self
            .params
            .audio_in_sample_rate
            .unwrap_or(frame.audio_in_sample_rate);

        self.sample_rate.store(sample_rate, Ordering::Relaxed);

        // Start audio filter if configured
        if let Some(ref mut _filter) = self.params.audio_in_filter {
            // TODO: Implement start method for AudioFilterHandle
            // filter.start(sample_rate).await?;
        }

        Ok(())
    }

    /// Stop the input transport and cleanup resources.
    pub async fn stop(&mut self) -> Result<(), String> {
        // Cancel and wait for the audio input task to finish
        self.cancel_audio_task().await?;

        // Stop audio filter if configured
        if let Some(ref mut _filter) = self.params.audio_in_filter {
            // TODO: Implement stop method for AudioFilterHandle
            // filter.stop().await?;
        }

        Ok(())
    }

    /// Pause the input transport temporarily.
    pub async fn pause(&mut self) -> Result<(), String> {
        self.paused.store(true, Ordering::Relaxed);

        // Cancel task so we clear the queue
        self.cancel_audio_task().await?;

        // Restart the task
        self.create_audio_task().await?;

        Ok(())
    }

    /// Cancel the input transport and stop all processing.
    pub async fn cancel(&mut self) -> Result<(), String> {
        // Cancel and wait for the audio input task to finish
        self.cancel_audio_task().await
    }

    /// Called when the transport is ready to stream.
    pub async fn set_transport_ready(&mut self) -> Result<(), String> {
        // Create audio input queue and task if needed
        self.create_audio_task().await
    }

    /// Push a video frame downstream if video input is enabled.
    ///
    /// # Arguments
    ///
    /// * `frame` - The input video frame to process
    pub async fn push_video_frame(&mut self, frame: InputImageRawFrame) -> Result<(), String> {
        if self.params.video_in_enabled && !self.paused.load(Ordering::Relaxed) {
            self.frame_processor
                .push_frame(FrameType::InputImageRaw(frame), FrameDirection::Downstream)
                .await?;
        }
        Ok(())
    }

    /// Push an audio frame to the processing queue if audio input is enabled.
    ///
    /// # Arguments
    ///
    /// * `frame` - The input audio frame to process
    pub async fn push_audio_frame(&mut self, frame: InputAudioRawFrame) -> Result<(), String> {
        if self.params.audio_in_enabled && !self.paused.load(Ordering::Relaxed) {
            if let Some(ref sender) = self.audio_in_queue {
                sender
                    .send(frame)
                    .map_err(|_| "Failed to send audio frame to queue")?;
            }
        }
        Ok(())
    }

    /// Handle bot interruption frames.
    async fn handle_bot_interruption(&mut self) -> Result<(), String> {
        log::debug!("Bot interruption");
        if self.frame_processor.interruptions_allowed() {
            self.frame_processor.start_interruption().await?;
            self.frame_processor
                .push_frame(
                    FrameType::StartInterruption(StartInterruptionFrame::new()),
                    FrameDirection::Downstream,
                )
                .await?;
        }
        Ok(())
    }

    /// Handle user interruption events based on speaking state.
    async fn handle_user_interruption(&mut self, frame: FrameType) -> Result<(), String> {
        match &frame {
            FrameType::UserStartedSpeaking(_) => {
                log::debug!("User started speaking");
                self.user_speaking.store(true, Ordering::Relaxed);
                self.frame_processor
                    .push_frame(frame, FrameDirection::Downstream)
                    .await?;

                // Only push StartInterruptionFrame if bot is not speaking
                let should_push_immediate_interruption = !self.bot_speaking.load(Ordering::Relaxed);

                if should_push_immediate_interruption
                    && self.frame_processor.interruptions_allowed()
                {
                    self.frame_processor.start_interruption().await?;
                    self.frame_processor
                        .push_frame(
                            FrameType::StartInterruption(StartInterruptionFrame::new()),
                            FrameDirection::Downstream,
                        )
                        .await?;
                } else if self.bot_speaking.load(Ordering::Relaxed) {
                    log::debug!(
                        "User started speaking while bot is speaking - deferring interruption"
                    );
                }
            }
            FrameType::UserStoppedSpeaking(_) => {
                log::debug!("User stopped speaking");
                self.user_speaking.store(false, Ordering::Relaxed);
                self.frame_processor
                    .push_frame(frame, FrameDirection::Downstream)
                    .await?;

                if self.frame_processor.interruptions_allowed() {
                    self.frame_processor
                        .push_frame(
                            FrameType::StopInterruption(StopInterruptionFrame::new()),
                            FrameDirection::Downstream,
                        )
                        .await?;
                }
            }
            _ => {
                return Err("Invalid frame type for user interruption".to_string());
            }
        }
        Ok(())
    }

    /// Update bot speaking state when bot starts speaking.
    async fn handle_bot_started_speaking(&mut self) -> Result<(), String> {
        self.bot_speaking.store(true, Ordering::Relaxed);
        Ok(())
    }

    /// Update bot speaking state when bot stops speaking.
    async fn handle_bot_stopped_speaking(&mut self) -> Result<(), String> {
        self.bot_speaking.store(false, Ordering::Relaxed);
        Ok(())
    }

    /// Create the audio processing task if audio input is enabled.
    async fn create_audio_task(&mut self) -> Result<(), String> {
        if self.audio_task.is_none() && self.params.audio_in_enabled {
            let (sender, receiver) = mpsc::unbounded_channel();
            self.audio_in_queue = Some(sender);

            // Create audio processing task
            let task_handle = self
                .frame_processor
                .create_task(
                    move |_ctx| Self::audio_task_handler(receiver),
                    Some("audio_input".to_string()),
                )
                .await?;

            self.audio_task = Some(task_handle);
        }
        Ok(())
    }

    /// Cancel and cleanup the audio processing task.
    async fn cancel_audio_task(&mut self) -> Result<(), String> {
        if let Some(task) = self.audio_task.take() {
            self.frame_processor
                .cancel_task(&task, Some(Duration::from_secs(5)))
                .await?;
        }
        self.audio_in_queue = None;
        Ok(())
    }

    /// Main audio processing task handler.
    async fn audio_task_handler(mut receiver: mpsc::UnboundedReceiver<InputAudioRawFrame>) {
        loop {
            match timeout(
                Duration::from_secs(AUDIO_INPUT_TIMEOUT_SECS),
                receiver.recv(),
            )
            .await
            {
                Ok(Some(frame)) => {
                    // TODO: Add audio filter processing here when implemented
                    // TODO: Add VAD analysis here when implemented
                    // TODO: Add turn analysis here when implemented

                    // For now, just log that we received a frame
                    log::trace!("Received audio frame with {} bytes", frame.audio.len());
                }
                Ok(None) => {
                    // Channel closed, exit task
                    log::debug!("Audio input channel closed, exiting task");
                    break;
                }
                Err(_) => {
                    // Timeout - check if user is speaking and force stop if needed
                    log::warn!("Audio input timeout - no frames received");
                    // TODO: Handle timeout case for user speaking state
                }
            }
        }
    }

    // Async methods - delegate to frame_processor
    pub async fn create_task<F, Fut>(
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
}

#[async_trait]
impl FrameProcessorTrait for BaseInputTransport {
    // Use delegate macro for all sync methods
    delegate! {
        to self.frame_processor {
            fn can_generate_metrics(&self) -> bool;
            fn id(&self) -> u64;
            fn is_started(&self) -> bool;
            fn is_cancelling(&self) -> bool;
            fn set_allow_interruptions(&mut self, allow: bool);
            fn set_enable_metrics(&mut self, enable: bool);
            fn set_enable_usage_metrics(&mut self, enable: bool);
            fn set_report_only_initial_ttfb(&mut self, report: bool);
            fn set_clock(&mut self, clock: Arc<dyn BaseClock>);
            fn set_task_manager(&mut self, task_manager: Arc<TaskManager>);
            fn add_processor(&mut self, processor: Arc<Mutex<dyn FrameProcessorTrait>>);
            fn clear_processors(&mut self);
            fn is_compound_processor(&self) -> bool;
            fn processor_count(&self) -> usize;
            fn get_processor(&self, index: usize) -> Option<&Arc<Mutex<dyn FrameProcessorTrait>>>;
            fn processors(&self) -> Vec<Arc<Mutex<dyn FrameProcessorTrait>>>;
            fn link(&mut self, next: Arc<Mutex<dyn FrameProcessorTrait>>);
            fn add_interruption_strategy(&mut self, strategy: Arc<dyn BaseInterruptionStrategy>);
            fn name(&self) -> &str;
        }
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

    // async fn cleanup_all_processors(&self) -> Result<(), String> {
    //     self.frame_processor.cleanup_all_processors().await
    // }

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

    async fn queue_frame(
        &mut self,
        frame: FrameType,
        direction: FrameDirection,
        callback: Option<FrameCallback>,
    ) -> Result<(), String> {
        self.frame_processor
            .queue_frame(frame, direction, callback)
            .await
    }

    // Custom process_frame implementation - not delegated

    async fn process_frame(
        &mut self,
        frame: FrameType,
        direction: FrameDirection,
    ) -> Result<(), String> {
        // Handle specific system frames
        match frame {
            FrameType::Start(start_frame) => {
                // Push StartFrame before start(), because we want StartFrame to be
                // processed by every processor before any other frame is processed
                let frame_copy = FrameType::Start(start_frame.clone());
                self.frame_processor
                    .push_frame(frame_copy, direction)
                    .await?;
                self.start(&start_frame).await?;
            }
            FrameType::Cancel(_) => {
                self.cancel().await?;
                self.frame_processor.push_frame(frame, direction).await?;
            }
            FrameType::BotInterruption(_) => {
                self.handle_bot_interruption().await?;
            }
            FrameType::BotStartedSpeaking(_) => {
                self.handle_bot_started_speaking().await?;
                self.frame_processor.push_frame(frame, direction).await?;
            }
            FrameType::BotStoppedSpeaking(_) => {
                self.handle_bot_stopped_speaking().await?;
                self.frame_processor.push_frame(frame, direction).await?;
            }
            FrameType::EmulateUserStartedSpeaking(_) => {
                log::debug!("Emulating user started speaking");
                let user_frame =
                    FrameType::UserStartedSpeaking(UserStartedSpeakingFrame::new(true));
                self.handle_user_interruption(user_frame).await?;
            }
            FrameType::EmulateUserStoppedSpeaking(_) => {
                log::debug!("Emulating user stopped speaking");
                let user_frame =
                    FrameType::UserStoppedSpeaking(UserStoppedSpeakingFrame::new(true));
                self.handle_user_interruption(user_frame).await?;
            }
            // All other system frames
            FrameType::Error(_)
            | FrameType::StartInterruption(_)
            | FrameType::StopInterruption(_) => {
                self.frame_processor.push_frame(frame, direction).await?;
            }
            // Control frames
            FrameType::End(_) => {
                // Push EndFrame before stop(), because stop() waits on the task to
                // finish and the task finishes when EndFrame is processed
                self.frame_processor.push_frame(frame, direction).await?;
                self.stop().await?;
            }
            FrameType::Stop(_) => {
                self.frame_processor.push_frame(frame, direction).await?;
                self.pause().await?;
            }
            // Other frames - pass through
            _ => {
                self.frame_processor.push_frame(frame, direction).await?;
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::task_manager::TaskManagerConfig;

    #[tokio::test]
    async fn test_base_input_transport_creation() {
        let task_manager = Arc::new(TaskManager::new(TaskManagerConfig::default()));
        let params = TransportParams::default();
        let transport = BaseInputTransport::new(params, task_manager, None);

        assert_eq!(transport.name(), "BaseInputTransport");
        assert_eq!(transport.sample_rate(), 0);
        assert!(!transport.bot_speaking.load(Ordering::Relaxed));
        assert!(!transport.user_speaking.load(Ordering::Relaxed));
        assert!(!transport.paused.load(Ordering::Relaxed));
    }

    #[tokio::test]
    async fn test_audio_stream_enablement() {
        let task_manager = Arc::new(TaskManager::new(TaskManagerConfig::default()));
        let params = TransportParams::default();
        let mut transport = BaseInputTransport::new(params, task_manager, None);

        transport.enable_audio_in_stream_on_start(true);
        assert!(transport.params.audio_in_stream_on_start);

        transport.enable_audio_in_stream_on_start(false);
        assert!(!transport.params.audio_in_stream_on_start);
    }

    #[tokio::test]
    async fn test_start_and_stop() {
        let task_manager = Arc::new(TaskManager::new(TaskManagerConfig::default()));
        let params = TransportParams::default();
        let mut transport = BaseInputTransport::new(params, task_manager, None);

        let start_frame = StartFrame::new().with_sample_rates(48000, 24000);

        transport.start(&start_frame).await.unwrap();
        assert_eq!(transport.sample_rate(), 48000);
        assert!(!transport.paused.load(Ordering::Relaxed));

        transport.stop().await.unwrap();
    }

    #[tokio::test]
    async fn test_bot_speaking_state() {
        let task_manager = Arc::new(TaskManager::new(TaskManagerConfig::default()));
        let params = TransportParams::default();
        let mut transport = BaseInputTransport::new(params, task_manager, None);

        assert!(!transport.bot_speaking.load(Ordering::Relaxed));

        transport.handle_bot_started_speaking().await.unwrap();
        assert!(transport.bot_speaking.load(Ordering::Relaxed));

        transport.handle_bot_stopped_speaking().await.unwrap();
        assert!(!transport.bot_speaking.load(Ordering::Relaxed));
    }

    #[tokio::test]
    async fn test_pause_and_resume() {
        let task_manager = Arc::new(TaskManager::new(TaskManagerConfig::default()));
        let params = TransportParams::default();
        let mut transport = BaseInputTransport::new(params, task_manager, None);

        transport.pause().await.unwrap();
        assert!(transport.paused.load(Ordering::Relaxed));

        let start_frame = StartFrame::new();
        transport.start(&start_frame).await.unwrap();
        assert!(!transport.paused.load(Ordering::Relaxed));
    }
}
