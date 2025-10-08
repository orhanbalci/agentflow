//! Rust implementation of the Python BaseOutputTransport class.
//!
//! This file provides a refactored implementation using a composite pattern
//! to separate transport and media sender concerns for better async resource management.

use async_trait::async_trait;
use delegate::delegate;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Arc;
use tokio::sync::Mutex;

use crate::frames::{
    CancelFrame, EndFrame, Frame, FrameType, OutputAudioRawFrame, OutputImageRawFrame,
    OutputTransportReadyFrame, SpriteFrame, StartFrame, TransportMessageFrame,
    TransportMessageUrgentFrame,
};
use crate::processors::frame::{
    BaseInterruptionStrategy, FrameCallback, FrameDirection, FrameProcessor,
    FrameProcessorInterface, FrameProcessorMetrics, FrameProcessorSetup, FrameProcessorTrait,
};
use crate::task_manager::{TaskHandle, TaskManager};
use crate::transport::media_sender::MediaSender;
use crate::transport::params::TransportParams;
use crate::{BaseClock, SystemClock};

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

/// Internal shared state for BaseOutputTransport
struct BaseOutputTransportInner {
    params: TransportParams,
    sample_rate: AtomicU32,
    audio_chunk_size: AtomicU32,
    /// Map of destinations to their media senders
    media_senders: HashMap<Option<String>, MediaSender>,
    clock: Arc<SystemClock>,
    stopped: AtomicBool,
}

/// Lightweight base output transport implementation using composite pattern.
///
/// This struct manages the overall transport coordination while delegating
/// media processing to individual MediaSender instances per destination.
pub struct BaseOutputTransport {
    inner: Arc<Mutex<BaseOutputTransportInner>>,
    pub frame_processor: Arc<Mutex<FrameProcessor>>,
    #[allow(dead_code)]
    task_manager: Arc<TaskManager>,
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

        let inner = BaseOutputTransportInner {
            params,
            sample_rate: AtomicU32::new(0),
            audio_chunk_size: AtomicU32::new(0),
            media_senders: HashMap::new(),
            clock: Arc::new(SystemClock::new()),
            stopped: AtomicBool::new(false),
        };

