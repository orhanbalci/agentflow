//! Rodio-based audio mixer for file playback integration.
//!
//! Provides an audio mixer that combines incoming audio with audio loaded from
//! files using the rodio and hound libraries. Supports multiple audio formats and
//! runtime configuration changes.

use async_trait::async_trait;
use hound::{SampleFormat, WavReader};
use log::{debug, error, warn};
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::audio::mixer::{BaseAudioMixer, MixerResult};
use crate::frames::MixerControlFrame;

/// Audio data loaded from a file
#[derive(Debug, Clone)]
struct SoundData {
    samples: Vec<i16>,
}

/// Audio mixer that combines incoming audio with file-based audio.
///
/// This mixer loads audio files using the hound library and mixes them with
/// incoming transport audio. It supports multiple audio formats through hound
/// and allows runtime configuration changes. The audio files should be mono
/// and match the sample rate of the output transport for best results.
///
/// Multiple files can be loaded, each with a different name. The mixer can
/// switch between files, adjust volume, and enable/disable looping at runtime.
#[derive(Debug)]
pub struct SoundfileMixer {
    sound_files: HashMap<String, String>,
    default_sound: String,
    volume: f32,
    sample_rate: u32,
    mixing: bool,
    loop_enabled: bool,

    // Runtime state
    sounds: Arc<RwLock<HashMap<String, SoundData>>>,
    current_sound: Arc<RwLock<String>>,
    sound_position: Arc<RwLock<usize>>,
}

impl SoundfileMixer {
    /// Create a new soundfile mixer.
    ///
    /// # Arguments
    /// * `sound_files` - Mapping of sound names to file paths for loading
    /// * `default_sound` - Name of the default sound to play initially
    /// * `volume` - Mixing volume level (0.0 to 1.0), defaults to 0.4
    /// * `mixing` - Whether mixing is initially enabled, defaults to true
    /// * `loop_enabled` - Whether to loop audio files when they end, defaults to true
    ///
    /// # Examples
    /// ```
    /// use std::collections::HashMap;
    /// use agentflow::audio::mixer::SoundfileMixer;
    ///
    /// let mut sound_files = HashMap::new();
    /// sound_files.insert("background".to_string(), "audio/background.wav".to_string());
    /// sound_files.insert("notification".to_string(), "audio/notification.wav".to_string());
    ///
    /// let mixer = SoundfileMixer::new(
    ///     sound_files,
    ///     "background".to_string(),
    ///     0.4,
    ///     true,
    ///     true,
    /// );
    /// ```
    pub fn new(
        sound_files: HashMap<String, String>,
        default_sound: String,
        volume: f32,
        mixing: bool,
        loop_enabled: bool,
    ) -> Self {
        Self {
            sound_files,
            current_sound: Arc::new(RwLock::new(default_sound.clone())),
            default_sound,
            volume: volume.clamp(0.0, 1.0),
            sample_rate: 0,
            mixing,
            loop_enabled,
            sounds: Arc::new(RwLock::new(HashMap::new())),
            sound_position: Arc::new(RwLock::new(0)),
        }
    }

