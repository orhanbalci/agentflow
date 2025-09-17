//! Core frame definitions for the AgentFlow AI framework.
//!
//! This module contains all frame types used throughout the AgentFlow pipeline system,
//! including data frames, system frames, and control frames for audio, video, text,
//! and LLM processing.

use std::collections::HashMap;
use std::fmt;

/// Unique identifier for frame instances
static mut FRAME_COUNTER: u64 = 0;

/// Generate a unique object ID
fn obj_id() -> u64 {
    unsafe {
        FRAME_COUNTER += 1;
        FRAME_COUNTER
    }
}

/// DTMF keypad entries for phone system integration
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeypadEntry {
    One,
    Two,
    Three,
    Four,
    Five,
    Six,
    Seven,
    Eight,
    Nine,
    Zero,
    Pound,
    Star,
}

impl fmt::Display for KeypadEntry {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            KeypadEntry::One => "1",
            KeypadEntry::Two => "2",
            KeypadEntry::Three => "3",
            KeypadEntry::Four => "4",
            KeypadEntry::Five => "5",
            KeypadEntry::Six => "6",
            KeypadEntry::Seven => "7",
            KeypadEntry::Eight => "8",
            KeypadEntry::Nine => "9",
            KeypadEntry::Zero => "0",
            KeypadEntry::Pound => "#",
            KeypadEntry::Star => "*",
        };
        write!(f, "{}", s)
    }
}

/// Base frame trait for all frames in the AgentFlow pipeline
pub trait Frame {
    /// Get the unique identifier for this frame
    fn id(&self) -> u64;

    /// Get the frame name
    fn name(&self) -> &str;

    /// Get the presentation timestamp in nanoseconds
    fn pts(&self) -> Option<u64>;

    /// Set the presentation timestamp
    fn set_pts(&mut self, pts: Option<u64>);

    /// Get frame metadata
    fn metadata(&self) -> &HashMap<String, String>;

    /// Get mutable frame metadata
    fn metadata_mut(&mut self) -> &mut HashMap<String, String>;

    /// Get transport source
    fn transport_source(&self) -> Option<&str>;

    /// Set transport source
    fn set_transport_source(&mut self, source: Option<String>);

    /// Get transport destination
    fn transport_destination(&self) -> Option<&str>;

    /// Set transport destination
    fn set_transport_destination(&mut self, destination: Option<String>);
}

/// Base frame structure with common fields
#[derive(Debug, Clone)]
pub struct BaseFrame {
    pub id: u64,
    pub name: String,
    pub pts: Option<u64>,
    pub metadata: HashMap<String, String>,
    pub transport_source: Option<String>,
    pub transport_destination: Option<String>,
}

impl BaseFrame {
    pub fn new(type_name: &str) -> Self {
        let id = obj_id();
        Self {
            id,
            name: format!("{}#{}", type_name, id),
            pts: None,
            metadata: HashMap::new(),
            transport_source: None,
            transport_destination: None,
        }
    }
}

impl Frame for BaseFrame {
    fn id(&self) -> u64 {
        self.id
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn pts(&self) -> Option<u64> {
        self.pts
    }

    fn set_pts(&mut self, pts: Option<u64>) {
        self.pts = pts;
    }

    fn metadata(&self) -> &HashMap<String, String> {
        &self.metadata
    }

    fn metadata_mut(&mut self) -> &mut HashMap<String, String> {
        &mut self.metadata
    }

    fn transport_source(&self) -> Option<&str> {
        self.transport_source.as_deref()
    }

    fn set_transport_source(&mut self, source: Option<String>) {
        self.transport_source = source;
    }

    fn transport_destination(&self) -> Option<&str> {
        self.transport_destination.as_deref()
    }

    fn set_transport_destination(&mut self, destination: Option<String>) {
        self.transport_destination = destination;
    }
}

impl fmt::Display for BaseFrame {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.name)
    }
}

//
// Frame Categories
//

/// System frame for immediate processing
#[derive(Debug, Clone)]
pub struct SystemFrame {
    pub base: BaseFrame,
}

impl SystemFrame {
    pub fn new(type_name: &str) -> Self {
        Self {
            base: BaseFrame::new(type_name),
        }
    }
}

impl Frame for SystemFrame {
    fn id(&self) -> u64 {
        self.base.id()
    }
    fn name(&self) -> &str {
        self.base.name()
    }
    fn pts(&self) -> Option<u64> {
        self.base.pts()
    }
    fn set_pts(&mut self, pts: Option<u64>) {
        self.base.set_pts(pts)
    }
    fn metadata(&self) -> &HashMap<String, String> {
        self.base.metadata()
    }
    fn metadata_mut(&mut self) -> &mut HashMap<String, String> {
        self.base.metadata_mut()
    }
    fn transport_source(&self) -> Option<&str> {
        self.base.transport_source()
    }
    fn set_transport_source(&mut self, source: Option<String>) {
        self.base.set_transport_source(source)
    }
    fn transport_destination(&self) -> Option<&str> {
        self.base.transport_destination()
    }
    fn set_transport_destination(&mut self, destination: Option<String>) {
        self.base.set_transport_destination(destination)
    }
}

/// Data frame for processing data in order
#[derive(Debug, Clone)]
pub struct DataFrame {
    pub base: BaseFrame,
}

impl DataFrame {
    pub fn new(type_name: &str) -> Self {
        Self {
            base: BaseFrame::new(type_name),
        }
    }
}

impl Frame for DataFrame {
    fn id(&self) -> u64 {
        self.base.id()
    }
    fn name(&self) -> &str {
        self.base.name()
    }
    fn pts(&self) -> Option<u64> {
        self.base.pts()
    }
    fn set_pts(&mut self, pts: Option<u64>) {
        self.base.set_pts(pts)
    }
    fn metadata(&self) -> &HashMap<String, String> {
        self.base.metadata()
    }
    fn metadata_mut(&mut self) -> &mut HashMap<String, String> {
        self.base.metadata_mut()
    }
    fn transport_source(&self) -> Option<&str> {
        self.base.transport_source()
    }
    fn set_transport_source(&mut self, source: Option<String>) {
        self.base.set_transport_source(source)
    }
    fn transport_destination(&self) -> Option<&str> {
        self.base.transport_destination()
    }
    fn set_transport_destination(&mut self, destination: Option<String>) {
        self.base.set_transport_destination(destination)
    }
}

/// Control frame for processing control information in order
#[derive(Debug, Clone)]
pub struct ControlFrame {
    pub base: BaseFrame,
}

impl ControlFrame {
    pub fn new(type_name: &str) -> Self {
        Self {
            base: BaseFrame::new(type_name),
        }
    }
}

impl Frame for ControlFrame {
    fn id(&self) -> u64 {
        self.base.id()
    }
    fn name(&self) -> &str {
        self.base.name()
    }
    fn pts(&self) -> Option<u64> {
        self.base.pts()
    }
    fn set_pts(&mut self, pts: Option<u64>) {
        self.base.set_pts(pts)
    }
    fn metadata(&self) -> &HashMap<String, String> {
        self.base.metadata()
    }
    fn metadata_mut(&mut self) -> &mut HashMap<String, String> {
        self.base.metadata_mut()
    }
    fn transport_source(&self) -> Option<&str> {
        self.base.transport_source()
    }
    fn set_transport_source(&mut self, source: Option<String>) {
        self.base.set_transport_source(source)
    }
    fn transport_destination(&self) -> Option<&str> {
        self.base.transport_destination()
    }
    fn set_transport_destination(&mut self, destination: Option<String>) {
        self.base.set_transport_destination(destination)
    }
}

/// Control frame for audio mixer operations
#[derive(Debug, Clone)]
pub struct MixerControlFrame {
    pub control_frame: ControlFrame,
    pub enabled: Option<bool>,
    pub volume: Option<f32>,
    pub settings: HashMap<String, String>,
}

impl MixerControlFrame {
    pub fn new() -> Self {
        Self {
            control_frame: ControlFrame::new("MixerControlFrame"),
            enabled: None,
            volume: None,
            settings: HashMap::new(),
        }
    }

    pub fn enable() -> Self {
        Self {
            control_frame: ControlFrame::new("MixerControlFrame"),
            enabled: Some(true),
            volume: None,
            settings: HashMap::new(),
        }
    }

    pub fn disable() -> Self {
        Self {
            control_frame: ControlFrame::new("MixerControlFrame"),
            enabled: Some(false),
            volume: None,
            settings: HashMap::new(),
        }
    }

    pub fn set_volume(volume: f32) -> Self {
        Self {
            control_frame: ControlFrame::new("MixerControlFrame"),
            enabled: None,
            volume: Some(volume),
            settings: HashMap::new(),
        }
    }

    pub fn with_settings(settings: HashMap<String, String>) -> Self {
        Self {
            control_frame: ControlFrame::new("MixerControlFrame"),
            enabled: None,
            volume: None,
            settings,
        }
    }
}