        Arc::new(Self {
            inner: Arc::new(Mutex::new(inner)),
            frame_processor: Arc::new(Mutex::new(frame_processor)),
            task_manager,
        })
    }

    /// Get the sample rate for audio output
    pub async fn sample_rate(&self) -> u32 {
        let inner = self.inner.lock().await;
        inner.sample_rate.load(Ordering::Relaxed)
    }

    /// Get the audio chunk size
    pub async fn audio_chunk_size(&self) -> usize {
        let inner = self.inner.lock().await;
        inner.audio_chunk_size.load(Ordering::Relaxed) as usize
    }

    /// Start the output transport and initialize components.
    pub async fn start(
        &self,
        frame: &StartFrame,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let mut inner = self.inner.lock().await;

        // Set sample rate from params or frame
        let sample_rate = inner
            .params
            .audio_out_sample_rate
            .unwrap_or(frame.audio_out_sample_rate);
        inner.sample_rate.store(sample_rate, Ordering::Relaxed);

        // Calculate audio chunk size
        let audio_bytes_10ms =
            (sample_rate as f64 / 100.0) as u32 * (inner.params.audio_out_channels as u32) * 2;
        let audio_chunk_size = audio_bytes_10ms * (inner.params.audio_out_10ms_chunks as u32);
        inner
            .audio_chunk_size
            .store(audio_chunk_size, Ordering::Relaxed);

        // Initialize media senders for all destinations
        self.ensure_media_senders(&mut inner, sample_rate, audio_chunk_size)
            .await?;

        // Start all media senders
        for (_, sender) in inner.media_senders.iter_mut() {
            sender.start(frame).await?;
        }

        Ok(())
    }

    /// Stop the output transport and cleanup resources.
    pub async fn stop(
        &self,
        frame: &EndFrame,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let mut inner = self.inner.lock().await;

        // Mark as stopped
        inner.stopped.store(true, Ordering::Relaxed);

        // Stop all media senders
        for (destination, sender) in inner.media_senders.iter_mut() {
            log::debug!("Stopping destination: {:?}", destination);
            sender.stop(frame).await?;
        }

        Ok(())
    }

    /// Cancel the output transport and stop all processing.
    pub async fn cancel(
        &self,
        _frame: &CancelFrame,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let mut inner = self.inner.lock().await;

        // Mark as stopped
        inner.stopped.store(true, Ordering::Relaxed);

        // Cancel all media senders
        for (destination, sender) in inner.media_senders.iter_mut() {
            log::debug!("Cancelling destination: {:?}", destination);
            let cancel_frame = crate::frames::CancelFrame::new();
            sender.cancel(&cancel_frame).await?;
        }

        // Clear the senders map immediately on cancel
        inner.media_senders.clear();

        Ok(())
    }

    /// Called when the transport is ready to stream.
    ///
    /// This method follows the Python implementation more closely by creating
    /// media senders dynamically when the transport becomes ready.
    pub async fn set_transport_ready(
        self: &Arc<Self>,
        frame: &StartFrame,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let mut inner = self.inner.lock().await;

        // Register destinations (would be implemented by concrete transports)
        for destination in &inner.params.audio_out_destinations {
            self.register_audio_destination(destination).await?;
        }

        for destination in &inner.params.video_out_destinations {
            self.register_video_destination(destination).await?;
        }

        // Create a weak reference to the transport for MediaSenders
        let transport_weak = Arc::downgrade(self);

        // Start default media sender (destination = None)
        if !inner.media_senders.contains_key(&None) {
            let mut sender = MediaSender::new(
                None,
                inner.sample_rate.load(Ordering::Relaxed),
                inner.audio_chunk_size.load(Ordering::Relaxed),
                inner.params.clone(),
                transport_weak.clone(),
            );
            sender.start(frame).await?;
            inner.media_senders.insert(None, sender);
        }

        // Media senders handle both audio and video, so make sure we only
        // have one media sender per shared name.
        let mut destinations = inner.params.audio_out_destinations.clone();
        destinations.extend(inner.params.video_out_destinations.clone());
        destinations.sort();
        destinations.dedup();

        // Start media senders for each destination
        for destination in destinations {
            if !inner.media_senders.contains_key(&Some(destination.clone())) {
                let mut sender = MediaSender::new(
                    Some(destination.clone()),
                    inner.sample_rate.load(Ordering::Relaxed),
                    inner.audio_chunk_size.load(Ordering::Relaxed),
                    inner.params.clone(),
                    transport_weak.clone(),
                );
                sender.start(frame).await?;
                inner.media_senders.insert(Some(destination), sender);
            }
        }

        drop(inner); // Release the lock before calling push_frame

        // Send a frame indicating that the output transport is ready and able to receive frames
        let ready_frame = OutputTransportReadyFrame::new();
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

    /// Internal helper to initialize media senders for destinations.
    async fn ensure_media_senders(
        &self,
        inner: &mut BaseOutputTransportInner,
        sample_rate: u32,
        audio_chunk_size: u32,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        // For now, we'll create a dummy weak reference - this will be fixed when we properly
        // integrate with the transport. The MediaSender will need to be refactored to work
        // with the composite pattern properly.
        let transport_weak = std::sync::Weak::<BaseOutputTransport>::new();

        // Create default destination sender (None destination)
        if !inner.media_senders.contains_key(&None) {
            let sender = MediaSender::new(
                None,
                sample_rate,
                audio_chunk_size,
                inner.params.clone(),
                transport_weak.clone(),
            );
            inner.media_senders.insert(None, sender);
        }

        // Create senders for audio destinations
        for dest in &inner.params.audio_out_destinations {
            if !inner.media_senders.contains_key(&Some(dest.clone())) {
                let sender = MediaSender::new(
                    Some(dest.clone()),
                    sample_rate,
                    audio_chunk_size,
                    inner.params.clone(),
                    transport_weak.clone(),
                );
                inner.media_senders.insert(Some(dest.clone()), sender);
            }
        }

        // Create senders for video destinations
        for dest in &inner.params.video_out_destinations {
            if !inner.media_senders.contains_key(&Some(dest.clone())) {
                let sender = MediaSender::new(
                    Some(dest.clone()),
                    sample_rate,
                    audio_chunk_size,
                    inner.params.clone(),
                    transport_weak.clone(),
                );
                inner.media_senders.insert(Some(dest.clone()), sender);
            }
        }

        Ok(())
    }

    /// Get the clock for timing
    pub async fn get_clock(&self) -> Arc<dyn BaseClock> {
        let inner = self.inner.lock().await;
        inner.clock.clone()
    }

    /// Check if interruptions are allowed
    pub async fn interruptions_allowed(&self) -> bool {
        // Default to true since allow_interruptions field doesn't exist in params
        true
    }

    /// Send a transport message.
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

    /// Write audio frame - returns whether frame was written successfully
    pub async fn write_audio_frame(
        &self,
        _frame: &OutputAudioRawFrame,
    ) -> Result<bool, Box<dyn std::error::Error + Send + Sync>> {
        // Placeholder implementation - should be overridden by concrete transports
        Ok(false)
    }

    /// Write video frame - returns whether frame was written successfully  
    pub async fn write_video_frame(
        &self,
        _frame: &OutputImageRawFrame,
    ) -> Result<bool, Box<dyn std::error::Error + Send + Sync>> {
        // Placeholder implementation - should be overridden by concrete transports
        Ok(false)
    }

    /// Write a DTMF tone using the transport's preferred method.
    ///
    /// This method first checks if the transport supports native DTMF,
    /// and falls back to audio-based DTMF generation if not.
    pub async fn write_dtmf(
        &self,
        _frame: FrameType, // Would be OutputDTMFFrame or OutputDTMFUrgentFrame
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        if self.supports_native_dtmf().await {
            self.write_dtmf_native(_frame).await
        } else {
            self.write_dtmf_audio(_frame).await
        }
    }

    /// Check if the transport supports native DTMF.
    ///
    /// Override in transport implementations that support native DTMF.
    pub async fn supports_native_dtmf(&self) -> bool {
        false
    }

    /// Write DTMF using native transport support.
    ///
    /// Override in transport implementations for native DTMF.
    pub async fn write_dtmf_native(
        &self,
        _frame: FrameType,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        Err("Transport claims native DTMF support but doesn't implement it".into())
    }

    /// Generate and send audio tones for DTMF.
    ///
    /// This method generates audio tones for DTMF when native support is not available.
    pub async fn write_dtmf_audio(
        &self,
        _frame: FrameType,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        // Placeholder implementation - would generate DTMF audio tones
        // let dtmf_audio = load_dtmf_audio(frame.button, self.sample_rate().await).await?;
        // let dtmf_audio_frame = OutputAudioRawFrame::new(
        //     dtmf_audio,
        //     self.sample_rate().await,
        //     1, // mono
        // );
        // self.write_audio_frame(&dtmf_audio_frame).await?;
        Ok(())
    }

    /// Send an audio frame downstream.
    pub async fn send_audio(
        &self,
        frame: OutputAudioRawFrame,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        self.push_frame(FrameType::OutputAudioRaw(frame), FrameDirection::Downstream)
            .await
            .map_err(|e| {
                Box::new(std::io::Error::new(std::io::ErrorKind::Other, e))
                    as Box<dyn std::error::Error + Send + Sync>
            })
    }

    /// Send an image frame downstream.
    pub async fn send_image(
        &self,
        frame: ImageFrameType,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
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

    /// Handle frames by delegating to appropriate media sender processing.
    ///
    /// This method routes frames to the correct media sender based on the
    /// transport destination, following the Python implementation pattern.
    async fn _handle_frame(
        &self,
        frame: FrameType,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        // Get the transport destination from the frame
        let transport_destination = frame.transport_destination().map(|s| s.to_string());

        // Check if we have a media sender for this destination
        let inner = self.inner.lock().await;
        if !inner.media_senders.contains_key(&transport_destination) {
            log::warn!(
                "Destination {:?} not registered for frame {}",
                transport_destination,
                frame.name()
            );
            return Ok(());
        }
        drop(inner); // Release the lock early

        // Get the media sender for this destination
        let mut inner = self.inner.lock().await;
        let sender = inner.media_senders.get_mut(&transport_destination);

        if let Some(sender) = sender {
            // Route frame to appropriate handler based on frame type (following Python pattern)
            match frame {
                // Interruption frames
                FrameType::StartInterruption(_)
                | FrameType::StopInterruption(_)
                | FrameType::BotInterruption(_) => {
                    sender.handle_interruptions(frame).await?;
                }
                // Audio frames
                FrameType::OutputAudioRaw(audio_frame) => {
                    log::debug!(
                        "Handling audio frame for destination: {:?}",
                        transport_destination
                    );
                    sender.handle_audio_frame(&audio_frame).await?;
                }
                FrameType::TTSAudioRaw(tts_frame) => {
                    log::debug!(
                        "Handling TTS audio frame for destination: {:?}",
                        transport_destination
                    );
                    sender
                        .handle_audio_frame(&tts_frame.output_audio_frame)
                        .await?;
                }
                FrameType::SpeechOutputAudioRaw(speech_frame) => {
                    log::debug!(
                        "Handling speech audio frame for destination: {:?}",
                        transport_destination
                    );
                    sender
                        .handle_audio_frame(&speech_frame.output_audio_frame)
                        .await?;
                }
                // Image/Video frames
                FrameType::OutputImageRaw(image_frame) => {
                    log::debug!(
                        "Handling image frame for destination: {:?}",
                        transport_destination
                    );
                    sender
                        .handle_image_frame(FrameType::OutputImageRaw(image_frame.clone()))
                        .await?;
                }
                FrameType::Sprite(sprite_frame) => {
                    log::debug!(
                        "Handling sprite frame for destination: {:?}",
                        transport_destination
                    );
                    sender
                        .handle_image_frame(FrameType::Sprite(sprite_frame.clone()))
                        .await?;
                }
                // Mixer control frames (when available)
                // FrameType::MixerControl(mixer_frame) => {
                //     sender.handle_mixer_control_frame(frame).await?;
                // }
                // Frames with presentation timestamps
                _ if frame.pts().is_some() => {
                    log::debug!(
                        "Handling timed frame for destination: {:?}",
                        transport_destination
                    );
                    sender.handle_timed_frame(frame).await?;
                }
                // All other frames (sync frames)
                _ => {
                    log::debug!(
                        "Handling sync frame for destination: {:?}",
                        transport_destination
                    );
                    sender.handle_sync_frame(frame).await?;
                }
            }
        }

        Ok(())
    }

    /// Static helper for drawing image with resizing (for use in video tasks)
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

        log::debug!(
            "Processed image frame for destination: {:?}, final size: {}x{}, format: {:?}",
            destination,
            processed_frame.image_frame.size.0,
            processed_frame.image_frame.size.1,
            processed_frame.image_frame.format
        );

        Ok(())
    }

    /// Resize frame data using the image crate (real implementation using thread pool)
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
    fn resize_frame_data_blocking(
        data: Vec<u8>,
        current_size: (u32, u32),
        desired_size: (u32, u32),
        format: Option<String>,
    ) -> Result<Vec<u8>, Box<dyn std::error::Error + Send + Sync>> {
        let (width, height) = current_size;
        let input_size = data.len();

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

        let resized = dynamic_img.resize(
            desired_size.0,
            desired_size.1,
            image::imageops::FilterType::Lanczos3,
        );

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
}

#[async_trait]
impl FrameProcessorTrait for BaseOutputTransport {
    fn name(&self) -> &str {
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
                self.start(start_frame).await.map_err(|e| e.to_string())?;
            }
            FrameType::Cancel(cancel_frame) => {
                self.cancel(cancel_frame).await.map_err(|e| e.to_string())?;
                self.push_frame(FrameType::Cancel(cancel_frame.clone()), direction)
                    .await
                    .map_err(|e| e.to_string())?;
            }
            // Handle interruption frames
            FrameType::StartInterruption(_) => {
                self.push_frame(frame.clone(), direction)
                    .await
                    .map_err(|e| e.to_string())?;
                self._handle_frame(frame.clone())
                    .await
                    .map_err(|e| e.to_string())?;
            }
            FrameType::StopInterruption(_) => {
                self.push_frame(frame.clone(), direction)
                    .await
                    .map_err(|e| e.to_string())?;
                self._handle_frame(frame.clone())
                    .await
                    .map_err(|e| e.to_string())?;
            }
            FrameType::BotInterruption(_) => {
                self.push_frame(frame.clone(), direction)
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
            // System frames - frames that should be pushed through
            FrameType::OutputTransportReady(_)
            | FrameType::UserStartedSpeaking(_)
            | FrameType::UserStoppedSpeaking(_)
            | FrameType::BotStartedSpeaking(_)
            | FrameType::BotSpeaking(_)
            | FrameType::BotStoppedSpeaking(_)
            | FrameType::EmulateUserStartedSpeaking(_)
            | FrameType::EmulateUserStoppedSpeaking(_) => {
                self.push_frame(frame.clone(), direction)
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
            // TODO: Add mixer control frame handling when available
            // FrameType::MixerControl(_) => {
            //     self._handle_frame(frame.clone())
            //         .await
            //         .map_err(|e| e.to_string())?;
            // }
            // Handle media frames
            FrameType::OutputAudioRaw(_) => {
                self._handle_frame(frame.clone())
                    .await
                    .map_err(|e| e.to_string())?;
            }
            FrameType::OutputImageRaw(_) | FrameType::Sprite(_) => {
                self._handle_frame(frame.clone())
                    .await
                    .map_err(|e| e.to_string())?;
            }
            FrameType::TTSAudioRaw(_) | FrameType::SpeechOutputAudioRaw(_) => {
                self._handle_frame(frame.clone())
                    .await
                    .map_err(|e| e.to_string())?;
            }
            // Handle frames with presentation timestamps
            _ if frame.pts().is_some() => {
                self._handle_frame(frame.clone())
                    .await
                    .map_err(|e| e.to_string())?;
            }
            // Handle upstream frames
            _ if direction == FrameDirection::Upstream => {
                self.push_frame(frame.clone(), direction)
                    .await
                    .map_err(|e| e.to_string())?;
            }
            // Handle all other frames
            _ => {
                self._handle_frame(frame.clone())
                    .await
                    .map_err(|e| e.to_string())?;
            }
        }

        Ok(())
    }
}

// Implement the FrameProcessorInterface trait for BaseOutputTransport
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
