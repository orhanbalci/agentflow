use crate::audio::filter::BaseAudioFilter;
use crate::audio::mixer::BaseAudioMixer;
use std::collections::HashMap;
use std::sync::Arc;

/// Configuration parameters for transport implementations.
///
/// # Parameters
///
/// * `audio_out_enabled` - Enable audio output streaming.
/// * `audio_out_sample_rate` - Output audio sample rate in Hz.
/// * `audio_out_channels` - Number of output audio channels.
/// * `audio_out_bitrate` - Output audio bitrate in bits per second.
/// * `audio_out_10ms_chunks` - Number of 10ms chunks to buffer for output.
/// * `audio_out_mixer` - Audio mixer instance or destination mapping.
/// * `audio_out_destinations` - List of audio output destination identifiers.
/// * `audio_in_enabled` - Enable audio input streaming.
/// * `audio_in_sample_rate` - Input audio sample rate in Hz.
/// * `audio_in_channels` - Number of input audio channels.
/// * `audio_in_filter` - Audio filter to apply to input audio.
/// * `audio_in_stream_on_start` - Start audio streaming immediately on transport start.
/// * `audio_in_passthrough` - Pass through input audio frames downstream.
/// * `video_in_enabled` - Enable video input streaming.
/// * `video_out_enabled` - Enable video output streaming.
/// * `video_out_is_live` - Enable real-time video output streaming.
/// * `video_out_width` - Video output width in pixels.
/// * `video_out_height` - Video output height in pixels.
/// * `video_out_bitrate` - Video output bitrate in bits per second.
/// * `video_out_framerate` - Video output frame rate in FPS.
/// * `video_out_color_format` - Video output color format string.
/// * `video_out_destinations` - List of video output destination identifiers.
/// * `vad_analyzer` - Voice Activity Detection analyzer instance.
/// * `turn_analyzer` - Turn-taking analyzer instance for conversation management.
#[derive(Debug, Clone)]
pub struct TransportParams {
    // Audio output parameters
    pub audio_out_enabled: bool,
    pub audio_out_sample_rate: Option<u32>,
    pub audio_out_channels: u32,
    pub audio_out_bitrate: u32,
    pub audio_out_10ms_chunks: u32,
    pub audio_out_mixer: Option<AudioMixerConfig>,
    pub audio_out_destinations: Vec<String>,

    // Audio input parameters
    pub audio_in_enabled: bool,
    pub audio_in_sample_rate: Option<u32>,
    pub audio_in_channels: u32,
    pub audio_in_filter: Option<AudioFilterHandle>,
    pub audio_in_stream_on_start: bool,
    pub audio_in_passthrough: bool,

    // Video parameters
    pub video_in_enabled: bool,
    pub video_out_enabled: bool,
    pub video_out_is_live: bool,
    pub video_out_width: u32,
    pub video_out_height: u32,
    pub video_out_bitrate: u32,
    pub video_out_framerate: u32,
    pub video_out_color_format: String,
    pub video_out_destinations: Vec<String>,

    // Analyzer parameters
    pub vad_analyzer: Option<VadAnalyzerHandle>,
    pub turn_analyzer: Option<TurnAnalyzerHandle>,
}

impl Default for TransportParams {
    fn default() -> Self {
        Self {
            // Audio output defaults
            audio_out_enabled: false,
            audio_out_sample_rate: None,
            audio_out_channels: 1,
            audio_out_bitrate: 96000,
            audio_out_10ms_chunks: 4,
            audio_out_mixer: None,
            audio_out_destinations: Vec::new(),

            // Audio input defaults
            audio_in_enabled: false,
            audio_in_sample_rate: None,
            audio_in_channels: 1,
            audio_in_filter: None,
            audio_in_stream_on_start: true,
            audio_in_passthrough: true,

            // Video defaults
            video_in_enabled: false,
            video_out_enabled: false,
            video_out_is_live: false,
            video_out_width: 1024,
            video_out_height: 768,
            video_out_bitrate: 800000,
            video_out_framerate: 30,
            video_out_color_format: "RGB".to_string(),
            video_out_destinations: Vec::new(),

            // Analyzer defaults
            vad_analyzer: None,
            turn_analyzer: None,
        }
    }
}

