// src/audio_manager.rs

use rodio::{source::Source as _, Decoder, OutputStream, OutputStreamHandle, Sink};
use std::{fs::File, io::BufReader, path::PathBuf, time::Instant};

const INITIAL_AUDIO_VOLUME: f64 = 0.03;
const INITIAL_AUDIO_RATE: f64 = 1.0;

/// Manages audio playback using the Rodio library.
pub struct AudioManager {
    _stream: OutputStream,
    stream_handle: OutputStreamHandle,
    pub sink: Option<Sink>,
    pub audio_source_path: Option<PathBuf>,
    current_error: Option<String>,

    // timing related fields
    playback_start_instant: Option<Instant>, // when current play segment started
    playback_start_rate: f64,                // rate at time of segment start
    accumulated_play_time_ms: f64,           // total time audio has played across pauses
    is_audio_engine_paused: bool,            // to reflect actual sink state

    length: Option<f64>, // length of audio
    rate: f64,           // playback rate
    volume: f64,
}

impl AudioManager {
    /// Creates a new `AudioManager` instance with an audio output stream and an initial sink.
    pub fn new() -> Result<Self, String> {
        let (stream, stream_handle) = OutputStream::try_default()
            .map_err(|e| format!("Failed to get audio output stream: {e}"))?;

        let initial_sink_result = Sink::try_new(&stream_handle);
        let initial_sink = match initial_sink_result {
            Ok(s) => s,
            Err(e) => return Err(format!("Failed to create initial audio sink: {e}")),
        };

        initial_sink.set_volume(INITIAL_AUDIO_VOLUME as f32);
        initial_sink.set_speed(INITIAL_AUDIO_RATE as f32);
        initial_sink.pause();

        Ok(Self {
            _stream: stream,
            stream_handle,
            sink: Some(initial_sink),
            audio_source_path: None,
            current_error: None,
            playback_start_instant: None,
            playback_start_rate: INITIAL_AUDIO_RATE,
            accumulated_play_time_ms: 0.0,
            is_audio_engine_paused: true,
            length: None,
            rate: INITIAL_AUDIO_RATE,
            volume: INITIAL_AUDIO_VOLUME,
        })
    }

    /// Sets the audio source path and verifies if the audio file is decodable.
    pub fn set_audio_path(&mut self, path: Option<PathBuf>) {
        self.audio_source_path = path;
        self.current_error = None;
        self.length = None; // reset duration when path changes

        if self.audio_source_path.is_none() {
            self.current_error = Some("No audio file specified in map.".to_string());
        } else if let Some(p) = &self.audio_source_path {
            match File::open(p) {
                Ok(file_handle) => match Decoder::new(BufReader::new(file_handle)) {
                    Ok(decoder) => {
                        if let Some(duration) = decoder.total_duration() {
                            self.length = Some(duration.as_secs_f64() * 1000f64);
                        }
                        log::info!(
                            "Audio path set and verified decodable: {:?}, Duration: {:?} ms",
                            p.display(),
                            self.length
                        );
                    }
                    Err(_) => {
                        self.current_error =
                            Some(format!("Failed to decode audio from: {:?}", p.display()));
                    }
                },
                Err(_) => {
                    self.current_error =
                        Some(format!("Failed to open audio file at: {:?}", p.display()));
                }
            }
        }
    }

    /// Returns the current audio source path.
    fn load_and_append_to_sink(&mut self) -> bool {
        if let Some(s) = self.sink.as_mut() {
            if let Some(path) = &self.audio_source_path {
                log::info!(
                    "Audiomanager: Attempting to load and append: {:?}",
                    path.display()
                );
                match File::open(path) {
                    Ok(file) => match Decoder::new(BufReader::new(file)) {
                        Ok(source) => {
                            // store total duration if not already
                            if self.length.is_none() {
                                if let Some(duration) = source.total_duration() {
                                    self.length = Some(duration.as_secs_f64() * 1000f64);
                                }
                            }
                            s.append(source);
                            self.current_error = None;
                            log::info!("Audiomanager: Audio loaded and appended to sink.");
                            return true;
                        }
                        Err(e) => {
                            let err_msg = format!("Audiomanager: Failed to decode audio: {e}");
                            log::error!("{err_msg}");
                            self.current_error = Some(err_msg);
                        }
                    },
                    Err(e) => {
                        let err_msg = format!("Audiomanager: Failed to open audio file: {e}");
                        log::error!("{err_msg}");
                        self.current_error = Some(err_msg);
                    }
                }
            } else {
                let err_msg = "Audiomanager: No audio source path to load.".to_string();
                log::info!("{err_msg}");
                self.current_error = Some(err_msg);
            }
        } else {
            let err_msg = "Audiomanager: No audio sink available to load into.".to_string();
            log::info!("{err_msg}");
            self.current_error = Some(err_msg);
        }
        false
    }