impl Frame for MixerControlFrame {
    fn id(&self) -> u64 {
        self.control_frame.id()
    }
    fn name(&self) -> &str {
        self.control_frame.name()
    }
    fn pts(&self) -> Option<u64> {
        self.control_frame.pts()
    }
    fn set_pts(&mut self, pts: Option<u64>) {
        self.control_frame.set_pts(pts)
    }
    fn metadata(&self) -> &HashMap<String, String> {
        self.control_frame.metadata()
    }
    fn metadata_mut(&mut self) -> &mut HashMap<String, String> {
        self.control_frame.metadata_mut()
    }
    fn transport_source(&self) -> Option<&str> {
        self.control_frame.transport_source()
    }
    fn set_transport_source(&mut self, source: Option<String>) {
        self.control_frame.set_transport_source(source)
    }
    fn transport_destination(&self) -> Option<&str> {
        self.control_frame.transport_destination()
    }
    fn set_transport_destination(&mut self, destination: Option<String>) {
        self.control_frame.set_transport_destination(destination)
    }
}

/// Frame to enable or disable the mixer
#[derive(Debug, Clone)]
pub struct MixerEnableFrame {
    pub control_frame: ControlFrame,
    pub enable: bool,
}

impl MixerEnableFrame {
    pub fn new(enable: bool) -> Self {
        Self {
            control_frame: ControlFrame::new("MixerEnableFrame"),
            enable,
        }
    }
}

impl Frame for MixerEnableFrame {
    fn id(&self) -> u64 {
        self.control_frame.id()
    }
    fn name(&self) -> &str {
        self.control_frame.name()
    }
    fn pts(&self) -> Option<u64> {
        self.control_frame.pts()
    }
    fn set_pts(&mut self, pts: Option<u64>) {
        self.control_frame.set_pts(pts)
    }
    fn metadata(&self) -> &HashMap<String, String> {
        self.control_frame.metadata()
    }
    fn metadata_mut(&mut self) -> &mut HashMap<String, String> {
        self.control_frame.metadata_mut()
    }
    fn transport_source(&self) -> Option<&str> {
        self.control_frame.transport_source()
    }
    fn set_transport_source(&mut self, source: Option<String>) {
        self.control_frame.set_transport_source(source)
    }
    fn transport_destination(&self) -> Option<&str> {
        self.control_frame.transport_destination()
    }
    fn set_transport_destination(&mut self, destination: Option<String>) {
        self.control_frame.set_transport_destination(destination)
    }
}

/// Frame to update mixer settings
#[derive(Debug, Clone)]
pub struct MixerUpdateSettingsFrame {
    pub control_frame: ControlFrame,
    pub settings: HashMap<String, String>,
}

impl MixerUpdateSettingsFrame {
    pub fn new(settings: HashMap<String, String>) -> Self {
        Self {
            control_frame: ControlFrame::new("MixerUpdateSettingsFrame"),
            settings,
        }
    }
}

impl Frame for MixerUpdateSettingsFrame {
    fn id(&self) -> u64 {
        self.control_frame.id()
    }
    fn name(&self) -> &str {
        self.control_frame.name()
    }
    fn pts(&self) -> Option<u64> {
        self.control_frame.pts()
    }
    fn set_pts(&mut self, pts: Option<u64>) {
        self.control_frame.set_pts(pts)
    }
    fn metadata(&self) -> &HashMap<String, String> {
        self.control_frame.metadata()
    }
    fn metadata_mut(&mut self) -> &mut HashMap<String, String> {
        self.control_frame.metadata_mut()
    }
    fn transport_source(&self) -> Option<&str> {
        self.control_frame.transport_source()
    }
    fn set_transport_source(&mut self, source: Option<String>) {
        self.control_frame.set_transport_source(source)
    }
    fn transport_destination(&self) -> Option<&str> {
        self.control_frame.transport_destination()
    }
    fn set_transport_destination(&mut self, destination: Option<String>) {
        self.control_frame.set_transport_destination(destination)
    }
}

//
// Audio and Image Mixins
//

/// A frame containing a chunk of raw audio
#[derive(Debug, Clone)]
pub struct AudioRawFrame {
    pub audio: Vec<u8>,
    pub sample_rate: u32,
    pub num_channels: u16,
    pub num_frames: usize,
}

impl AudioRawFrame {
    pub fn new(audio: Vec<u8>, sample_rate: u32, num_channels: u16) -> Self {
        let num_frames = audio.len() / (num_channels as usize * 2); // 16-bit samples
        Self {
            audio,
            sample_rate,
            num_channels,
            num_frames,
        }
    }
}

/// A frame containing a raw image
#[derive(Debug, Clone)]
pub struct ImageRawFrame {
    pub image: Vec<u8>,
    pub size: (u32, u32), // (width, height)
    pub format: Option<String>,
}

impl ImageRawFrame {
    pub fn new(image: Vec<u8>, size: (u32, u32), format: Option<String>) -> Self {
        Self {
            image,
            size,
            format,
        }
    }
}

//
// Data Frames
//

/// Audio data frame for output to transport
#[derive(Debug, Clone)]
pub struct OutputAudioRawFrame {
    pub data_frame: DataFrame,
    pub audio_frame: AudioRawFrame,
}

impl OutputAudioRawFrame {
    pub fn new(audio: Vec<u8>, sample_rate: u32, num_channels: u16) -> Self {
        Self {
            data_frame: DataFrame::new("OutputAudioRawFrame"),
            audio_frame: AudioRawFrame::new(audio, sample_rate, num_channels),
        }
    }
}

impl Frame for OutputAudioRawFrame {
    fn id(&self) -> u64 {
        self.data_frame.id()
    }
    fn name(&self) -> &str {
        self.data_frame.name()
    }
    fn pts(&self) -> Option<u64> {
        self.data_frame.pts()
    }
    fn set_pts(&mut self, pts: Option<u64>) {
        self.data_frame.set_pts(pts)
    }
    fn metadata(&self) -> &HashMap<String, String> {
        self.data_frame.metadata()
    }
    fn metadata_mut(&mut self) -> &mut HashMap<String, String> {
        self.data_frame.metadata_mut()
    }
    fn transport_source(&self) -> Option<&str> {
        self.data_frame.transport_source()
    }
    fn set_transport_source(&mut self, source: Option<String>) {
        self.data_frame.set_transport_source(source)
    }
    fn transport_destination(&self) -> Option<&str> {
        self.data_frame.transport_destination()
    }
    fn set_transport_destination(&mut self, destination: Option<String>) {
        self.data_frame.set_transport_destination(destination)
    }
}

impl fmt::Display for OutputAudioRawFrame {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}(pts: {:?}, destination: {:?}, size: {}, frames: {}, sample_rate: {}, channels: {})",
            self.name(),
            self.pts(),
            self.transport_destination(),
            self.audio_frame.audio.len(),
            self.audio_frame.num_frames,
            self.audio_frame.sample_rate,
            self.audio_frame.num_channels
        )
    }
}

/// Image data frame for output to transport
#[derive(Debug, Clone)]
pub struct OutputImageRawFrame {
    pub data_frame: DataFrame,
    pub image_frame: ImageRawFrame,
}

impl OutputImageRawFrame {
    pub fn new(image: Vec<u8>, size: (u32, u32), format: Option<String>) -> Self {
        Self {
            data_frame: DataFrame::new("OutputImageRawFrame"),
            image_frame: ImageRawFrame::new(image, size, format),
        }
    }
}

impl Frame for OutputImageRawFrame {
    fn id(&self) -> u64 {
        self.data_frame.id()
    }
    fn name(&self) -> &str {
        self.data_frame.name()
    }
    fn pts(&self) -> Option<u64> {
        self.data_frame.pts()
    }
    fn set_pts(&mut self, pts: Option<u64>) {
        self.data_frame.set_pts(pts)
    }
    fn metadata(&self) -> &HashMap<String, String> {
        self.data_frame.metadata()
    }
    fn metadata_mut(&mut self) -> &mut HashMap<String, String> {
        self.data_frame.metadata_mut()
    }
    fn transport_source(&self) -> Option<&str> {
        self.data_frame.transport_source()
    }
    fn set_transport_source(&mut self, source: Option<String>) {
        self.data_frame.set_transport_source(source)
    }
    fn transport_destination(&self) -> Option<&str> {
        self.data_frame.transport_destination()
    }
    fn set_transport_destination(&mut self, destination: Option<String>) {
        self.data_frame.set_transport_destination(destination)
    }
}

impl fmt::Display for OutputImageRawFrame {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}(pts: {:?}, size: {:?}, format: {:?})",
            self.name(),
            self.pts(),
            self.image_frame.size,
            self.image_frame.format
        )
    }
}

/// Text data frame for passing text through the pipeline
#[derive(Debug, Clone)]
pub struct TextFrame {
    pub data_frame: DataFrame,
    pub text: String,
}

impl TextFrame {
    pub fn new(text: String) -> Self {
        Self {
            data_frame: DataFrame::new("TextFrame"),
            text,
        }
    }
}