    /// Load an audio file into memory for mixing.
    ///
    /// # Arguments
    /// * `sound_name` - The name to associate with this sound
    /// * `file_path` - Path to the audio file to load
    ///
    /// # Errors
    /// Returns an error if the file cannot be read or has an incompatible format
    async fn load_sound_file(&self, sound_name: &str, file_path: &str) -> MixerResult<()> {
        debug!("Loading mixer sound from {}", file_path);

        let path = Path::new(file_path);
        if !path.exists() {
            return Err(format!("Audio file not found: {}", file_path).into());
        }

        // Load WAV file using hound
        let reader = WavReader::open(path)
            .map_err(|e| format!("Failed to open audio file {}: {}", file_path, e))?;

        let spec = reader.spec();

        // Check if sample rate matches (warn if not)
        if self.sample_rate > 0 && spec.sample_rate != self.sample_rate {
            warn!(
                "Sound file {} has sample rate {} but mixer expects {}",
                file_path, spec.sample_rate, self.sample_rate
            );
        }

        // Read samples based on format
        let samples: Vec<i16> = match spec.sample_format {
            SampleFormat::Float => {
                let float_samples: Result<Vec<f32>, _> = reader.into_samples().collect();
                match float_samples {
                    Ok(samples) => samples
                        .into_iter()
                        .map(|s| (s * i16::MAX as f32) as i16)
                        .collect(),
                    Err(e) => return Err(format!("Failed to read float samples: {}", e).into()),
                }
            }
            SampleFormat::Int => {
                match spec.bits_per_sample {
                    16 => {
                        let int_samples: Result<Vec<i16>, _> = reader.into_samples().collect();
                        match int_samples {
                            Ok(samples) => samples,
                            Err(e) => {
                                return Err(format!("Failed to read 16-bit samples: {}", e).into())
                            }
                        }
                    }
                    24 => {
                        let int_samples: Result<Vec<i32>, _> = reader.into_samples().collect();
                        match int_samples {
                            Ok(samples) => samples
                                .into_iter()
                                .map(|s| (s >> 8) as i16) // Convert 24-bit to 16-bit
                                .collect(),
                            Err(e) => {
                                return Err(format!("Failed to read 24-bit samples: {}", e).into())
                            }
                        }
                    }
                    32 => {
                        let int_samples: Result<Vec<i32>, _> = reader.into_samples().collect();
                        match int_samples {
                            Ok(samples) => samples
                                .into_iter()
                                .map(|s| (s >> 16) as i16) // Convert 32-bit to 16-bit
                                .collect(),
                            Err(e) => {
                                return Err(format!("Failed to read 32-bit samples: {}", e).into())
                            }
                        }
                    }
                    _ => {
                        return Err(
                            format!("Unsupported bit depth: {}", spec.bits_per_sample).into()
                        );
                    }
                }
            }
        };

        // Convert stereo to mono if necessary
        let mono_samples = if spec.channels == 1 {
            samples
        } else if spec.channels == 2 {
            // Convert stereo to mono by averaging channels
            samples
                .chunks_exact(2)
                .map(|chunk| ((chunk[0] as i32 + chunk[1] as i32) / 2) as i16)
                .collect()
        } else {
            return Err(format!("Unsupported channel count: {}", spec.channels).into());
        };

        let sound_data = SoundData {
            samples: mono_samples,
        };

        // Store the loaded sound
        let mut sounds = self.sounds.write().await;
        sounds.insert(sound_name.to_string(), sound_data);

        debug!(
            "Successfully loaded sound '{}' with {} samples",
            sound_name,
            sounds[sound_name].samples.len()
        );
        Ok(())
    }

    /// Change the currently playing sound file.
    ///
    /// # Arguments
    /// * `sound_name` - Name of the sound to switch to
    async fn change_sound(&self, sound_name: &str) -> MixerResult<()> {
        let sounds = self.sounds.read().await;
        if sounds.contains_key(sound_name) {
            let mut current_sound = self.current_sound.write().await;
            *current_sound = sound_name.to_string();

            // Reset position when changing sounds
            let mut position = self.sound_position.write().await;
            *position = 0;

            debug!("Changed to sound: {}", sound_name);
            Ok(())
        } else {
            error!("Sound '{}' is not available", sound_name);
            Err(format!("Sound '{}' not found", sound_name).into())
        }
    }

    /// Mix raw audio with the current sound file.
    ///
    /// # Arguments
    /// * `audio_bytes` - Raw audio bytes to mix with
    ///
    /// # Returns
    /// Mixed audio bytes combining input and file audio
    fn mix_with_sound(
        &self,
        audio_bytes: &[u8],
        current_sound_data: &SoundData,
        position: &mut usize,
    ) -> Vec<u8> {
        if !self.mixing {
            return audio_bytes.to_vec();
        }

        // Convert input bytes to i16 samples
        let audio_samples: Vec<i16> = audio_bytes
            .chunks_exact(2)
            .map(|chunk| i16::from_le_bytes([chunk[0], chunk[1]]))
            .collect();

        let chunk_size = audio_samples.len();
        let sound_samples = &current_sound_data.samples;

        // Check if we need to loop or if we've reached the end
        if *position + chunk_size > sound_samples.len() {
            if !self.loop_enabled {
                return audio_bytes.to_vec(); // Return original audio if not looping
            }
            *position = 0; // Reset to beginning
        }

        // Extract sound chunk
        let end_pos = (*position + chunk_size).min(sound_samples.len());
        let sound_chunk = &sound_samples[*position..end_pos];
        *position = end_pos;

        // Pad with zeros if sound chunk is shorter than audio chunk
        let mut padded_sound_chunk = sound_chunk.to_vec();
        while padded_sound_chunk.len() < chunk_size {
            padded_sound_chunk.push(0);
        }

        // Mix the audio by adding samples with volume control
        let mixed_samples: Vec<i16> = audio_samples
            .iter()
            .zip(padded_sound_chunk.iter())
            .map(|(&audio_sample, &sound_sample)| {
                let mixed = audio_sample as i32 + (sound_sample as f32 * self.volume) as i32;
                mixed.clamp(i16::MIN as i32, i16::MAX as i32) as i16
            })
            .collect();

        // Convert back to bytes
        mixed_samples
            .iter()
            .flat_map(|&sample| sample.to_le_bytes())
            .collect()
    }
}