    /// Loads the audio file into the sink if not already loaded.
    pub fn play(&mut self) {
        let need_load = self.sink.as_ref().is_some_and(rodio::Sink::empty);

        if let Some(s) = self.sink.as_mut() {
            if s.is_paused() || need_load {
                // play if paused or if empty (needs loading)
                if need_load && !self.load_and_append_to_sink() {
                    self.is_audio_engine_paused = true; // ensure state reflects failure
                    return;
                }
                // re-borrow after possible load
                if let Some(sink_ref) = self.sink.as_mut() {
                    sink_ref.play();
                }
                self.playback_start_instant = Some(Instant::now());
                self.playback_start_rate = self.rate;
                self.is_audio_engine_paused = false;
                log::info!("Audiomanager: Audio playing/resumed.");
            }
        } else {
            self.current_error = Some("Play called but no sink exists.".to_string());
            log::info!("{}", self.current_error.as_ref().unwrap());
            self.is_audio_engine_paused = true;
        }
    }

    /// Pauses playback and records the elapsed time.
    pub fn pause(&mut self) {
        if let Some(s) = self.sink.as_mut() {
            if !s.is_paused() {
                s.pause();
                if let Some(start_instant) = self.playback_start_instant.take() {
                    self.accumulated_play_time_ms +=
                        start_instant.elapsed().as_secs_f64() * 1000f64 * self.playback_start_rate;
                }
                self.is_audio_engine_paused = true;
                log::info!(
                    "Audiomanager: Audio paused. Accumulated time: {} ms",
                    self.accumulated_play_time_ms
                );
            }
        }
    }

    /// Stops the audio playback, clears the sink, and resets the state.
    pub fn restart(&mut self) {
        self.accumulated_play_time_ms = 0f64;
        self.playback_start_instant = None;
        self.playback_start_rate = self.rate;
        self.is_audio_engine_paused = true; // will be set to false by play() if successful

        if let Some(s) = self.sink.as_mut() {
            s.stop();
            s.clear();
            log::info!("Audiomanager: Sink stopped and cleared for restart.");
        } else {
            match Sink::try_new(&self.stream_handle) {
                Ok(new_sink) => {
                    new_sink.set_volume(self.volume as f32);
                    new_sink.set_speed(self.rate as f32);
                    new_sink.pause();
                    self.sink = Some(new_sink);
                    log::info!("Audiomanager: New sink created on restart.");
                }
                Err(e) => {
                    let err_msg = format!("Audiomanager: Failed to create sink on restart: {e}");
                    log::error!("{err_msg}");
                    self.current_error = Some(err_msg);
                }
            }
        }
        // after restart, play() will handle loading and starting
    }