impl Frame for TextFrame {
    fn id(&self) -> u64 {
        self.data_frame.id()
    }
    fn name(&self) -> &str {
        self.data_frame.name()
    }
    fn pts(&self) -> Option<u64> {
        self.data_frame.pts()
    }
    fn set_pts(&mut self, pts: Option<u64>) {
        self.data_frame.set_pts(pts)
    }
    fn metadata(&self) -> &HashMap<String, String> {
        self.data_frame.metadata()
    }
    fn metadata_mut(&mut self) -> &mut HashMap<String, String> {
        self.data_frame.metadata_mut()
    }
    fn transport_source(&self) -> Option<&str> {
        self.data_frame.transport_source()
    }
    fn set_transport_source(&mut self, source: Option<String>) {
        self.data_frame.set_transport_source(source)
    }
    fn transport_destination(&self) -> Option<&str> {
        self.data_frame.transport_destination()
    }
    fn set_transport_destination(&mut self, destination: Option<String>) {
        self.data_frame.set_transport_destination(destination)
    }
}

impl fmt::Display for TextFrame {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}(pts: {:?}, text: [{}])",
            self.name(),
            self.pts(),
            self.text
        )
    }
}

/// Text frame generated by LLM services
#[derive(Debug, Clone)]
pub struct LLMTextFrame {
    pub text_frame: TextFrame,
}

impl LLMTextFrame {
    pub fn new(text: String) -> Self {
        let mut text_frame = TextFrame::new(text);
        text_frame.data_frame.base.name = format!("LLMTextFrame#{}", text_frame.id());
        Self { text_frame }
    }
}

impl Frame for LLMTextFrame {
    fn id(&self) -> u64 {
        self.text_frame.id()
    }
    fn name(&self) -> &str {
        self.text_frame.name()
    }
    fn pts(&self) -> Option<u64> {
        self.text_frame.pts()
    }
    fn set_pts(&mut self, pts: Option<u64>) {
        self.text_frame.set_pts(pts)
    }
    fn metadata(&self) -> &HashMap<String, String> {
        self.text_frame.metadata()
    }
    fn metadata_mut(&mut self) -> &mut HashMap<String, String> {
        self.text_frame.metadata_mut()
    }
    fn transport_source(&self) -> Option<&str> {
        self.text_frame.transport_source()
    }
    fn set_transport_source(&mut self, source: Option<String>) {
        self.text_frame.set_transport_source(source)
    }
    fn transport_destination(&self) -> Option<&str> {
        self.text_frame.transport_destination()
    }
    fn set_transport_destination(&mut self, destination: Option<String>) {
        self.text_frame.set_transport_destination(destination)
    }
}

impl fmt::Display for LLMTextFrame {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.text_frame.fmt(f)
    }
}

//
// System Frames
//

/// Initial frame to start pipeline processing
#[derive(Debug, Clone)]
pub struct StartFrame {
    pub system_frame: SystemFrame,
    pub audio_in_sample_rate: u32,
    pub audio_out_sample_rate: u32,
    pub allow_interruptions: bool,
    pub enable_metrics: bool,
}

impl StartFrame {
    pub fn new() -> Self {
        Self {
            system_frame: SystemFrame::new("StartFrame"),
            audio_in_sample_rate: 16000,
            audio_out_sample_rate: 24000,
            allow_interruptions: false,
            enable_metrics: false,
        }
    }

    pub fn with_sample_rates(mut self, input: u32, output: u32) -> Self {
        self.audio_in_sample_rate = input;
        self.audio_out_sample_rate = output;
        self
    }

    pub fn with_interruptions(mut self, allow: bool) -> Self {
        self.allow_interruptions = allow;
        self
    }

    pub fn with_metrics(mut self, enable: bool) -> Self {
        self.enable_metrics = enable;
        self
    }
}

impl Frame for StartFrame {
    fn id(&self) -> u64 {
        self.system_frame.id()
    }
    fn name(&self) -> &str {
        self.system_frame.name()
    }
    fn pts(&self) -> Option<u64> {
        self.system_frame.pts()
    }
    fn set_pts(&mut self, pts: Option<u64>) {
        self.system_frame.set_pts(pts)
    }
    fn metadata(&self) -> &HashMap<String, String> {
        self.system_frame.metadata()
    }
    fn metadata_mut(&mut self) -> &mut HashMap<String, String> {
        self.system_frame.metadata_mut()
    }
    fn transport_source(&self) -> Option<&str> {
        self.system_frame.transport_source()
    }
    fn set_transport_source(&mut self, source: Option<String>) {
        self.system_frame.set_transport_source(source)
    }
    fn transport_destination(&self) -> Option<&str> {
        self.system_frame.transport_destination()
    }
    fn set_transport_destination(&mut self, destination: Option<String>) {
        self.system_frame.set_transport_destination(destination)
    }
}

/// Frame indicating pipeline should stop immediately
#[derive(Debug, Clone)]
pub struct CancelFrame {
    pub system_frame: SystemFrame,
}

impl CancelFrame {
    pub fn new() -> Self {
        Self {
            system_frame: SystemFrame::new("CancelFrame"),
        }
    }
}

impl Frame for CancelFrame {
    fn id(&self) -> u64 {
        self.system_frame.id()
    }
    fn name(&self) -> &str {
        self.system_frame.name()
    }
    fn pts(&self) -> Option<u64> {
        self.system_frame.pts()
    }
    fn set_pts(&mut self, pts: Option<u64>) {
        self.system_frame.set_pts(pts)
    }
    fn metadata(&self) -> &HashMap<String, String> {
        self.system_frame.metadata()
    }
    fn metadata_mut(&mut self) -> &mut HashMap<String, String> {
        self.system_frame.metadata_mut()
    }
    fn transport_source(&self) -> Option<&str> {
        self.system_frame.transport_source()
    }
    fn set_transport_source(&mut self, source: Option<String>) {
        self.system_frame.set_transport_source(source)
    }
    fn transport_destination(&self) -> Option<&str> {
        self.system_frame.transport_destination()
    }
    fn set_transport_destination(&mut self, destination: Option<String>) {
        self.system_frame.set_transport_destination(destination)
    }
}

/// Frame notifying of errors in the pipeline
#[derive(Debug, Clone)]
pub struct ErrorFrame {
    pub system_frame: SystemFrame,
    pub error: String,
    pub fatal: bool,
}

impl ErrorFrame {
    pub fn new(error: String) -> Self {
        Self {
            system_frame: SystemFrame::new("ErrorFrame"),
            error,
            fatal: false,
        }
    }

    pub fn fatal(error: String) -> Self {
        Self {
            system_frame: SystemFrame::new("ErrorFrame"),
            error,
            fatal: true,
        }
    }
}

impl Frame for ErrorFrame {
    fn id(&self) -> u64 {
        self.system_frame.id()
    }
    fn name(&self) -> &str {
        self.system_frame.name()
    }
    fn pts(&self) -> Option<u64> {
        self.system_frame.pts()
    }
    fn set_pts(&mut self, pts: Option<u64>) {
        self.system_frame.set_pts(pts)
    }
    fn metadata(&self) -> &HashMap<String, String> {
        self.system_frame.metadata()
    }
    fn metadata_mut(&mut self) -> &mut HashMap<String, String> {
        self.system_frame.metadata_mut()
    }
    fn transport_source(&self) -> Option<&str> {
        self.system_frame.transport_source()
    }
    fn set_transport_source(&mut self, source: Option<String>) {
        self.system_frame.set_transport_source(source)
    }
    fn transport_destination(&self) -> Option<&str> {
        self.system_frame.transport_destination()
    }
    fn set_transport_destination(&mut self, destination: Option<String>) {
        self.system_frame.set_transport_destination(destination)
    }
}

impl fmt::Display for ErrorFrame {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}(error: {}, fatal: {})",
            self.name(),
            self.error,
            self.fatal
        )
    }
}

/// Frame indicating user has started speaking
#[derive(Debug, Clone)]
pub struct UserStartedSpeakingFrame {
    pub system_frame: SystemFrame,
    pub emulated: bool,
}

impl UserStartedSpeakingFrame {
    pub fn new(emulated: bool) -> Self {
        Self {
            system_frame: SystemFrame::new("UserStartedSpeakingFrame"),
            emulated,
        }
    }
}

impl Frame for UserStartedSpeakingFrame {
    fn id(&self) -> u64 {
        self.system_frame.id()
    }
    fn name(&self) -> &str {
        self.system_frame.name()
    }
    fn pts(&self) -> Option<u64> {
        self.system_frame.pts()
    }
    fn set_pts(&mut self, pts: Option<u64>) {
        self.system_frame.set_pts(pts)
    }
    fn metadata(&self) -> &HashMap<String, String> {
        self.system_frame.metadata()
    }
    fn metadata_mut(&mut self) -> &mut HashMap<String, String> {
        self.system_frame.metadata_mut()
    }
    fn transport_source(&self) -> Option<&str> {
        self.system_frame.transport_source()
    }
    fn set_transport_source(&mut self, source: Option<String>) {
        self.system_frame.set_transport_source(source)
    }
    fn transport_destination(&self) -> Option<&str> {
        self.system_frame.transport_destination()
    }
    fn set_transport_destination(&mut self, destination: Option<String>) {
        self.system_frame.set_transport_destination(destination)
    }
}