/// Audio mixer configuration that can be either a single mixer or a mapping
#[derive(Debug, Clone)]
pub enum AudioMixerConfig {
    Single(Arc<dyn BaseAudioMixer>),
    Mapping(HashMap<Option<String>, Arc<dyn BaseAudioMixer>>),
}

/// Audio filter configuration using BaseAudioFilter trait
pub type AudioFilterHandle = Arc<dyn BaseAudioFilter>;

/// Handle for Voice Activity Detection analyzer implementations
#[derive(Debug, Clone)]
pub struct VadAnalyzerHandle {
    pub id: String,
    pub config: VadAnalyzerType,
}

/// Types of VAD analyzers
#[derive(Debug, Clone)]
pub enum VadAnalyzerType {
    Webrtc,
    Silero,
    Custom(String),
}

/// Handle for turn-taking analyzer implementations
#[derive(Debug, Clone)]
pub struct TurnAnalyzerHandle {
    pub id: String,
    pub config: TurnAnalyzerType,
}

/// Types of turn analyzers
#[derive(Debug, Clone)]
pub enum TurnAnalyzerType {
    Simple,
    Contextual,
    Custom(String),
}

/// Represents the state of a conversation turn
#[derive(Debug, Clone, PartialEq)]
pub enum TurnState {
    Speaking,
    Listening,
    Transition,
}

/// Trait for audio mixer implementations
pub trait AudioMixer: Send + Sync {
    fn mix(&self, inputs: Vec<&[f32]>) -> Vec<f32>;
}

/// Trait for Voice Activity Detection analyzer implementations
pub trait VadAnalyzer: Send + Sync {
    fn analyze(&self, audio: &[f32]) -> bool;
}

/// Trait for turn-taking analyzer implementations
pub trait TurnAnalyzer: Send + Sync {
    fn analyze_turn(&self, audio: &[f32], text: Option<&str>) -> TurnState;
}

// Implementations for handle types
impl VadAnalyzerHandle {
    pub fn webrtc(id: String) -> Self {
        Self {
            id,
            config: VadAnalyzerType::Webrtc,
        }
    }

    pub fn silero(id: String) -> Self {
        Self {
            id,
            config: VadAnalyzerType::Silero,
        }
    }

    pub fn custom(id: String, custom_type: String) -> Self {
        Self {
            id,
            config: VadAnalyzerType::Custom(custom_type),
        }
    }
}

impl TurnAnalyzerHandle {
    pub fn simple(id: String) -> Self {
        Self {
            id,
            config: TurnAnalyzerType::Simple,
        }
    }

    pub fn contextual(id: String) -> Self {
        Self {
            id,
            config: TurnAnalyzerType::Contextual,
        }
    }

    pub fn custom(id: String, custom_type: String) -> Self {
        Self {
            id,
            config: TurnAnalyzerType::Custom(custom_type),
        }
    }
}

impl TransportParams {
    /// Create a new TransportParams instance with default values
    pub fn new() -> Self {
        Default::default()
    }

    /// Builder method to enable audio output
    pub fn with_audio_out_enabled(mut self, enabled: bool) -> Self {
        self.audio_out_enabled = enabled;
        self
    }

    /// Builder method to set audio output sample rate
    pub fn with_audio_out_sample_rate(mut self, sample_rate: u32) -> Self {
        self.audio_out_sample_rate = Some(sample_rate);
        self
    }

    /// Builder method to enable audio input
    pub fn with_audio_in_enabled(mut self, enabled: bool) -> Self {
        self.audio_in_enabled = enabled;
        self
    }

    /// Builder method to set audio input sample rate
    pub fn with_audio_in_sample_rate(mut self, sample_rate: u32) -> Self {
        self.audio_in_sample_rate = Some(sample_rate);
        self
    }

    /// Builder method to enable video output
    pub fn with_video_out_enabled(mut self, enabled: bool) -> Self {
        self.video_out_enabled = enabled;
        self
    }

    /// Builder method to set video output dimensions
    pub fn with_video_out_dimensions(mut self, width: u32, height: u32) -> Self {
        self.video_out_width = width;
        self.video_out_height = height;
        self
    }

    /// Builder method to enable video input
    pub fn with_video_in_enabled(mut self, enabled: bool) -> Self {
        self.video_in_enabled = enabled;
        self
    }
}