    /// Seeks to the specified position in milliseconds.
    ///
    /// If audio was playing before the seek, playback will resume from the new
    /// position. Otherwise, the sink remains paused.
    pub fn seek_ms(&mut self, ms: f64) {
        let target_ms = self.length.map_or(ms.max(0.0), |len| ms.clamp(0.0, len));

        let was_playing = self.is_playing();

        self.accumulated_play_time_ms = target_ms;
        self.playback_start_instant = if was_playing {
            Some(Instant::now())
        } else {
            None
        };
        self.playback_start_rate = self.rate;

        if let Some(old) = self.sink.take() {
            old.stop();
        }

        match Sink::try_new(&self.stream_handle) {
            Ok(new_sink) => {
                new_sink.set_volume(self.volume as f32);
                new_sink.set_speed(self.rate as f32);

                if let Some(path) = &self.audio_source_path {
                    match File::open(path) {
                        Ok(file) => match Decoder::new(BufReader::new(file)) {
                            Ok(decoder) => {
                                let source = decoder.skip_duration(
                                    std::time::Duration::from_millis(target_ms as u64),
                                );
                                new_sink.append(source);
                                if was_playing {
                                    new_sink.play();
                                    self.is_audio_engine_paused = false;
                                } else {
                                    new_sink.pause();
                                    self.is_audio_engine_paused = true;
                                }
                                self.sink = Some(new_sink);
                                self.current_error = None;
                            }
                            Err(e) => {
                                let err_msg = format!("Audiomanager: Failed to decode audio: {e}");
                                log::error!("{err_msg}");
                                self.current_error = Some(err_msg);
                                self.sink = Some(new_sink);
                            }
                        },
                        Err(e) => {
                            let err_msg = format!("Audiomanager: Failed to open audio file: {e}");
                            log::error!("{err_msg}");
                            self.current_error = Some(err_msg);
                            self.sink = Some(new_sink);
                        }
                    }
                } else {
                    let err_msg = "Audiomanager: No audio source path to seek.".to_string();
                    log::error!("{err_msg}");
                    self.current_error = Some(err_msg);
                    new_sink.pause();
                    self.sink = Some(new_sink);
                    self.is_audio_engine_paused = true;
                }
            }
            Err(e) => {
                let err_msg = format!("Audiomanager: Failed to create sink on seek: {e}");
                log::error!("{err_msg}");
                self.current_error = Some(err_msg);
                self.is_audio_engine_paused = true;
            }
        }
    }


    /// Returns the current playback time in milliseconds.
    pub fn get_current_song_time_ms(&self) -> f64 {
        let mut current_time = self.accumulated_play_time_ms;
        if !self.is_audio_engine_paused {
            if let Some(start_instant) = self.playback_start_instant {
                current_time = self.accumulated_play_time_ms
                    + (start_instant.elapsed().as_secs_f64() * 1000f64 * self.playback_start_rate);
            }
        }
        // clamp time to total duration if available
        self.length.map_or(current_time, |total_duration| {
            current_time.min(total_duration)
        })
    }

    /// Returns whether the audio is currently playing.
    pub fn is_playing(&self) -> bool {
        !self.is_audio_engine_paused
            && self
                .sink
                .as_ref()
                .is_some_and(|s| !s.empty() && !s.is_paused())
    }

    /// Returns the duration of the audio file in milliseconds.
    pub const fn get_total_duration_ms(&self) -> Option<f64> {
        self.length
    }

    /// Sets the volume of the audio playback.
    pub fn set_volume(&mut self, volume: f64) {
        self.volume = volume.clamp(0.0, 1.5); // clamp volume
        if let Some(s) = self.sink.as_mut() {
            s.set_volume(self.volume as f32);
        }
        log::info!("Audiomanager: Volume set to {}", self.volume);
    }

    /// Returns the current volume of the audio playback.
    pub const fn get_volume(&self) -> f64 {
        self.volume
    }

    /// Sets the playback rate of the audio.
    pub fn set_rate(&mut self, rate: f64) {
        self.rate = rate.max(0.1); // prevent rate from being too low or zero
        if let Some(s) = self.sink.as_mut() {
            s.set_speed(self.rate as f32);
        }
        if !self.is_audio_engine_paused {
            if let Some(start_instant) = self.playback_start_instant.take() {
                self.accumulated_play_time_ms +=
                    start_instant.elapsed().as_secs_f64() * 1000f64 * self.playback_start_rate;
            }
            self.playback_start_instant = Some(Instant::now());
            self.playback_start_rate = self.rate;
        }
        log::info!("Audiomanager: Rate set to {}", self.rate);
    }

    /// Returns the current playback rate of the audio.
    pub const fn get_rate(&self) -> f64 {
        self.rate
    }

    /// Returns the current error message, if any.
    pub const fn get_error(&self) -> Option<&String> {
        self.current_error.as_ref()
    }
}