/// Frame indicating user has stopped speaking
#[derive(Debug, Clone)]
pub struct UserStoppedSpeakingFrame {
    pub system_frame: SystemFrame,
    pub emulated: bool,
}

impl UserStoppedSpeakingFrame {
    pub fn new(emulated: bool) -> Self {
        Self {
            system_frame: SystemFrame::new("UserStoppedSpeakingFrame"),
            emulated,
        }
    }
}

impl Frame for UserStoppedSpeakingFrame {
    fn id(&self) -> u64 {
        self.system_frame.id()
    }
    fn name(&self) -> &str {
        self.system_frame.name()
    }
    fn pts(&self) -> Option<u64> {
        self.system_frame.pts()
    }
    fn set_pts(&mut self, pts: Option<u64>) {
        self.system_frame.set_pts(pts)
    }
    fn metadata(&self) -> &HashMap<String, String> {
        self.system_frame.metadata()
    }
    fn metadata_mut(&mut self) -> &mut HashMap<String, String> {
        self.system_frame.metadata_mut()
    }
    fn transport_source(&self) -> Option<&str> {
        self.system_frame.transport_source()
    }
    fn set_transport_source(&mut self, source: Option<String>) {
        self.system_frame.set_transport_source(source)
    }
    fn transport_destination(&self) -> Option<&str> {
        self.system_frame.transport_destination()
    }
    fn set_transport_destination(&mut self, destination: Option<String>) {
        self.system_frame.set_transport_destination(destination)
    }
}

/// Frame indicating start of frame processing interruption
#[derive(Debug, Clone)]
pub struct StartInterruptionFrame {
    pub system_frame: SystemFrame,
}

impl StartInterruptionFrame {
    pub fn new() -> Self {
        Self {
            system_frame: SystemFrame::new("StartInterruptionFrame"),
        }
    }
}

impl Frame for StartInterruptionFrame {
    fn id(&self) -> u64 {
        self.system_frame.id()
    }
    fn name(&self) -> &str {
        self.system_frame.name()
    }
    fn pts(&self) -> Option<u64> {
        self.system_frame.pts()
    }
    fn set_pts(&mut self, pts: Option<u64>) {
        self.system_frame.set_pts(pts)
    }
    fn metadata(&self) -> &HashMap<String, String> {
        self.system_frame.metadata()
    }
    fn metadata_mut(&mut self) -> &mut HashMap<String, String> {
        self.system_frame.metadata_mut()
    }
    fn transport_source(&self) -> Option<&str> {
        self.system_frame.transport_source()
    }
    fn set_transport_source(&mut self, source: Option<String>) {
        self.system_frame.set_transport_source(source)
    }
    fn transport_destination(&self) -> Option<&str> {
        self.system_frame.transport_destination()
    }
    fn set_transport_destination(&mut self, destination: Option<String>) {
        self.system_frame.set_transport_destination(destination)
    }
}

/// Frame indicating stop of frame processing interruption
#[derive(Debug, Clone)]
pub struct StopInterruptionFrame {
    pub system_frame: SystemFrame,
}

impl StopInterruptionFrame {
    pub fn new() -> Self {
        Self {
            system_frame: SystemFrame::new("StopInterruptionFrame"),
        }
    }
}

impl Frame for StopInterruptionFrame {
    fn id(&self) -> u64 {
        self.system_frame.id()
    }
    fn name(&self) -> &str {
        self.system_frame.name()
    }
    fn pts(&self) -> Option<u64> {
        self.system_frame.pts()
    }
    fn set_pts(&mut self, pts: Option<u64>) {
        self.system_frame.set_pts(pts)
    }
    fn metadata(&self) -> &HashMap<String, String> {
        self.system_frame.metadata()
    }
    fn metadata_mut(&mut self) -> &mut HashMap<String, String> {
        self.system_frame.metadata_mut()
    }
    fn transport_source(&self) -> Option<&str> {
        self.system_frame.transport_source()
    }
    fn set_transport_source(&mut self, source: Option<String>) {
        self.system_frame.set_transport_source(source)
    }
    fn transport_destination(&self) -> Option<&str> {
        self.system_frame.transport_destination()
    }
    fn set_transport_destination(&mut self, destination: Option<String>) {
        self.system_frame.set_transport_destination(destination)
    }
}

/// Frame indicating bot interruption
#[derive(Debug, Clone)]
pub struct BotInterruptionFrame {
    pub system_frame: SystemFrame,
}

impl BotInterruptionFrame {
    pub fn new() -> Self {
        Self {
            system_frame: SystemFrame::new("BotInterruptionFrame"),
        }
    }
}

impl Frame for BotInterruptionFrame {
    fn id(&self) -> u64 {
        self.system_frame.id()
    }
    fn name(&self) -> &str {
        self.system_frame.name()
    }
    fn pts(&self) -> Option<u64> {
        self.system_frame.pts()
    }
    fn set_pts(&mut self, pts: Option<u64>) {
        self.system_frame.set_pts(pts)
    }
    fn metadata(&self) -> &HashMap<String, String> {
        self.system_frame.metadata()
    }
    fn metadata_mut(&mut self) -> &mut HashMap<String, String> {
        self.system_frame.metadata_mut()
    }
    fn transport_source(&self) -> Option<&str> {
        self.system_frame.transport_source()
    }
    fn set_transport_source(&mut self, source: Option<String>) {
        self.system_frame.set_transport_source(source)
    }
    fn transport_destination(&self) -> Option<&str> {
        self.system_frame.transport_destination()
    }
    fn set_transport_destination(&mut self, destination: Option<String>) {
        self.system_frame.set_transport_destination(destination)
    }
}

/// Frame indicating bot started speaking
#[derive(Debug, Clone)]
pub struct BotStartedSpeakingFrame {
    pub system_frame: SystemFrame,
}

impl BotStartedSpeakingFrame {
    pub fn new() -> Self {
        Self {
            system_frame: SystemFrame::new("BotStartedSpeakingFrame"),
        }
    }
}

impl Frame for BotStartedSpeakingFrame {
    fn id(&self) -> u64 {
        self.system_frame.id()
    }
    fn name(&self) -> &str {
        self.system_frame.name()
    }
    fn pts(&self) -> Option<u64> {
        self.system_frame.pts()
    }
    fn set_pts(&mut self, pts: Option<u64>) {
        self.system_frame.set_pts(pts)
    }
    fn metadata(&self) -> &HashMap<String, String> {
        self.system_frame.metadata()
    }
    fn metadata_mut(&mut self) -> &mut HashMap<String, String> {
        self.system_frame.metadata_mut()
    }
    fn transport_source(&self) -> Option<&str> {
        self.system_frame.transport_source()
    }
    fn set_transport_source(&mut self, source: Option<String>) {
        self.system_frame.set_transport_source(source)
    }
    fn transport_destination(&self) -> Option<&str> {
        self.system_frame.transport_destination()
    }
    fn set_transport_destination(&mut self, destination: Option<String>) {
        self.system_frame.set_transport_destination(destination)
    }
}

/// Frame indicating bot stopped speaking
#[derive(Debug, Clone)]
pub struct BotStoppedSpeakingFrame {
    pub system_frame: SystemFrame,
}

impl BotStoppedSpeakingFrame {
    pub fn new() -> Self {
        Self {
            system_frame: SystemFrame::new("BotStoppedSpeakingFrame"),
        }
    }
}

impl Frame for BotStoppedSpeakingFrame {
    fn id(&self) -> u64 {
        self.system_frame.id()
    }
    fn name(&self) -> &str {
        self.system_frame.name()
    }
    fn pts(&self) -> Option<u64> {
        self.system_frame.pts()
    }
    fn set_pts(&mut self, pts: Option<u64>) {
        self.system_frame.set_pts(pts)
    }
    fn metadata(&self) -> &HashMap<String, String> {
        self.system_frame.metadata()
    }
    fn metadata_mut(&mut self) -> &mut HashMap<String, String> {
        self.system_frame.metadata_mut()
    }
    fn transport_source(&self) -> Option<&str> {
        self.system_frame.transport_source()
    }
    fn set_transport_source(&mut self, source: Option<String>) {
        self.system_frame.set_transport_source(source)
    }
    fn transport_destination(&self) -> Option<&str> {
        self.system_frame.transport_destination()
    }
    fn set_transport_destination(&mut self, destination: Option<String>) {
        self.system_frame.set_transport_destination(destination)
    }
}

/// Frame for emulating user started speaking
#[derive(Debug, Clone)]
pub struct EmulateUserStartedSpeakingFrame {
    pub system_frame: SystemFrame,
}