#[async_trait]
impl BaseAudioMixer for SoundfileMixer {
    async fn start(&mut self, sample_rate: u32) -> MixerResult<()> {
        self.sample_rate = sample_rate;
        debug!(
            "Starting SoundfileMixer with sample rate: {} Hz",
            sample_rate
        );

        // Load all sound files
        for (sound_name, file_path) in &self.sound_files.clone() {
            if let Err(e) = self.load_sound_file(sound_name, file_path).await {
                error!("Failed to load sound '{}': {}", sound_name, e);
            }
        }

        Ok(())
    }

    async fn stop(&mut self) -> MixerResult<()> {
        debug!("Stopping SoundfileMixer");
        // Clear loaded sounds
        let mut sounds = self.sounds.write().await;
        sounds.clear();
        Ok(())
    }

    async fn process_frame(&mut self, frame: &MixerControlFrame) -> MixerResult<()> {
        // Handle different types of mixer control frames
        if let Some(enabled) = frame.enabled {
            self.mixing = enabled;
            debug!("Mixer enabled state changed to: {}", enabled);
        }

        if let Some(volume) = frame.volume {
            self.volume = volume.clamp(0.0, 1.0);
            debug!("Mixer volume changed to: {}", self.volume);
        }

        // Handle custom settings
        for (key, value) in &frame.settings {
            match key.as_str() {
                "sound" => {
                    if let Err(e) = self.change_sound(value).await {
                        error!("Failed to change sound: {}", e);
                    }
                }
                "volume" => {
                    if let Ok(vol) = value.parse::<f32>() {
                        self.volume = vol.clamp(0.0, 1.0);
                        debug!("Volume updated to: {}", self.volume);
                    }
                }
                "loop" => {
                    if let Ok(loop_val) = value.parse::<bool>() {
                        self.loop_enabled = loop_val;
                        debug!("Loop enabled changed to: {}", loop_val);
                    }
                }
                _ => {
                    debug!("Unknown mixer setting: {} = {}", key, value);
                }
            }
        }

        Ok(())
    }

    async fn mix(&mut self, audio: &[u8]) -> MixerResult<Vec<u8>> {
        if !self.mixing {
            return Ok(audio.to_vec());
        }

        let current_sound_name = {
            let current_sound = self.current_sound.read().await;
            current_sound.clone()
        };

        let sounds = self.sounds.read().await;
        if let Some(sound_data) = sounds.get(&current_sound_name) {
            let mut position = self.sound_position.write().await;
            let mixed_audio = self.mix_with_sound(audio, sound_data, &mut position);
            Ok(mixed_audio)
        } else {
            // No sound loaded, return original audio
            Ok(audio.to_vec())
        }
    }

    fn get_config(&self) -> HashMap<String, String> {
        let mut config = HashMap::new();
        config.insert("mixing".to_string(), self.mixing.to_string());
        config.insert("volume".to_string(), self.volume.to_string());
        config.insert("sample_rate".to_string(), self.sample_rate.to_string());
        config.insert("loop_enabled".to_string(), self.loop_enabled.to_string());
        config.insert("default_sound".to_string(), self.default_sound.clone());
        config
    }

    fn is_enabled(&self) -> bool {
        self.mixing
    }

    fn get_volume(&self) -> f32 {
        self.volume
    }
}
