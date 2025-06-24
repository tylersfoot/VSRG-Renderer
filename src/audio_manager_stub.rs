use std::path::PathBuf;
use std::time::Instant;

use hound::WavReader;

/// No-op implementation of `AudioManager` used when the `audio` feature is disabled.
pub struct AudioManager {
    pub audio_source_path: Option<PathBuf>,
    current_time_ms: f64,
    rate: f64,
    volume: f64,
    length: Option<f64>,
    current_error: Option<String>,
    is_playing: bool,
    #[allow(dead_code)]
    playback_start_instant: Option<Instant>,
}

impl AudioManager {
    pub fn new() -> Result<Self, String> {
        Ok(Self {
            audio_source_path: None,
            current_time_ms: 0.0,
            rate: 1.0,
            volume: 0.03,
            length: None,
            current_error: None,
            is_playing: false,
            playback_start_instant: None,
        })
    }

    pub fn set_audio_path(&mut self, path: Option<PathBuf>) {
        self.audio_source_path = path.clone();
        self.length = None;
        if let Some(p) = path {
            if let Ok(reader) = WavReader::open(&p) {
                let spec = reader.spec();
                let duration = reader.duration() as f64 / spec.sample_rate as f64;
                self.length = Some(duration * 1000.0);
            }
        }
    }

    pub fn play(&mut self) {
        self.is_playing = true;
        self.playback_start_instant = Some(Instant::now());
    }

    pub fn pause(&mut self) {
        self.is_playing = false;
        self.playback_start_instant = None;
    }

    pub fn restart(&mut self) {
        self.current_time_ms = 0.0;
        self.is_playing = false;
        self.playback_start_instant = None;
    }

    pub fn seek_ms(&mut self, ms: f64) {
        let target = self
            .length
            .map_or(ms.max(0.0), |len| ms.clamp(0.0, len));
        self.current_time_ms = target;
    }

    pub fn get_current_song_time_ms(&self) -> f64 {
        self.current_time_ms
    }

    pub fn is_playing(&self) -> bool {
        self.is_playing
    }

    pub const fn get_total_duration_ms(&self) -> Option<f64> {
        self.length
    }

    pub fn set_volume(&mut self, volume: f64) {
        self.volume = volume;
    }

    pub const fn get_volume(&self) -> f64 {
        self.volume
    }

    pub fn set_rate(&mut self, rate: f64) {
        self.rate = rate;
    }

    pub const fn get_rate(&self) -> f64 {
        self.rate
    }

    pub const fn get_error(&self) -> Option<&String> {
        self.current_error.as_ref()
    }
}