impl EmulateUserStartedSpeakingFrame {
    pub fn new() -> Self {
        Self {
            system_frame: SystemFrame::new("EmulateUserStartedSpeakingFrame"),
        }
    }
}

impl Frame for EmulateUserStartedSpeakingFrame {
    fn id(&self) -> u64 {
        self.system_frame.id()
    }
    fn name(&self) -> &str {
        self.system_frame.name()
    }
    fn pts(&self) -> Option<u64> {
        self.system_frame.pts()
    }
    fn set_pts(&mut self, pts: Option<u64>) {
        self.system_frame.set_pts(pts)
    }
    fn metadata(&self) -> &HashMap<String, String> {
        self.system_frame.metadata()
    }
    fn metadata_mut(&mut self) -> &mut HashMap<String, String> {
        self.system_frame.metadata_mut()
    }
    fn transport_source(&self) -> Option<&str> {
        self.system_frame.transport_source()
    }
    fn set_transport_source(&mut self, source: Option<String>) {
        self.system_frame.set_transport_source(source)
    }
    fn transport_destination(&self) -> Option<&str> {
        self.system_frame.transport_destination()
    }
    fn set_transport_destination(&mut self, destination: Option<String>) {
        self.system_frame.set_transport_destination(destination)
    }
}

/// Frame for emulating user stopped speaking
#[derive(Debug, Clone)]
pub struct EmulateUserStoppedSpeakingFrame {
    pub system_frame: SystemFrame,
}

impl EmulateUserStoppedSpeakingFrame {
    pub fn new() -> Self {
        Self {
            system_frame: SystemFrame::new("EmulateUserStoppedSpeakingFrame"),
        }
    }
}

impl Frame for EmulateUserStoppedSpeakingFrame {
    fn id(&self) -> u64 {
        self.system_frame.id()
    }
    fn name(&self) -> &str {
        self.system_frame.name()
    }
    fn pts(&self) -> Option<u64> {
        self.system_frame.pts()
    }
    fn set_pts(&mut self, pts: Option<u64>) {
        self.system_frame.set_pts(pts)
    }
    fn metadata(&self) -> &HashMap<String, String> {
        self.system_frame.metadata()
    }
    fn metadata_mut(&mut self) -> &mut HashMap<String, String> {
        self.system_frame.metadata_mut()
    }
    fn transport_source(&self) -> Option<&str> {
        self.system_frame.transport_source()
    }
    fn set_transport_source(&mut self, source: Option<String>) {
        self.system_frame.set_transport_source(source)
    }
    fn transport_destination(&self) -> Option<&str> {
        self.system_frame.transport_destination()
    }
    fn set_transport_destination(&mut self, destination: Option<String>) {
        self.system_frame.set_transport_destination(destination)
    }
}

/// Frame indicating transport should stop temporarily
#[derive(Debug, Clone)]
pub struct StopFrame {
    pub control_frame: ControlFrame,
}

impl StopFrame {
    pub fn new() -> Self {
        Self {
            control_frame: ControlFrame::new("StopFrame"),
        }
    }
}

impl Frame for StopFrame {
    fn id(&self) -> u64 {
        self.control_frame.id()
    }
    fn name(&self) -> &str {
        self.control_frame.name()
    }
    fn pts(&self) -> Option<u64> {
        self.control_frame.pts()
    }
    fn set_pts(&mut self, pts: Option<u64>) {
        self.control_frame.set_pts(pts)
    }
    fn metadata(&self) -> &HashMap<String, String> {
        self.control_frame.metadata()
    }
    fn metadata_mut(&mut self) -> &mut HashMap<String, String> {
        self.control_frame.metadata_mut()
    }
    fn transport_source(&self) -> Option<&str> {
        self.control_frame.transport_source()
    }
    fn set_transport_source(&mut self, source: Option<String>) {
        self.control_frame.set_transport_source(source)
    }
    fn transport_destination(&self) -> Option<&str> {
        self.control_frame.transport_destination()
    }
    fn set_transport_destination(&mut self, destination: Option<String>) {
        self.control_frame.set_transport_destination(destination)
    }
}

/// Frame containing input audio data
#[derive(Debug, Clone)]
pub struct InputAudioRawFrame {
    pub data_frame: DataFrame,
    pub audio: Vec<u8>,
    pub sample_rate: u32,
    pub num_channels: u32,
}

impl InputAudioRawFrame {
    pub fn new(audio: Vec<u8>, sample_rate: u32, num_channels: u32) -> Self {
        Self {
            data_frame: DataFrame::new("InputAudioRawFrame"),
            audio,
            sample_rate,
            num_channels,
        }
    }
}

impl Frame for InputAudioRawFrame {
    fn id(&self) -> u64 {
        self.data_frame.id()
    }
    fn name(&self) -> &str {
        self.data_frame.name()
    }
    fn pts(&self) -> Option<u64> {
        self.data_frame.pts()
    }
    fn set_pts(&mut self, pts: Option<u64>) {
        self.data_frame.set_pts(pts)
    }
    fn metadata(&self) -> &HashMap<String, String> {
        self.data_frame.metadata()
    }
    fn metadata_mut(&mut self) -> &mut HashMap<String, String> {
        self.data_frame.metadata_mut()
    }
    fn transport_source(&self) -> Option<&str> {
        self.data_frame.transport_source()
    }
    fn set_transport_source(&mut self, source: Option<String>) {
        self.data_frame.set_transport_source(source)
    }
    fn transport_destination(&self) -> Option<&str> {
        self.data_frame.transport_destination()
    }
    fn set_transport_destination(&mut self, destination: Option<String>) {
        self.data_frame.set_transport_destination(destination)
    }
}

/// Frame containing input image data
#[derive(Debug, Clone)]
pub struct InputImageRawFrame {
    pub data_frame: DataFrame,
    pub image: Vec<u8>,
    pub size: (u32, u32),
    pub format: String,
}

impl InputImageRawFrame {
    pub fn new(image: Vec<u8>, size: (u32, u32), format: String) -> Self {
        Self {
            data_frame: DataFrame::new("InputImageRawFrame"),
            image,
            size,
            format,
        }
    }
}

impl Frame for InputImageRawFrame {
    fn id(&self) -> u64 {
        self.data_frame.id()
    }
    fn name(&self) -> &str {
        self.data_frame.name()
    }
    fn pts(&self) -> Option<u64> {
        self.data_frame.pts()
    }
    fn set_pts(&mut self, pts: Option<u64>) {
        self.data_frame.set_pts(pts)
    }
    fn metadata(&self) -> &HashMap<String, String> {
        self.data_frame.metadata()
    }
    fn metadata_mut(&mut self) -> &mut HashMap<String, String> {
        self.data_frame.metadata_mut()
    }
    fn transport_source(&self) -> Option<&str> {
        self.data_frame.transport_source()
    }
    fn set_transport_source(&mut self, source: Option<String>) {
        self.data_frame.set_transport_source(source)
    }
    fn transport_destination(&self) -> Option<&str> {
        self.data_frame.transport_destination()
    }
    fn set_transport_destination(&mut self, destination: Option<String>) {
        self.data_frame.set_transport_destination(destination)
    }
}

/// Frame indicating pipeline has ended and should shut down
#[derive(Debug, Clone)]
pub struct EndFrame {
    pub control_frame: ControlFrame,
}

impl EndFrame {
    pub fn new() -> Self {
        Self {
            control_frame: ControlFrame::new("EndFrame"),
        }
    }
}

impl Frame for EndFrame {
    fn id(&self) -> u64 {
        self.control_frame.id()
    }
    fn name(&self) -> &str {
        self.control_frame.name()
    }
    fn pts(&self) -> Option<u64> {
        self.control_frame.pts()
    }
    fn set_pts(&mut self, pts: Option<u64>) {
        self.control_frame.set_pts(pts)
    }
    fn metadata(&self) -> &HashMap<String, String> {
        self.control_frame.metadata()
    }
    fn metadata_mut(&mut self) -> &mut HashMap<String, String> {
        self.control_frame.metadata_mut()
    }
    fn transport_source(&self) -> Option<&str> {
        self.control_frame.transport_source()
    }
    fn set_transport_source(&mut self, source: Option<String>) {
        self.control_frame.set_transport_source(source)
    }
    fn transport_destination(&self) -> Option<&str> {
        self.control_frame.transport_destination()
    }
    fn set_transport_destination(&mut self, destination: Option<String>) {
        self.control_frame.set_transport_destination(destination)
    }
}

/// Frame indicating the beginning of an LLM response
#[derive(Debug, Clone)]
pub struct LLMFullResponseStartFrame {
    pub control_frame: ControlFrame,
}

impl LLMFullResponseStartFrame {
    pub fn new() -> Self {
        Self {
            control_frame: ControlFrame::new("LLMFullResponseStartFrame"),
        }
    }
}

impl Frame for LLMFullResponseStartFrame {
    fn id(&self) -> u64 {
        self.control_frame.id()
    }
    fn name(&self) -> &str {
        self.control_frame.name()
    }
    fn pts(&self) -> Option<u64> {
        self.control_frame.pts()
    }
    fn set_pts(&mut self, pts: Option<u64>) {
        self.control_frame.set_pts(pts)
    }
    fn metadata(&self) -> &HashMap<String, String> {
        self.control_frame.metadata()
    }
    fn metadata_mut(&mut self) -> &mut HashMap<String, String> {
        self.control_frame.metadata_mut()
    }
    fn transport_source(&self) -> Option<&str> {
        self.control_frame.transport_source()
    }
    fn set_transport_source(&mut self, source: Option<String>) {
        self.control_frame.set_transport_source(source)
    }
    fn transport_destination(&self) -> Option<&str> {
        self.control_frame.transport_destination()
    }
    fn set_transport_destination(&mut self, destination: Option<String>) {
        self.control_frame.set_transport_destination(destination)
    }
}

/// Frame indicating the end of an LLM response
#[derive(Debug, Clone)]
pub struct LLMFullResponseEndFrame {
    pub control_frame: ControlFrame,
}

impl LLMFullResponseEndFrame {
    pub fn new() -> Self {
        Self {
            control_frame: ControlFrame::new("LLMFullResponseEndFrame"),
        }
    }
}

impl Frame for LLMFullResponseEndFrame {
    fn id(&self) -> u64 {
        self.control_frame.id()
    }
    fn name(&self) -> &str {
        self.control_frame.name()
    }
    fn pts(&self) -> Option<u64> {
        self.control_frame.pts()
    }
    fn set_pts(&mut self, pts: Option<u64>) {
        self.control_frame.set_pts(pts)
    }
    fn metadata(&self) -> &HashMap<String, String> {
        self.control_frame.metadata()
    }
    fn metadata_mut(&mut self) -> &mut HashMap<String, String> {
        self.control_frame.metadata_mut()
    }
    fn transport_source(&self) -> Option<&str> {
        self.control_frame.transport_source()
    }
    fn set_transport_source(&mut self, source: Option<String>) {
        self.control_frame.set_transport_source(source)
    }
    fn transport_destination(&self) -> Option<&str> {
        self.control_frame.transport_destination()
    }
    fn set_transport_destination(&mut self, destination: Option<String>) {
        self.control_frame.set_transport_destination(destination)
    }
}

//
// Enum for all frame types
//

/// Enum containing all possible frame types for type-safe frame handling
#[derive(Debug, Clone)]
pub enum FrameType {
    // Data frames
    OutputAudioRaw(OutputAudioRawFrame),
    OutputImageRaw(OutputImageRawFrame),
    InputAudioRaw(InputAudioRawFrame),
    InputImageRaw(InputImageRawFrame),
    Text(TextFrame),
    LLMText(LLMTextFrame),

    // System frames
    Start(StartFrame),
    Cancel(CancelFrame),
    Error(ErrorFrame),
    StartInterruption(StartInterruptionFrame),
    StopInterruption(StopInterruptionFrame),
    UserStartedSpeaking(UserStartedSpeakingFrame),
    UserStoppedSpeaking(UserStoppedSpeakingFrame),
    BotInterruption(BotInterruptionFrame),
    BotStartedSpeaking(BotStartedSpeakingFrame),
    BotStoppedSpeaking(BotStoppedSpeakingFrame),
    EmulateUserStartedSpeaking(EmulateUserStartedSpeakingFrame),
    EmulateUserStoppedSpeaking(EmulateUserStoppedSpeakingFrame),

    // Control frames
    End(EndFrame),
    Stop(StopFrame),
    LLMFullResponseStart(LLMFullResponseStartFrame),
    LLMFullResponseEnd(LLMFullResponseEndFrame),
}

impl Frame for FrameType {
    fn id(&self) -> u64 {
        match self {
            FrameType::OutputAudioRaw(f) => f.id(),
            FrameType::OutputImageRaw(f) => f.id(),
            FrameType::InputAudioRaw(f) => f.id(),
            FrameType::InputImageRaw(f) => f.id(),
            FrameType::Text(f) => f.id(),
            FrameType::LLMText(f) => f.id(),
            FrameType::Start(f) => f.id(),
            FrameType::Cancel(f) => f.id(),
            FrameType::Error(f) => f.id(),
            FrameType::UserStartedSpeaking(f) => f.id(),
            FrameType::UserStoppedSpeaking(f) => f.id(),
            FrameType::StartInterruption(f) => f.id(),
            FrameType::StopInterruption(f) => f.id(),
            FrameType::BotInterruption(f) => f.id(),
            FrameType::BotStartedSpeaking(f) => f.id(),
            FrameType::BotStoppedSpeaking(f) => f.id(),
            FrameType::EmulateUserStartedSpeaking(f) => f.id(),
            FrameType::EmulateUserStoppedSpeaking(f) => f.id(),
            FrameType::End(f) => f.id(),
            FrameType::Stop(f) => f.id(),
            FrameType::LLMFullResponseStart(f) => f.id(),
            FrameType::LLMFullResponseEnd(f) => f.id(),
        }
    }

    fn name(&self) -> &str {
        match self {
            FrameType::OutputAudioRaw(f) => f.name(),
            FrameType::OutputImageRaw(f) => f.name(),
            FrameType::InputAudioRaw(f) => f.name(),
            FrameType::InputImageRaw(f) => f.name(),
            FrameType::Text(f) => f.name(),
            FrameType::LLMText(f) => f.name(),
            FrameType::Start(f) => f.name(),
            FrameType::Cancel(f) => f.name(),
            FrameType::Error(f) => f.name(),
            FrameType::UserStartedSpeaking(f) => f.name(),
            FrameType::UserStoppedSpeaking(f) => f.name(),
            FrameType::StartInterruption(f) => f.name(),
            FrameType::StopInterruption(f) => f.name(),
            FrameType::BotInterruption(f) => f.name(),
            FrameType::BotStartedSpeaking(f) => f.name(),
            FrameType::BotStoppedSpeaking(f) => f.name(),
            FrameType::EmulateUserStartedSpeaking(f) => f.name(),
            FrameType::EmulateUserStoppedSpeaking(f) => f.name(),
            FrameType::End(f) => f.name(),
            FrameType::Stop(f) => f.name(),
            FrameType::LLMFullResponseStart(f) => f.name(),
            FrameType::LLMFullResponseEnd(f) => f.name(),
        }
    }

    fn pts(&self) -> Option<u64> {
        match self {
            FrameType::OutputAudioRaw(f) => f.pts(),
            FrameType::OutputImageRaw(f) => f.pts(),
            FrameType::InputAudioRaw(f) => f.pts(),
            FrameType::InputImageRaw(f) => f.pts(),
            FrameType::Text(f) => f.pts(),
            FrameType::LLMText(f) => f.pts(),
            FrameType::Start(f) => f.pts(),
            FrameType::Cancel(f) => f.pts(),
            FrameType::Error(f) => f.pts(),
            FrameType::UserStartedSpeaking(f) => f.pts(),
            FrameType::UserStoppedSpeaking(f) => f.pts(),
            FrameType::StartInterruption(f) => f.pts(),
            FrameType::StopInterruption(f) => f.pts(),
            FrameType::BotInterruption(f) => f.pts(),
            FrameType::BotStartedSpeaking(f) => f.pts(),
            FrameType::BotStoppedSpeaking(f) => f.pts(),
            FrameType::EmulateUserStartedSpeaking(f) => f.pts(),
            FrameType::EmulateUserStoppedSpeaking(f) => f.pts(),
            FrameType::End(f) => f.pts(),
            FrameType::Stop(f) => f.pts(),
            FrameType::LLMFullResponseStart(f) => f.pts(),
            FrameType::LLMFullResponseEnd(f) => f.pts(),
        }
    }

    fn set_pts(&mut self, pts: Option<u64>) {
        match self {
            FrameType::OutputAudioRaw(f) => f.set_pts(pts),
            FrameType::OutputImageRaw(f) => f.set_pts(pts),
            FrameType::InputAudioRaw(f) => f.set_pts(pts),
            FrameType::InputImageRaw(f) => f.set_pts(pts),
            FrameType::Text(f) => f.set_pts(pts),
            FrameType::LLMText(f) => f.set_pts(pts),
            FrameType::Start(f) => f.set_pts(pts),
            FrameType::Cancel(f) => f.set_pts(pts),
            FrameType::Error(f) => f.set_pts(pts),
            FrameType::UserStartedSpeaking(f) => f.set_pts(pts),
            FrameType::UserStoppedSpeaking(f) => f.set_pts(pts),
            FrameType::StartInterruption(f) => f.set_pts(pts),
            FrameType::StopInterruption(f) => f.set_pts(pts),
            FrameType::BotInterruption(f) => f.set_pts(pts),
            FrameType::BotStartedSpeaking(f) => f.set_pts(pts),
            FrameType::BotStoppedSpeaking(f) => f.set_pts(pts),
            FrameType::EmulateUserStartedSpeaking(f) => f.set_pts(pts),
            FrameType::EmulateUserStoppedSpeaking(f) => f.set_pts(pts),
            FrameType::End(f) => f.set_pts(pts),
            FrameType::Stop(f) => f.set_pts(pts),
            FrameType::LLMFullResponseStart(f) => f.set_pts(pts),
            FrameType::LLMFullResponseEnd(f) => f.set_pts(pts),
        }
    }

    fn metadata(&self) -> &HashMap<String, String> {
        match self {
            FrameType::OutputAudioRaw(f) => f.metadata(),
            FrameType::OutputImageRaw(f) => f.metadata(),
            FrameType::InputAudioRaw(f) => f.metadata(),
            FrameType::InputImageRaw(f) => f.metadata(),
            FrameType::Text(f) => f.metadata(),
            FrameType::LLMText(f) => f.metadata(),
            FrameType::Start(f) => f.metadata(),
            FrameType::Cancel(f) => f.metadata(),
            FrameType::Error(f) => f.metadata(),
            FrameType::UserStartedSpeaking(f) => f.metadata(),
            FrameType::UserStoppedSpeaking(f) => f.metadata(),
            FrameType::StartInterruption(f) => f.metadata(),
            FrameType::StopInterruption(f) => f.metadata(),
            FrameType::BotInterruption(f) => f.metadata(),
            FrameType::BotStartedSpeaking(f) => f.metadata(),
            FrameType::BotStoppedSpeaking(f) => f.metadata(),
            FrameType::EmulateUserStartedSpeaking(f) => f.metadata(),
            FrameType::EmulateUserStoppedSpeaking(f) => f.metadata(),
            FrameType::End(f) => f.metadata(),
            FrameType::Stop(f) => f.metadata(),
            FrameType::LLMFullResponseStart(f) => f.metadata(),
            FrameType::LLMFullResponseEnd(f) => f.metadata(),
        }
    }

    fn metadata_mut(&mut self) -> &mut HashMap<String, String> {
        match self {
            FrameType::OutputAudioRaw(f) => f.metadata_mut(),
            FrameType::OutputImageRaw(f) => f.metadata_mut(),
            FrameType::InputAudioRaw(f) => f.metadata_mut(),
            FrameType::InputImageRaw(f) => f.metadata_mut(),
            FrameType::Text(f) => f.metadata_mut(),
            FrameType::LLMText(f) => f.metadata_mut(),
            FrameType::Start(f) => f.metadata_mut(),
            FrameType::Cancel(f) => f.metadata_mut(),
            FrameType::Error(f) => f.metadata_mut(),
            FrameType::UserStartedSpeaking(f) => f.metadata_mut(),
            FrameType::UserStoppedSpeaking(f) => f.metadata_mut(),
            FrameType::StartInterruption(f) => f.metadata_mut(),
            FrameType::StopInterruption(f) => f.metadata_mut(),
            FrameType::BotInterruption(f) => f.metadata_mut(),
            FrameType::BotStartedSpeaking(f) => f.metadata_mut(),
            FrameType::BotStoppedSpeaking(f) => f.metadata_mut(),
            FrameType::EmulateUserStartedSpeaking(f) => f.metadata_mut(),
            FrameType::EmulateUserStoppedSpeaking(f) => f.metadata_mut(),
            FrameType::End(f) => f.metadata_mut(),
            FrameType::Stop(f) => f.metadata_mut(),
            FrameType::LLMFullResponseStart(f) => f.metadata_mut(),
            FrameType::LLMFullResponseEnd(f) => f.metadata_mut(),
        }
    }

    fn transport_source(&self) -> Option<&str> {
        match self {
            FrameType::OutputAudioRaw(f) => f.transport_source(),
            FrameType::OutputImageRaw(f) => f.transport_source(),
            FrameType::InputAudioRaw(f) => f.transport_source(),
            FrameType::InputImageRaw(f) => f.transport_source(),
            FrameType::Text(f) => f.transport_source(),
            FrameType::LLMText(f) => f.transport_source(),
            FrameType::Start(f) => f.transport_source(),
            FrameType::Cancel(f) => f.transport_source(),
            FrameType::Error(f) => f.transport_source(),
            FrameType::UserStartedSpeaking(f) => f.transport_source(),
            FrameType::UserStoppedSpeaking(f) => f.transport_source(),
            FrameType::StartInterruption(f) => f.transport_source(),
            FrameType::StopInterruption(f) => f.transport_source(),
            FrameType::BotInterruption(f) => f.transport_source(),
            FrameType::BotStartedSpeaking(f) => f.transport_source(),
            FrameType::BotStoppedSpeaking(f) => f.transport_source(),
            FrameType::EmulateUserStartedSpeaking(f) => f.transport_source(),
            FrameType::EmulateUserStoppedSpeaking(f) => f.transport_source(),
            FrameType::End(f) => f.transport_source(),
            FrameType::Stop(f) => f.transport_source(),
            FrameType::LLMFullResponseStart(f) => f.transport_source(),
            FrameType::LLMFullResponseEnd(f) => f.transport_source(),
        }
    }

    fn set_transport_source(&mut self, source: Option<String>) {
        match self {
            FrameType::OutputAudioRaw(f) => f.set_transport_source(source),
            FrameType::OutputImageRaw(f) => f.set_transport_source(source),
            FrameType::InputAudioRaw(f) => f.set_transport_source(source),
            FrameType::InputImageRaw(f) => f.set_transport_source(source),
            FrameType::Text(f) => f.set_transport_source(source),
            FrameType::LLMText(f) => f.set_transport_source(source),
            FrameType::Start(f) => f.set_transport_source(source),
            FrameType::Cancel(f) => f.set_transport_source(source),
            FrameType::Error(f) => f.set_transport_source(source),
            FrameType::UserStartedSpeaking(f) => f.set_transport_source(source),
            FrameType::UserStoppedSpeaking(f) => f.set_transport_source(source),
            FrameType::StartInterruption(f) => f.set_transport_source(source),
            FrameType::StopInterruption(f) => f.set_transport_source(source),
            FrameType::BotInterruption(f) => f.set_transport_source(source),
            FrameType::BotStartedSpeaking(f) => f.set_transport_source(source),
            FrameType::BotStoppedSpeaking(f) => f.set_transport_source(source),
            FrameType::EmulateUserStartedSpeaking(f) => f.set_transport_source(source),
            FrameType::EmulateUserStoppedSpeaking(f) => f.set_transport_source(source),
            FrameType::End(f) => f.set_transport_source(source),
            FrameType::Stop(f) => f.set_transport_source(source),
            FrameType::LLMFullResponseStart(f) => f.set_transport_source(source),
            FrameType::LLMFullResponseEnd(f) => f.set_transport_source(source),
        }
    }

    fn transport_destination(&self) -> Option<&str> {
        match self {
            FrameType::OutputAudioRaw(f) => f.transport_destination(),
            FrameType::OutputImageRaw(f) => f.transport_destination(),
            FrameType::InputAudioRaw(f) => f.transport_destination(),
            FrameType::InputImageRaw(f) => f.transport_destination(),
            FrameType::Text(f) => f.transport_destination(),
            FrameType::LLMText(f) => f.transport_destination(),
            FrameType::Start(f) => f.transport_destination(),
            FrameType::Cancel(f) => f.transport_destination(),
            FrameType::Error(f) => f.transport_destination(),
            FrameType::UserStartedSpeaking(f) => f.transport_destination(),
            FrameType::UserStoppedSpeaking(f) => f.transport_destination(),
            FrameType::StartInterruption(f) => f.transport_destination(),
            FrameType::StopInterruption(f) => f.transport_destination(),
            FrameType::BotInterruption(f) => f.transport_destination(),
            FrameType::BotStartedSpeaking(f) => f.transport_destination(),
            FrameType::BotStoppedSpeaking(f) => f.transport_destination(),
            FrameType::EmulateUserStartedSpeaking(f) => f.transport_destination(),
            FrameType::EmulateUserStoppedSpeaking(f) => f.transport_destination(),
            FrameType::End(f) => f.transport_destination(),
            FrameType::Stop(f) => f.transport_destination(),
            FrameType::LLMFullResponseStart(f) => f.transport_destination(),
            FrameType::LLMFullResponseEnd(f) => f.transport_destination(),
        }
    }

    fn set_transport_destination(&mut self, destination: Option<String>) {
        match self {
            FrameType::OutputAudioRaw(f) => f.set_transport_destination(destination),
            FrameType::OutputImageRaw(f) => f.set_transport_destination(destination),
            FrameType::InputAudioRaw(f) => f.set_transport_destination(destination),
            FrameType::InputImageRaw(f) => f.set_transport_destination(destination),
            FrameType::Text(f) => f.set_transport_destination(destination),
            FrameType::LLMText(f) => f.set_transport_destination(destination),
            FrameType::Start(f) => f.set_transport_destination(destination),
            FrameType::Cancel(f) => f.set_transport_destination(destination),
            FrameType::Error(f) => f.set_transport_destination(destination),
            FrameType::UserStartedSpeaking(f) => f.set_transport_destination(destination),
            FrameType::UserStoppedSpeaking(f) => f.set_transport_destination(destination),
            FrameType::StartInterruption(f) => f.set_transport_destination(destination),
            FrameType::StopInterruption(f) => f.set_transport_destination(destination),
            FrameType::BotInterruption(f) => f.set_transport_destination(destination),
            FrameType::BotStartedSpeaking(f) => f.set_transport_destination(destination),
            FrameType::BotStoppedSpeaking(f) => f.set_transport_destination(destination),
            FrameType::EmulateUserStartedSpeaking(f) => f.set_transport_destination(destination),
            FrameType::EmulateUserStoppedSpeaking(f) => f.set_transport_destination(destination),
            FrameType::End(f) => f.set_transport_destination(destination),
            FrameType::Stop(f) => f.set_transport_destination(destination),
            FrameType::LLMFullResponseStart(f) => f.set_transport_destination(destination),
            FrameType::LLMFullResponseEnd(f) => f.set_transport_destination(destination),
        }
    }
}

impl fmt::Display for FrameType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FrameType::OutputAudioRaw(frame) => frame.fmt(f),
            FrameType::OutputImageRaw(frame) => frame.fmt(f),
            FrameType::InputAudioRaw(frame) => write!(f, "{}", frame.name()),
            FrameType::InputImageRaw(frame) => write!(f, "{}", frame.name()),
            FrameType::Text(frame) => frame.fmt(f),
            FrameType::LLMText(frame) => frame.fmt(f),
            FrameType::Start(frame) => write!(f, "{}", frame.name()),
            FrameType::Cancel(frame) => write!(f, "{}", frame.name()),
            FrameType::Error(frame) => frame.fmt(f),
            FrameType::UserStartedSpeaking(frame) => write!(f, "{}", frame.name()),
            FrameType::UserStoppedSpeaking(frame) => write!(f, "{}", frame.name()),
            FrameType::StartInterruption(frame) => write!(f, "{}", frame.name()),
            FrameType::StopInterruption(frame) => write!(f, "{}", frame.name()),
            FrameType::BotInterruption(frame) => write!(f, "{}", frame.name()),
            FrameType::BotStartedSpeaking(frame) => write!(f, "{}", frame.name()),
            FrameType::BotStoppedSpeaking(frame) => write!(f, "{}", frame.name()),
            FrameType::EmulateUserStartedSpeaking(frame) => write!(f, "{}", frame.name()),
            FrameType::EmulateUserStoppedSpeaking(frame) => write!(f, "{}", frame.name()),
            FrameType::End(frame) => write!(f, "{}", frame.name()),
            FrameType::Stop(frame) => write!(f, "{}", frame.name()),
            FrameType::LLMFullResponseStart(frame) => write!(f, "{}", frame.name()),
            FrameType::LLMFullResponseEnd(frame) => write!(f, "{}", frame.name()),
        }
    }
}

impl FrameType {
    /// Check if this is a system frame
    pub fn is_system_frame(&self) -> bool {
        matches!(
            self,
            FrameType::Start(_)
                | FrameType::Cancel(_)
                | FrameType::Error(_)
                | FrameType::UserStartedSpeaking(_)
                | FrameType::UserStoppedSpeaking(_)
                | FrameType::StartInterruption(_)
                | FrameType::StopInterruption(_)
                | FrameType::BotInterruption(_)
                | FrameType::BotStartedSpeaking(_)
                | FrameType::BotStoppedSpeaking(_)
                | FrameType::EmulateUserStartedSpeaking(_)
                | FrameType::EmulateUserStoppedSpeaking(_)
        )
    }

    /// Check if this is a data frame
    pub fn is_data_frame(&self) -> bool {
        matches!(
            self,
            FrameType::OutputAudioRaw(_)
                | FrameType::OutputImageRaw(_)
                | FrameType::InputAudioRaw(_)
                | FrameType::InputImageRaw(_)
                | FrameType::Text(_)
                | FrameType::LLMText(_)
        )
    }

    /// Check if this is a control frame
    pub fn is_control_frame(&self) -> bool {
        matches!(
            self,
            FrameType::End(_)
                | FrameType::LLMFullResponseStart(_)
                | FrameType::LLMFullResponseEnd(_)
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_frame_creation() {
        let text_frame = TextFrame::new("Hello world".to_string());
        assert_eq!(text_frame.text, "Hello world");
        assert!(text_frame.id() > 0);
        assert!(text_frame.name().contains("TextFrame"));
    }

    #[test]
    fn test_llm_text_frame() {
        let llm_frame = LLMTextFrame::new("AI response".to_string());
        assert_eq!(llm_frame.text_frame.text, "AI response");
        assert!(llm_frame.name().contains("LLMTextFrame"));
    }

    #[test]
    fn test_audio_frame() {
        let audio_data = vec![0u8; 1024]; // 1KB of audio data
        let audio_frame = OutputAudioRawFrame::new(audio_data.clone(), 16000, 1);

        assert_eq!(audio_frame.audio_frame.audio, audio_data);
        assert_eq!(audio_frame.audio_frame.sample_rate, 16000);
        assert_eq!(audio_frame.audio_frame.num_channels, 1);
        assert_eq!(audio_frame.audio_frame.num_frames, 512); // 1024 bytes / (1 channel * 2 bytes per sample)
    }

    #[test]
    fn test_system_frames() {
        let start_frame = StartFrame::new()
            .with_sample_rates(48000, 24000)
            .with_interruptions(true)
            .with_metrics(true);

        assert_eq!(start_frame.audio_in_sample_rate, 48000);
        assert_eq!(start_frame.audio_out_sample_rate, 24000);
        assert!(start_frame.allow_interruptions);
        assert!(start_frame.enable_metrics);

        let cancel_frame = CancelFrame::new();
        assert!(cancel_frame.name().contains("CancelFrame"));

        let error_frame = ErrorFrame::new("Test error".to_string());
        assert_eq!(error_frame.error, "Test error");
        assert!(!error_frame.fatal);

        let fatal_error_frame = ErrorFrame::fatal("Fatal error".to_string());
        assert!(fatal_error_frame.fatal);
    }

    #[test]
    fn test_control_frames() {
        let end_frame = EndFrame::new();
        assert!(end_frame.name().contains("EndFrame"));

        let llm_start = LLMFullResponseStartFrame::new();
        assert!(llm_start.name().contains("LLMFullResponseStartFrame"));

        let llm_end = LLMFullResponseEndFrame::new();
        assert!(llm_end.name().contains("LLMFullResponseEndFrame"));
    }

    #[test]
    fn test_frame_type_enum() {
        let text_frame = TextFrame::new("Test".to_string());
        let frame_type = FrameType::Text(text_frame);

        assert!(frame_type.is_data_frame());
        assert!(!frame_type.is_system_frame());
        assert!(!frame_type.is_control_frame());

        let start_frame = StartFrame::new();
        let frame_type = FrameType::Start(start_frame);

        assert!(!frame_type.is_data_frame());
        assert!(frame_type.is_system_frame());
        assert!(!frame_type.is_control_frame());

        let end_frame = EndFrame::new();
        let frame_type = FrameType::End(end_frame);

        assert!(!frame_type.is_data_frame());
        assert!(!frame_type.is_system_frame());
        assert!(frame_type.is_control_frame());
    }

    #[test]
    fn test_frame_metadata() {
        let mut text_frame = TextFrame::new("Test".to_string());

        text_frame
            .metadata_mut()
            .insert("key1".to_string(), "value1".to_string());
        text_frame
            .metadata_mut()
            .insert("key2".to_string(), "value2".to_string());

        assert_eq!(
            text_frame.metadata().get("key1"),
            Some(&"value1".to_string())
        );
        assert_eq!(
            text_frame.metadata().get("key2"),
            Some(&"value2".to_string())
        );
    }

    #[test]
    fn test_transport_fields() {
        let mut audio_frame = OutputAudioRawFrame::new(vec![0u8; 100], 16000, 1);

        audio_frame.set_transport_source(Some("microphone".to_string()));
        audio_frame.set_transport_destination(Some("speaker".to_string()));

        assert_eq!(audio_frame.transport_source(), Some("microphone"));
        assert_eq!(audio_frame.transport_destination(), Some("speaker"));
    }

    #[test]
    fn test_keypad_entry() {
        assert_eq!(KeypadEntry::One.to_string(), "1");
        assert_eq!(KeypadEntry::Star.to_string(), "*");
        assert_eq!(KeypadEntry::Pound.to_string(), "#");
    }

    #[test]
    fn test_frame_display() {
        let text_frame = TextFrame::new("Hello".to_string());
        let display_str = format!("{}", text_frame);
        assert!(display_str.contains("TextFrame"));
        assert!(display_str.contains("Hello"));

        let error_frame = ErrorFrame::new("Test error".to_string());
        let display_str = format!("{}", error_frame);
        assert!(display_str.contains("Test error"));
        assert!(display_str.contains("fatal: false"));
    }
}
