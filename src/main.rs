#![allow(unused)]

// use bitflags::bitflags;
use macroquad::color::Color;
use macroquad::prelude::*;
use ordered_float::OrderedFloat;
use rodio::{source::Source, Decoder, OutputStream, OutputStreamHandle, Sink};
use serde::{Deserialize, Deserializer, Serialize};
use std::collections::{HashMap, HashSet};
use std::fs::{self, File};
use std::io::{BufReader, Write};
use std::path::{Path, PathBuf};
use std::time::{Instant};

// --- YAML/QUA Types ---
// Mapping -> HashMap<K,V>, BTreeMap<K,V>, Struct
// Sequence -> Vec<T>
// String -> String
// Number -> f32, i32, u64
// Bool -> bool
// Null -> Value::Null

const DEFAULT_SCROLL_GROUP_ID: &str = "$Default";
const GLOBAL_SCROLL_GROUP_ID: &str = "$Global";
const INITIAL_AUDIO_VOLUME: f32 = 0.05;
const INITIAL_AUDIO_SPEED: f32 = 1.0;
const SNAP_EPSILON: f32 = 0.01; // tolerance for float comparisons

// snap colors
const SNAP_TYPES: &[SnapType] = &[
    SnapType { divisor: 1,  color: Color::new(255.0 / 255.0, 96.0  / 255.0, 96.0  / 255.0, 1.0) },
    SnapType { divisor: 2,  color: Color::new(61.0  / 255.0, 132.0 / 255.0, 255.0 / 255.0, 1.0) },
    SnapType { divisor: 3,  color: Color::new(178.0 / 255.0, 71.0  / 255.0, 255.0 / 255.0, 1.0) },
    SnapType { divisor: 4,  color: Color::new(255.0 / 255.0, 238.0 / 255.0, 58.0  / 255.0, 1.0) },
    SnapType { divisor: 6,  color: Color::new(255.0 / 255.0, 146.0 / 255.0, 210.0 / 255.0, 1.0) },
    SnapType { divisor: 8,  color: Color::new(255.0 / 255.0, 167.0 / 255.0, 61.0  / 255.0, 1.0) },
    SnapType { divisor: 12, color: Color::new(132.0 / 255.0, 255.0 / 255.0, 255.0 / 255.0, 1.0) },
    SnapType { divisor: 24, color: Color::new(127.0 / 255.0, 255.0 / 255.0, 138.0 / 255.0, 1.0) },
    SnapType { divisor: 48, color: Color::new(200.0 / 255.0, 200.0 / 255.0, 200.0 / 255.0, 1.0) },
];

struct SnapType {
    divisor: u32, // e.g. 4 for 1/4 notes, 6 for 1/6 notes
    color: Color,
}

struct AudioManager {
    _stream: OutputStream,
    stream_handle: OutputStreamHandle,
    sink: Option<Sink>,
    audio_source_path: Option<PathBuf>,
    current_error: Option<String>,

    // timing related fields
    playback_start_instant: Option<Instant>, // when current play segment started
    accumulated_play_time_ms: f32,           // total time audio has played across pauses
    is_audio_engine_paused: bool,            // to reflect actual sink state

    total_duration_ms: Option<f32>,
    current_volume: f32,
    current_speed: f32,
}

impl AudioManager {
    pub fn new() -> Result<Self, String> {
        let (stream, stream_handle) = OutputStream::try_default()
            .map_err(|e| format!("Failed to get audio output stream: {}", e))?;

        let initial_sink_result = Sink::try_new(&stream_handle);
        let initial_sink = match initial_sink_result {
            Ok(s) => s,
            Err(e) => return Err(format!("Failed to create initial audio sink: {}", e)),
        };

        initial_sink.set_volume(INITIAL_AUDIO_VOLUME);
        initial_sink.set_speed(INITIAL_AUDIO_SPEED);
        initial_sink.pause();

        Ok(AudioManager {
            _stream: stream,
            stream_handle,
            sink: Some(initial_sink),
            audio_source_path: None,
            current_error: None,
            playback_start_instant: None,
            accumulated_play_time_ms: 0.0,
            is_audio_engine_paused: true,
            total_duration_ms: None,
            current_volume: INITIAL_AUDIO_VOLUME,
            current_speed: INITIAL_AUDIO_SPEED,
        })
    }

    pub fn set_audio_path(&mut self, path: Option<PathBuf>) {
        self.audio_source_path = path;
        self.current_error = None;
        self.total_duration_ms = None; // reset duration when path changes

        if self.audio_source_path.is_none() {
            self.current_error = Some("No audio file specified in map.".to_string());
        } else {
            if let Some(p) = &self.audio_source_path {
                match File::open(p) {
                    Ok(file_handle) => {
                        match Decoder::new(BufReader::new(file_handle)) {
                            Ok(decoder) => {
                                if let Some(duration) = decoder.total_duration() {
                                    self.total_duration_ms = Some(duration.as_millis() as f32);
                                }
                                println!("Audio path set and verified decodable: {:?}, Duration: {:?} ms", p.display(), self.total_duration_ms);
                            }
                            Err(_) => {
                                self.current_error = Some(format!("Failed to decode audio from: {:?}", p.display()));
                            }
                        }
                    }
                    Err(_) => {
                        self.current_error = Some(format!("Failed to open audio file at: {:?}", p.display()));
                    }
                }
            }
        }
    }
    fn load_and_append_to_sink(&mut self) -> bool {
        if let Some(s) = self.sink.as_mut() {
            if let Some(path) = &self.audio_source_path {
                println!(
                    "Audiomanager: Attempting to load and append: {:?}",
                    path.display()
                );
                match File::open(path) {
                    Ok(file) => match Decoder::new(BufReader::new(file)) {
                        Ok(source) => {
                            // store total duration if not already
                            if self.total_duration_ms.is_none() {
                                if let Some(duration) = source.total_duration() {
                                    self.total_duration_ms = Some(duration.as_millis() as f32);
                                }
                            }
                            s.append(source);
                            self.current_error = None;
                            println!("Audiomanager: Audio loaded and appended to sink.");
                            return true;
                        }
                        Err(e) => {
                            let err_msg = format!("Audiomanager: Failed to decode audio: {}", e);
                            eprintln!("{}", err_msg);
                            self.current_error = Some(err_msg);
                        }
                    },
                    Err(e) => {
                        let err_msg = format!("Audiomanager: Failed to open audio file: {}", e);
                        eprintln!("{}", err_msg);
                        self.current_error = Some(err_msg);
                    }
                }
            } else {
                let err_msg = "Audiomanager: No audio source path to load.".to_string();
                println!("{}", err_msg);
                self.current_error = Some(err_msg);
            }
        } else {
            let err_msg = "Audiomanager: No audio sink available to load into.".to_string();
            println!("{}", err_msg);
            self.current_error = Some(err_msg);
        }
        false
    }

    pub fn play(&mut self) {
        let need_load = if let Some(s) = self.sink.as_ref() {
            s.empty()
        } else {
            false
        };

        if let Some(s) = self.sink.as_mut() {
            if s.is_paused() || need_load {
                // play if paused or if empty (needs loading)
                if need_load {
                    if !self.load_and_append_to_sink() {
                        self.is_audio_engine_paused = true; // ensure state reflects failure
                        return;
                    }
                }
                // re-borrow after possible load
                if let Some(s) = self.sink.as_mut() {
                    s.play();
                }
                self.playback_start_instant = Some(Instant::now());
                self.is_audio_engine_paused = false;
                println!("Audiomanager: Audio playing/resumed.");
            }
        } else {
            self.current_error = Some("Play called but no sink exists.".to_string());
            println!("{}", self.current_error.as_ref().unwrap());
            self.is_audio_engine_paused = true;
        }
    }

    pub fn pause(&mut self) {
        if let Some(s) = self.sink.as_mut() {
            if !s.is_paused() {
                s.pause();
                if let Some(start_instant) = self.playback_start_instant.take() {
                    self.accumulated_play_time_ms += start_instant.elapsed().as_millis() as f32;
                }
                self.is_audio_engine_paused = true;
                println!(
                    "Audiomanager: Audio paused. Accumulated time: {} ms",
                    self.accumulated_play_time_ms
                );
            }
        }
    }

    pub fn restart(&mut self) {
        self.accumulated_play_time_ms = 0.0;
        self.playback_start_instant = None;
        self.is_audio_engine_paused = true; // will be set to false by play() if successful

        if let Some(s) = self.sink.as_mut() {
            s.stop();
            s.clear();
            println!("Audiomanager: Sink stopped and cleared for restart.");
        } else {
            match Sink::try_new(&self.stream_handle) {
                Ok(new_sink) => {
                    new_sink.set_volume(self.current_volume);
                    new_sink.set_speed(self.current_speed);
                    new_sink.pause();
                    self.sink = Some(new_sink);
                    println!("Audiomanager: New sink created on restart.");
                }
                Err(e) => {
                    let err_msg = format!("Audiomanager: Failed to create sink on restart: {}", e);
                    eprintln!("{}", err_msg);
                    self.current_error = Some(err_msg);
                    return;
                }
            }
        }
        // after restart, play() will handle loading and starting
    }

    pub fn get_current_song_time_ms(&self) -> f32 {
        let mut current_time = self.accumulated_play_time_ms;
        if !self.is_audio_engine_paused {
            if let Some(start_instant) = self.playback_start_instant {
                current_time = self.accumulated_play_time_ms
                    + (start_instant.elapsed().as_millis() as f32 * self.current_speed);
            }
        }
        // clamp time to total duration if available
        if let Some(total_duration) = self.total_duration_ms {
            current_time.min(total_duration)
        } else {
            current_time
        }
    }

    pub fn is_playing(&self) -> bool {
        !self.is_audio_engine_paused
            && self
                .sink
                .as_ref()
                .map_or(false, |s| !s.empty() && !s.is_paused())
    }

    pub fn get_total_duration_ms(&self) -> Option<f32> {
        self.total_duration_ms
    }

    pub fn set_volume(&mut self, volume: f32) {
        self.current_volume = volume.clamp(0.0, 1.5); // clamp volume
        if let Some(s) = self.sink.as_mut() {
            s.set_volume(self.current_volume);
        }
        println!("Audiomanager: Volume set to {}", self.current_volume);
    }

    pub fn get_volume(&self) -> f32 {
        self.current_volume
    }

    pub fn set_speed(&mut self, speed: f32) {
        self.current_speed = speed.max(0.1); // prevent speed from being too low or zero
        if let Some(s) = self.sink.as_mut() {
            s.set_speed(self.current_speed);
        }
        if !self.is_audio_engine_paused {
            if let Some(start_instant) = self.playback_start_instant.take() {
                self.accumulated_play_time_ms += start_instant.elapsed().as_millis() as f32;
            }
            self.playback_start_instant = Some(Instant::now());
        }
        println!("Audiomanager: Speed set to {}", self.current_speed);
    }

    pub fn get_speed(&self) -> f32 {
        self.current_speed
    }

    pub fn get_error(&self) -> Option<&String> {
        self.current_error.as_ref()
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "PascalCase")]
struct Map {
    audio_file: Option<String>,      // audio file name
    song_preview_time: Option<i32>,  // time (ms) of the song where the preview starts
    background_file: Option<String>, // background file name
    banner_file: Option<String>,     // mapset banner name
    map_id: Option<i32>,             // unique Map Identifier (-1 if not submitted)
    map_set_id: Option<i32>,         // unique Map Set identifier (-1 if not submitted)
    #[serde(default)]
    mode: String,                    // game mode for this map {Keys4, Keys7}
    title: Option<String>,           // song title
    artist: Option<String>,          // song artist
    source: Option<String>,          // source of the song (album, mixtape, etc.)
    tags: Option<String>,            // any tags that could be used to help find the song
    creator: Option<String>,         // map creator
    difficulty_name: Option<String>, // map difficulty name
    description: Option<String>,     // map description
    genre: Option<String>,           // song genre
    #[serde(rename = "LegacyLNRendering")]
    #[serde(default)]
    legacy_ln_rendering: bool, // whether to use the old LN rendering system (earliest/latest -> start/end)
    #[serde(rename = "BPMDoesNotAffectScrollVelocity")]
    #[serde(default)]
    bpm_does_not_affect_scroll_velocity: bool, // indicates if BPM changes affect SV
    initial_scroll_velocity: Option<f32>, // the initial SV before the first SV change
        // get => DefaultScrollGroup.InitialScrollVelocity;
        // set => DefaultScrollGroup.InitialScrollVelocity = value;
    #[serde(default)]
    has_scratch_key: bool, // +1 scratch key (5/8 key play)
    #[serde(default)]
    editor_layers: Vec<serde_yaml::Value>,
    #[serde(default)]
    bookmarks: Vec<serde_yaml::Value>,
    #[serde(default)]
    custom_audio_samples: Vec<serde_yaml::Value>,
    #[serde(default)]
    sound_effects: Vec<SoundEffect>,
    #[serde(default)]
    timing_points: Vec<TimingPoint>,
    #[serde(rename = "SliderVelocities")]
    #[serde(default)]
    scroll_velocities: Vec<ControlPoint>,
        // get => DefaultScrollGroup.ScrollVelocities;
        // private set => DefaultScrollGroup.ScrollVelocities = value;
    #[serde(default)]
    scroll_speed_factors: Vec<ControlPoint>,
        // get => DefaultScrollGroup.ScrollSpeedFactors;
        // private set => DefaultScrollGroup.ScrollSpeedFactors = value;
    #[serde(default)]
    hit_objects: Vec<HitObject>,
    #[serde(default)]
    timing_groups: HashMap<String, TimingGroup>,
    #[serde(skip)]
    default_scroll_group: TimingGroup,
    #[serde(skip)]
    global_scroll_group: TimingGroup,
        // (ScrollGroup)TimingGroups[GlobalScrollGroupId];
    #[serde(skip)]
    file_path: String, // map file path
}

impl Map {
    fn length(&self) -> f32 {
        // length of the map in ms
        if let Some(last_object) = self.hit_objects.last() {
            return last_object.start_time + last_object.end_time.unwrap_or(0.0);
        }
        0.0
    }

    fn link_default_scroll_group(&mut self) {
        // links DefaultScrollGroup to TimingGroups so that
        // TimingGroups[DefaultScrollGroupId] points to that group
        self.timing_groups
            .insert(DEFAULT_SCROLL_GROUP_ID.to_string(), TimingGroup::default());
        self.timing_groups
            .entry(GLOBAL_SCROLL_GROUP_ID.to_string())
            .or_insert_with(|| TimingGroup::default());
    }

    fn get_timing_point_at(&self, time: f32) -> Option<&TimingPoint> {
        // gets the timing point at a particular time in the map
        if self.timing_points.is_empty() {
            return None;
        }

        Some(at_time(self.timing_points.as_slice(), time).unwrap_or(&self.timing_points[0]))
    }

    fn get_scroll_velocity_at(&self, time: f32, timing_group_id: Option<&str>) -> Option<&ControlPoint> {
        // gets a scroll velocity at a particular time in the map
        let timing_group_id = timing_group_id.unwrap_or(DEFAULT_SCROLL_GROUP_ID);
        let timing_group = self.timing_groups.get(timing_group_id)?;
        timing_group.get_scroll_velocity_at(time)
    }

    fn get_scroll_speed_factor_at(&self, time: f32, timing_group_id: Option<&str>) -> Option<&ControlPoint> {
        // gets a scroll speed factor at a particular time in the map
        let timing_group_id = timing_group_id.unwrap_or(DEFAULT_SCROLL_GROUP_ID);
        let timing_group = self.timing_groups.get(timing_group_id)?;
        timing_group.get_scroll_speed_factor_at(time)
    }

    fn get_timing_point_length(&self, timing_point: &TimingPoint) -> f32 {
        // gets the length of a timing point in ms
        if let Some(next_timing_point) = self
            .timing_points
            .iter()
            .find(|tp| tp.start_time > timing_point.start_time) {
            return next_timing_point.start_time - timing_point.start_time;
        }
        0.0
    }

    fn get_timing_group_objects(&self, timing_group_id: &str) -> Vec<&HitObject> {
        // returns the list of hit objects that are in the specified group
        self.hit_objects
            .iter()
            .filter(|obj| obj.timing_group.as_deref() == Some(timing_group_id))
            .collect()
    }

    fn get_multiple_timing_group_objects(&self, timing_group_ids: &HashSet<String>) -> HashMap<String, Vec<&HitObject>> {
        // returns a hashmap of hit objects that are in the specified groups
        let mut result: HashMap<String, Vec<&HitObject>> = HashMap::new();
        for obj in &self.hit_objects {
            if let Some(timing_group) = &obj.timing_group {
                if timing_group_ids.contains(timing_group) {
                    result.entry(timing_group.clone()).or_default().push(obj);

                }
            }
        }
        result
    }

    fn get_key_count(&self, include_scratch: bool) -> i32 {
        // returns the number of keys in the map
        let mut key_count = match self.mode.as_str() {
            "Keys4" => 4,
            "Keys7" => 7,
            _ => panic!("Invalid game mode"),
        };

        if self.has_scratch_key && include_scratch {
            key_count += 1;
        }

        key_count
    }

    fn sort_scroll_velocities(&mut self) {
        for timing_group in self.timing_groups.values_mut() {
            sort_by_start_time(&mut timing_group.scroll_velocities);
        }
    }

    fn sort_scroll_speed_factors(&mut self) {
        for timing_group in self.timing_groups.values_mut() {
            sort_by_start_time(&mut timing_group.scroll_speed_factors);
        }
    }

    fn sort_hit_objects(&mut self) {
        sort_by_start_time(&mut self.hit_objects);
    }

    fn sort_sound_effects(&mut self) {
        sort_by_start_time(&mut self.sound_effects);
    }

    fn sort_timing_points(&mut self) {
        sort_by_start_time(&mut self.timing_points);
    }

    fn sort(&mut self) {
        self.link_default_scroll_group();

        self.sort_hit_objects();
        self.sort_sound_effects();
        self.sort_timing_points();
        self.sort_scroll_velocities();
        self.sort_scroll_speed_factors();
    }

    fn validate(&mut self) -> Result<(), Vec<String>> {
        // returns all validation errors (usually used in map creation)
        let mut errors = Vec::new();

        // if there aren't any hit objects
        if self.hit_objects.is_empty() {
            errors.push("There are no Hit Objects.".to_string());
        }

        // if there aren't any timing points
        if self.timing_points.is_empty() {
            errors.push("There are no timing points.".to_string());
        }

        // check if the mode is actually valid
        if self.mode != "Keys4" && self.mode != "Keys7" {
            errors.push(format!("The game mode '{}' is invalid.", self.mode));
        }

        // check that sound effects are valid
        for sound_effect in &self.sound_effects {
            // sample should be a valid array index
            if sound_effect.sample < 1 || sound_effect.sample > self.custom_audio_samples.len() as i32 {
                errors.push(format!(
                    "Sound effect at {} has an invalid sample index.",
                    sound_effect.start_time
                ));
            }

            // the sample volume should be between 1 and 100
            if sound_effect.volume < 1 || sound_effect.volume > 100 {
                errors.push(format!(
                    "Sound effect at {} has an invalid volume.",
                    sound_effect.start_time
                ));
            }
        }

        // check that hit objects are valid
        for hit_object in &self.hit_objects {
            // LN end times should be > start times
            if hit_object.is_long_note() && hit_object.get_end_time() <= hit_object.start_time {
                errors.push(format!(
                    "Long note at {} has an invalid end time.",
                    hit_object.start_time
                ));
            }

            // check that key sounds are valid
            for key_sound in &hit_object.key_sounds {
                // sample should be a valid array index
                if key_sound.sample < 1 || key_sound.sample > self.custom_audio_samples.len() as i32 {
                    errors.push(format!(
                        "Key sound at {} has an invalid sample index.",
                        hit_object.start_time
                    ));
                }

                // the sample volume should be above 0
                if key_sound.volume < 1 {
                    errors.push(format!(
                        "Key sound at {} has an invalid volume.",
                        hit_object.start_time
                    ));
                }
            }
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }

    fn get_common_bpm(&self) -> f32 {
        // finds the most common BPM in the map
        if self.timing_points.is_empty() {
            return 0.0;
        }
        if self.hit_objects.is_empty() {
            return self.timing_points[0].bpm;
        }

        let last_object = self.hit_objects
            .iter()
            .filter(|obj| obj.is_long_note())
            .max_by(|a, b| a.get_end_time().partial_cmp(&b.get_end_time()).unwrap())
            .unwrap_or(&self.hit_objects[0]);

        let mut last_time = if last_object.is_long_note() {
            last_object.get_end_time()
        } else {
            last_object.start_time
        };

        let mut durations: HashMap<OrderedFloat<f32>, i32> = HashMap::new();
        for timing_point in self.timing_points.iter().rev() {
            if timing_point.start_time > last_time {
                continue;
            }

            let duration = (last_time - timing_point.start_time) as i32;
            last_time = timing_point.start_time;

            let bpm_key = OrderedFloat(timing_point.bpm);
            *durations.entry(bpm_key).or_insert(0) += duration;
        }

        if durations.is_empty() {
            return self.timing_points[0].bpm;
        } else {
            durations
                .iter()
                .max_by(|a, b| a.1.cmp(b.1))
                .map(|(bpm, _)| bpm.0)
                .unwrap_or(self.timing_points[0].bpm)
        }
    }

    fn with_normalized_svs(&mut self) -> Map {
        // returns a new map with normalized SVs
        let mut new_map = self.clone();
        new_map.normalize_svs();
        return new_map;
    }

    fn with_denormalized_svs(&mut self) -> Map {
        // returns a new map with denormalized SVs
        let mut new_map = self.clone();
        new_map.denormalize_svs();
        return new_map;
    }

    fn normalize_svs(&mut self) {
        // converts SVs to the normalized format (BPM does not affect SV)
        // must be done after sorting timing points and SVs
        if self.bpm_does_not_affect_scroll_velocity {
            return;
        }

        let base_bpm = self.get_common_bpm();

        let mut normalized_scroll_velocities: Vec<ControlPoint> = Vec::new();

        let mut current_bpm = self.timing_points[0].bpm;
        let mut current_sv_index = 0;
        let mut current_sv_start_time: Option<f32> = None;
        let mut current_sv_multiplier = 1.0;
        let mut current_adjusted_sv_multiplier: Option<f32> = None;
        let mut initial_sv_multiplier: Option<f32> = None;

        for (i, timing_point) in self.timing_points.iter().enumerate() {
            let next_timing_point_has_same_timestamp = (i + 1 < self.timing_points.len())
                && (self.timing_points[i + 1].start_time == timing_point.start_time);
            loop {
                if current_sv_index >= self.scroll_velocities.len() {
                    break;
                }

                let sv = &self.scroll_velocities[current_sv_index];

                if sv.start_time > timing_point.start_time {
                    break;
                }

                // if there are more timing points on this timestamp, the SV only applies on the
                // very last one, so skip it for now
                if next_timing_point_has_same_timestamp && sv.start_time == timing_point.start_time {
                    break;
                }

                if sv.start_time < timing_point.start_time {
                    // the way that osu! handles infinite BPM is more akin to "arbitrarily large SV"
                    // we chose the smallest power of two greater than MAX_MULTIPLIER
                    // from SVFactor to make DenormalizeSVs more accurate
                    let multiplier = if current_bpm.is_infinite() {
                        128.0
                    } else {
                        sv.multiplier * (current_bpm / base_bpm)
                    };

                    if current_adjusted_sv_multiplier.is_none() {
                        current_adjusted_sv_multiplier = Some(multiplier);
                        initial_sv_multiplier = Some(multiplier);
                    }

                    if multiplier != current_adjusted_sv_multiplier.unwrap() {
                        normalized_scroll_velocities.push(ControlPoint {
                            start_time: sv.start_time,
                            multiplier: multiplier,
                        });

                        current_adjusted_sv_multiplier = Some(multiplier);
                    }
                }

                current_sv_start_time = Some(sv.start_time);
                current_sv_multiplier = sv.multiplier;
                current_sv_index += 1;
            }

            // timing points reset the previous SV multiplier
            if current_sv_start_time.is_none()
                || current_sv_start_time.unwrap() < timing_point.start_time {
                current_sv_multiplier = 1.0;
            }

            current_bpm = timing_point.bpm;

            let multiplier_too = if current_bpm.is_infinite() {
                128.0
            } else {
                current_sv_multiplier * (current_bpm / base_bpm)
            };

            if current_adjusted_sv_multiplier.is_none() {
                current_adjusted_sv_multiplier = Some(multiplier_too);
                initial_sv_multiplier = Some(multiplier_too);
            }

            if multiplier_too == current_adjusted_sv_multiplier.unwrap() {
                continue;
            }

            normalized_scroll_velocities.push(ControlPoint {
                start_time: timing_point.start_time,
                multiplier: multiplier_too,
            });

            current_adjusted_sv_multiplier = Some(multiplier_too);
        }

        for i in current_sv_index..self.scroll_velocities.len() {
            let sv = &self.scroll_velocities[i];
            let multiplier = if current_bpm.is_infinite() {
                128.0
            } else {
                sv.multiplier * (current_bpm / base_bpm)
            };

            debug_assert!(
                current_adjusted_sv_multiplier.is_some(),
                "current_adjusted_sv_multiplier should not be None"
            );

            if multiplier == current_adjusted_sv_multiplier.unwrap() {
                continue;
            }

            normalized_scroll_velocities.push(ControlPoint {
                start_time: sv.start_time,
                multiplier: multiplier,
            });

            current_adjusted_sv_multiplier = Some(multiplier);
        }

        self.bpm_does_not_affect_scroll_velocity = true;
        normalized_scroll_velocities
            .sort_by(|a, b| a.start_time.partial_cmp(&b.start_time).unwrap());
        self.scroll_velocities = normalized_scroll_velocities;
        self.initial_scroll_velocity = Some(initial_sv_multiplier.unwrap_or(1.0));
    }

    fn denormalize_svs(&mut self) {
        // converts SVs to the denormalized format (BPM affects SV)
        // must be done after sorting timing points and SVs
        if !self.bpm_does_not_affect_scroll_velocity {
            // already denormalized
            return;
        }

        let base_bpm = self.get_common_bpm();

        let mut denormalized_scroll_velocities: Vec<ControlPoint> = Vec::new();
        let mut current_bpm = self.timing_points[0].bpm;

        // for the purposes of this conversion, 0 and +inf should be handled like max value
        if current_bpm == 0.0 || current_bpm == f32::INFINITY {
            current_bpm = f32::MAX;
        }

        let mut current_sv_index = 0;
        let mut current_sv_multiplier = self.initial_scroll_velocity.unwrap_or(1.0);
        let mut current_adjusted_sv_multiplier: Option<f32> = None;

        for (i, timing_point) in self.timing_points.iter().enumerate() {
            loop {
                if current_sv_index >= self.scroll_velocities.len() {
                    break;
                }

                let sv = &self.scroll_velocities[current_sv_index];

                if sv.start_time > timing_point.start_time {
                    break;
                }

                if sv.start_time < timing_point.start_time {
                    // the way that osu! handles infinite BPM is more akin to "arbitrarily large SV"
                    // we chose the greatest power of two less than MIN_MULTIPLIER
                    // from SVFactor to make NormalizeSVs more accurate
                    let multiplier = if current_bpm.is_infinite() {
                        1.0 / 128.0
                    } else {
                        sv.multiplier / (current_bpm / base_bpm)
                    };

                    if current_adjusted_sv_multiplier.is_none()
                        || multiplier != current_adjusted_sv_multiplier.unwrap()
                    {
                        if current_adjusted_sv_multiplier.is_none()
                            && sv.multiplier != self.initial_scroll_velocity.unwrap()
                        {
                            // insert an SV 1 ms earlier to simulate the initial scroll speed multiplier
                            if current_bpm.is_infinite() {
                                denormalized_scroll_velocities.push(ControlPoint {
                                    start_time: sv.start_time - 1.0,
                                    multiplier: 1.0 / 128.0,
                                });
                            } else {
                                denormalized_scroll_velocities.push(ControlPoint {
                                    start_time: sv.start_time - 1.0,
                                    multiplier: self.initial_scroll_velocity.unwrap() / (current_bpm / base_bpm),
                                });
                            }
                        }

                        denormalized_scroll_velocities.push(ControlPoint {
                            start_time: sv.start_time,
                            multiplier: multiplier,
                        });

                        current_adjusted_sv_multiplier = Some(multiplier);
                    }
                }

                current_sv_multiplier = sv.multiplier;
                current_sv_index += 1;
            }

            current_bpm = timing_point.bpm;

            // for the purposes of this conversion, 0 and +inf should be handled like max value
            if current_bpm == 0.0 || current_bpm == f32::INFINITY {
                current_bpm = f32::MAX;
            }

            if current_adjusted_sv_multiplier.is_none()
                && current_sv_multiplier != self.initial_scroll_velocity.unwrap()
            {
                // insert an SV 1 ms earlier to simulate the initial scroll speed multiplier
                if current_bpm.is_infinite() {
                    denormalized_scroll_velocities.push(ControlPoint {
                        start_time: timing_point.start_time - 1.0,
                        multiplier: 1.0 / 128.0,
                    });
                } else {
                    denormalized_scroll_velocities.push(ControlPoint {
                        start_time: timing_point.start_time - 1.0,
                        multiplier: self.initial_scroll_velocity.unwrap() / (current_bpm / base_bpm),
                    });
                }
            }

            // timing points reset the SV multiplier
            current_adjusted_sv_multiplier = Some(1.0);

            // skip over multiple timing points at the same timestamp
            if (i + 1 < self.timing_points.len())
                && (self.timing_points[i + 1].start_time == timing_point.start_time)
            {
                continue;
            }

            let multiplier_too = if current_bpm.is_infinite() {
                1.0 / 128.0
            } else {
                current_sv_multiplier / (current_bpm / base_bpm)
            };

            if multiplier_too == current_adjusted_sv_multiplier.unwrap() {
                continue;
            }

            denormalized_scroll_velocities.push(ControlPoint {
                start_time: timing_point.start_time,
                multiplier: multiplier_too,
            });

            current_adjusted_sv_multiplier = Some(multiplier_too);
        }

        for i in current_sv_index..self.scroll_velocities.len() {
            let sv = &self.scroll_velocities[i];
            let multiplier = if current_bpm.is_infinite() {
                1.0 / 128.0
            } else {
                sv.multiplier / (current_bpm / base_bpm)
            };

            debug_assert!(
                current_adjusted_sv_multiplier.is_some(),
                "current_adjusted_sv_multiplier should not be None"
            );

            if multiplier == current_adjusted_sv_multiplier.unwrap() {
                continue;
            }

            denormalized_scroll_velocities.push(ControlPoint {
                start_time: sv.start_time,
                multiplier: multiplier,
            });

            current_adjusted_sv_multiplier = Some(multiplier);
        }

        self.initial_scroll_velocity = Some(0.0);
        self.bpm_does_not_affect_scroll_velocity = false;
        denormalized_scroll_velocities
            .sort_by(|a, b| a.start_time.partial_cmp(&b.start_time).unwrap());
        self.scroll_velocities = denormalized_scroll_velocities;
    }
}

trait HasStartTime {
    // for objects with a start time
    fn start_time(&self) -> f32;
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "PascalCase")]
struct TimingPoint {
    // sets values until next timing point
    // used for bpm change, or affine
    #[serde(default)]
    start_time: f32, // start time (ms)
    bpm: f32,
    time_signature: Option<TimeSignature>,
    #[serde(default)]
    hidden: bool, // show timing lines
}

impl HasStartTime for TimingPoint {
    fn start_time(&self) -> f32 {
        self.start_time
    }
}

impl TimingPoint {
    fn milliseconds_per_beat(&self) -> f32 {
        if self.bpm != 0.0 {
            60000.0 / self.bpm
        } else {
            0.0
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "PascalCase")]
struct ControlPoint {
    // represents either an SV or SSF point
    #[serde(default)]
    start_time: f32,
    #[serde(default)]
    multiplier: f32,
}

impl HasStartTime for ControlPoint {
    fn start_time(&self) -> f32 {
        self.start_time
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "PascalCase")]
struct HitObject {
    // a note
    #[serde(default)]
    start_time: f32,
    lane: i32,
    end_time: Option<f32>,
    key_sounds: Vec<KeySound>, // key sounds to play when this object is hit
    #[serde(default)]
    #[serde(deserialize_with = "deserialize_hit_sounds")]
    hit_sound: Vec<HitSounds>,
    timing_group: Option<String>,
}

impl HasStartTime for HitObject {
    fn start_time(&self) -> f32 {
        self.start_time
    }
}

impl HitObject {
    fn is_long_note(&self) -> bool {
        // if end_time is >0 it is a long note
        if let Some(end_time) = self.end_time {
            return end_time > 0.0;
        }
        false
    }

    fn get_end_time(&self) -> f32 {
        // get end time of the note
        if let Some(end_time) = self.end_time {
            return end_time;
        }
        return self.start_time;
    }

    fn get_timing_point<'a>(&self, timing_points: &'a [TimingPoint]) -> Option<&'a TimingPoint> {
        // gets the timing point this object is in range of
        if timing_points.is_empty() {
            return None;
        }

        Some(at_time(timing_points, self.start_time()).unwrap_or(&timing_points[0]))
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "PascalCase")]
struct KeySound {
    sample: i32, // the one-based index of the sound sample in the CustomAudioSamples array
    volume: i32, // the volume of the sound sample (defaults to 100)
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "PascalCase")]
struct TimingGroup {
    // group of hitobjects with seperate effects
    initial_scroll_velocity: Option<f32>,
    #[serde(default)]
    scroll_velocities: Vec<ControlPoint>,
    #[serde(default)]
    scroll_speed_factors: Vec<ControlPoint>,
    color_rgb: Option<String>,
}

impl TimingGroup {
    fn get_scroll_velocity_at(&self, time: f32) -> Option<&ControlPoint> {
        at_time(&self.scroll_velocities, time)
    }

    fn get_scroll_speed_factor_at(&self, time: f32) -> Option<&ControlPoint> {
        at_time(&self.scroll_speed_factors, time)
    }
}

impl Default for TimingGroup {
    fn default() -> Self {
        Self {
            initial_scroll_velocity: None,
            scroll_velocities: Vec::new(),
            scroll_speed_factors: Vec::new(),
            color_rgb: None,
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "PascalCase")]
enum TimeSignature {
    Quadruple = 4,
    Triple = 3,
}

// bitflags! {
//     #[derive(Serialize, Deserialize, Debug, Clone)]
//     struct HitSounds: u8 {
//         const NORMAL  = 0b0001;
//         const WHISTLE = 0b0010;
//         const FINISH  = 0b0100;
//         const CLAP    = 0b1000;
//     }
// }

#[derive(Serialize, Deserialize, Debug, Clone)]
enum HitSounds {
    Normal = 0b0001,
    Whistle = 0b0010,
    Finish = 0b0100,
    Clap = 0b1000,
}

impl Default for HitSounds {
    fn default() -> Self {
        // HitSounds::empty()
        HitSounds::Normal
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "PascalCase")]
struct SoundEffect {
    // sound effect for the map
    #[serde(default)]
    start_time: f32, // the time at which to play the sound sample
    #[serde(default)]
    sample: i32, // the one-based index of the sound sample in the CustomAudioSamples array
    #[serde(default)]
    volume: i32, // the volume of the sound sample (defaults to 100)
    #[serde(default)]
    sound_effect: String,
}

impl HasStartTime for SoundEffect {
    fn start_time(&self) -> f32 {
        self.start_time
    }
}

fn deserialize_hit_sounds<'de, D>(deserializer: D) -> Result<Vec<HitSounds>, D::Error>
where
    D: Deserializer<'de>,
{
    use serde::de::Error;
    let s: Option<String> = Option::deserialize(deserializer)?;
    if let Some(s) = s {
        s.split(',')
            .map(|name| match name.trim() {
                "Normal" => Ok(HitSounds::Normal),
                "Whistle" => Ok(HitSounds::Whistle),
                "Finish" => Ok(HitSounds::Finish),
                "Clap" => Ok(HitSounds::Clap),
                "" => Err(Error::custom("Empty hit sound")),
                other => Err(Error::custom(format!("Unknown hit sound: '{}'", other))),
            })
            .collect()
    } else {
        Ok(vec![]) // empty if not present
    }
}

fn index_at_time<T: HasStartTime>(list: &[T], time: f32) -> Option<usize> {
    // binary search for the index of the first element with start_time > time
    if list.is_empty() || list[0].start_time() > time {
        return None;
    }

    let mut left = 0;
    let mut right = list.len() - 1;

    while left <= right {
        let mid = left + (right - left) / 2;
        if list[mid].start_time() <= time {
            left = mid + 1;
        } else {
            if mid == 0 {
                break;
            }
            right = mid - 1;
        }
    }

    Some(right)
}

// finds the first item in the list with a start time equal to the given time
fn at_time<T: HasStartTime>(list: &[T], time: f32) -> Option<&T> {
    index_at_time(list, time).map(|i| &list[i])
}

// finds the first item in the list with a start time less than or equal to the given time
fn at_time_before<T: HasStartTime>(list: &[T], time: f32) -> Option<&T> {
    let tiny_offset = f32::EPSILON;
    at_time(list, time - tiny_offset)
}

// finds the first item in the list with a start time greater than the given time
fn at_time_after<T: HasStartTime>(list: &[T], time: f32) -> Option<&T> {
    list.iter().find(|item| item.start_time() > time)
}

// sorts a vector of items by their start time
fn sort_by_start_time<T: HasStartTime>(items: &mut Vec<T>) {
    items.sort_by(|a, b| a.start_time().partial_cmp(&b.start_time()).unwrap());
}

fn get_snap_color(beat: f32) -> Color {
    // gets the color of the snap for a given beat
    // 1, 2, 3, 4, 6, 8, 12, 16, 48

    // get just the decimal of the beat number
    let mut fraction = (beat.fract() + 1.0) % 1.0;
    // handle cases near 1.0 being equivalent to 0.0 for the next beat
    if (1.0 - fraction).abs() < SNAP_EPSILON {
        fraction = 0.0;
    }

    for snap in SNAP_TYPES {
        for n in 0..snap.divisor {
            let snap_pos = n as f32 / snap.divisor as f32;
            if (fraction - snap_pos).abs() < SNAP_EPSILON {
                return snap.color;
            }
        }
    }

    // fallback, last snap (48th)
    // return SNAP_TYPES[SNAP_TYPES.len() - 1].color;
    return WHITE;
}

struct FrameState<'a> {
    pub song_time_ms: f32,
    pub map: &'a Map,
    pub playback_speed: f32,
    // add more fields if needed (input, debug, options)
}

trait Draw {
    fn draw_rectangle(&mut self, x: f32, y: f32, w: f32, h: f32, color: macroquad::color::Color);
    fn draw_line(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, thickness: f32, color: macroquad::color::Color);
    fn draw_text(&mut self, text: &str, x: f32, y: f32, size: f32, color: macroquad::color::Color);
    fn screen_height(&self) -> f32;
    fn screen_width(&self) -> f32;
}

struct MacroquadDraw;

impl Draw for MacroquadDraw {
    fn draw_rectangle(&mut self, x: f32, y: f32, w: f32, h: f32, color: macroquad::color::Color) {
        macroquad::shapes::draw_rectangle(x, y, w, h, color);
    }
    fn draw_line(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, thickness: f32, color: macroquad::color::Color) {
        macroquad::shapes::draw_line(x1, y1, x2, y2, thickness, color);
    }
    fn draw_text(&mut self, text: &str, x: f32, y: f32, size: f32, color: macroquad::color::Color) {
        macroquad::text::draw_text(text, x, y, size, color);
    }
    fn screen_height(&self) -> f32 {
        macroquad::window::screen_height()
    }
    fn screen_width(&self) -> f32 {
        macroquad::window::screen_width()
    }
}

fn render_frame(state: &FrameState, draw: &mut impl Draw) {
    // calculates the positions of all objects and renders the current frame given the framestate

    const LANE_WIDTH_PX: f32 = 120.0; // width of each lane/column
    const NOTE_WIDTH_PX: f32 = 120.0; // how wide the notes are
    const NOTE_HEIGHT_PX: f32 = 30.0; // how tall the notes are
    const HIT_LINE_Y_POS_PX: f32 = 200.0; // how high the hit line is
    const REF_SCROLL_SPEED_PX_PER_SECOND: f32 = 2200.0; // how fast notes fall
    const LOOK_AHEAD_TIME_MS: f32 = 2000.0; // how far ahead (ms) to start showing notes
    const MIRROR: bool = true; // mirror lanes
    const RATE_AFFECTS_SCROLL_SPEED: bool = false; // whether the rate multiplies the scroll speed

    // 0.5x - 2.0x
    let rate = state.playback_speed;

    let window_height = draw.screen_height();
    let window_width = draw.screen_width();
    let scroll_speed_px_per_second = REF_SCROLL_SPEED_PX_PER_SECOND * (window_height / 1400.0);

    // calculated scroll speed (still in px/s)
    let scroll_speed = if RATE_AFFECTS_SCROLL_SPEED {
        scroll_speed_px_per_second * rate
    } else {
        scroll_speed_px_per_second
    };

    let distance_top = window_height - HIT_LINE_Y_POS_PX - 0.0;
    let time_until_hit_top_ms = (distance_top / scroll_speed) * 1000.0 * rate;
    let time_at_top_of_screen_ms = state.song_time_ms + time_until_hit_top_ms;

    let distance_bot = window_height - HIT_LINE_Y_POS_PX - window_height;
    let time_until_hit_bot_ms = (distance_bot / scroll_speed) * 1000.0 * rate;
    let time_at_bottom_of_screen_ms = state.song_time_ms + time_until_hit_bot_ms;

    // background
    draw.draw_rectangle(
        0.0,
        0.0,
        window_width,
        window_height,
        macroquad::color::BLACK,
    );

    let num_lanes = state.map.get_key_count(false);
    let playfield_width = num_lanes as f32 * LANE_WIDTH_PX;
    let playfield_x = (window_width - playfield_width) / 2.0;

    // lanes
    for i in 0..=num_lanes {
        let x = playfield_x + (i as f32 * LANE_WIDTH_PX);
        draw.draw_line(
            x,
            0.0,
            x,
            window_height,
            1.0, 
            macroquad::color::DARKGRAY
        );
    }

    // receptors
    draw.draw_line(
        playfield_x,
        window_height - HIT_LINE_Y_POS_PX,
        playfield_x + (num_lanes as f32 * LANE_WIDTH_PX),
        draw.screen_height() - HIT_LINE_Y_POS_PX,
        2.0,
        macroquad::color::GRAY,
    );

    let line_color = GRAY;
    let line_thickness = 1.0;

    // timing lines
    for (i, tp) in state.map.timing_points.iter().enumerate() {
        if tp.hidden {
            continue;
        }

        let mspb = tp.milliseconds_per_beat();
        if mspb <= 0.0 {
            continue;
        }

        let tp_start_actual = tp.start_time;
        let tp_end_actual = if i + 1 < state.map.timing_points.len() {
            state.map.timing_points[i + 1].start_time
        } else {
            state.map
                .length()
                .max(state.song_time_ms + LOOK_AHEAD_TIME_MS) // extend to map length or a bit past current view
        };

        // determine the first beat index (float) that could be visible at or after the top of the screen
        let mut current_beat_index_within_tp = 0.0; // beat index relative to tp_start_actual
        // if time_at_top_of_screen_ms > tp_start_actual {
        //     current_beat_index_within_tp = ((time_at_top_of_screen_ms - tp_start_actual) / mspb).floor();
        //     if current_beat_index_within_tp < 0.0 { current_beat_index_within_tp = 0.0; }
        // }

        let mut current_beat_render_time = tp_start_actual + current_beat_index_within_tp * mspb;
        let time_signature_beats = match tp.time_signature {
            Some(TimeSignature::Quadruple) => 4.0,
            Some(TimeSignature::Triple) => 3.0,
            None => 4.0,
        };
        // let mut beat_count_for_measure_calc = current_beat_index_within_tp; // this is beat *within this TP*

        // a more robust way for measure lines involves calculating total beats from song start to tp_start_actual
        // for now, we'll use a simpler method: a line is a measure line if it's the first beat of the TP,
        // or if current_beat_index_within_tp is a multiple of time_signature_beats
        // this isn't perfect if TPs don't align with measures, but it's a start

        let mut first_line_in_tp_drawn = false;

        // while current_beat_render_time < tp_end_actual && current_beat_render_time < time_at_bottom_of_screen_ms {

            // only draw if this beat line is also at/after the top of the screen
            if current_beat_render_time <= time_at_top_of_screen_ms - SNAP_EPSILON {
                // add epsilon for safety

                // to account for rate
                let effective_time_until_beat_ms = (current_beat_render_time - state.song_time_ms) / rate;
                let distance_from_receptor_px = (effective_time_until_beat_ms / 1000.0) * scroll_speed;
                let line_y = (window_height - HIT_LINE_Y_POS_PX) - distance_from_receptor_px;

                if line_y < window_height && line_y > 0.0 {
                    draw.draw_line(
                        playfield_x,
                        line_y,
                        playfield_x + (num_lanes as f32 * LANE_WIDTH_PX),
                        line_y,
                        line_thickness,
                        line_color,
                    );
                }
                first_line_in_tp_drawn = true;
            }

            current_beat_index_within_tp += 1.0;
            current_beat_render_time = tp_start_actual + current_beat_index_within_tp * mspb;
        // }
    }

    // notes
    for note in &state.map.hit_objects {
        let time_until_hit_ms = note.start_time - state.song_time_ms;

        //  how far ahead notes should be visible, adjusted by current playback speed
        let look_ahead = LOOK_AHEAD_TIME_MS;
        //  how far past notes should be visible, adjusted by current playback speed
        let look_behind = (window_height / scroll_speed) * 1000.0;

        if time_until_hit_ms < look_ahead && time_until_hit_ms > -look_behind {
            let effective_time_until_hit_ms = (note.start_time - state.song_time_ms) / rate;
            let distance_from_receptor_px = (effective_time_until_hit_ms / 1000.0) * scroll_speed_px_per_second;

            // y position of exact hitpoint
            let note_y = (window_height - HIT_LINE_Y_POS_PX) - distance_from_receptor_px;

            // calculate x position based on lane (1-indexed in quaver)
            // adjust lane to be 0-indexed for calculation
            let lane_index = if MIRROR {
                num_lanes - note.lane
            } else {
                note.lane - 1
            };
            let note_x = playfield_x
                + (lane_index as f32 * LANE_WIDTH_PX)
                + (LANE_WIDTH_PX / 2.0) // center in lane
                - (NOTE_WIDTH_PX / 2.0);

            // if within screen bounds
            if note_y < window_height && note_y + NOTE_HEIGHT_PX > 0.0 {
                // snap colors
                let active_tp = state.map.get_timing_point_at(note.start_time).unwrap();
                let time_since_tp = note.start_time - active_tp.start_time;
                let beat = time_since_tp / active_tp.milliseconds_per_beat();

                // gray if past receptors
                let color = if distance_from_receptor_px > 0.0 {
                    get_snap_color(beat)
                } else {
                    macroquad::color::DARKGRAY
                };

                draw.draw_rectangle(
                    note_x,
                    note_y - NOTE_HEIGHT_PX,
                    NOTE_WIDTH_PX,
                    NOTE_HEIGHT_PX,
                    color,
                );
            }
        }
    }
}

fn window_conf() -> Conf {
    Conf {
        window_title: "VSRG Renderer".to_string(),
        window_width: 1000,
        window_height: 1200,
        ..Default::default()
    }
}

#[macroquad::main(window_conf)]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // --- audio setup ---
    let mut audio_manager = AudioManager::new().map_err(|e| {
        eprintln!("Critical audio error on init: {}", e);
        std::io::Error::new(std::io::ErrorKind::Other, e)
    })?;

    // --- map loading ---
    let project_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let map_folder_path = project_dir.join("songs/emme");
    let map_file_name_option = fs::read_dir(&map_folder_path)
        .map_err(|e| format!("Failed to read map directory {:?}: {}", map_folder_path, e))?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .find(|path| path.extension().map_or(false, |ext| ext == "qua"))
        .and_then(|path| path.to_str().map(|s| s.to_string()));

    let map_file_name = match map_file_name_option {
        Some(name) => name,
        None => {
            let err_msg = format!(
                "Error: No .qua file found in directory {:?}",
                map_folder_path
            );
            eprintln!("{}", err_msg);
            loop {
                clear_background(DARKGRAY);
                draw_text(&err_msg, 20.0, 20.0, 20.0, RED);
                next_frame().await;
            }
        }
    };

    println!("Loading map: {}", map_file_name);
    let qua_file_content = fs::read_to_string(&map_file_name)
        .map_err(|e| format!("Failed to read map file '{}': {}", map_file_name, e))?;

    let mut map: Map = serde_yaml::from_str(&qua_file_content)
        .map_err(|e| format!("Failed to parse map data from '{}': {}", map_file_name, e))?;

    map.sort();

    if let Err(errors) = map.validate() {
        for error in errors {
            println!("Validation error: {}", error);
        }
    }

    // set audio path in audio manager
    if let Some(audio_filename_str) = &map.audio_file {
        let map_dir = Path::new(&map_file_name)
            .parent()
            .unwrap_or_else(|| Path::new(""));
        let current_audio_path = map_dir.join(audio_filename_str);
        audio_manager.set_audio_path(Some(current_audio_path));
    } else {
        audio_manager.set_audio_path(None);
    }

    println!(
        "Map loaded successfully: {} Hit Objects, {} Timing Points, {} SVs",
        map.hit_objects.len(),
        map.timing_points.len(),
        map.scroll_velocities.len()
    );

    // this is the visual play state, audio is handled by audio_manager
    let mut is_playing_visuals = false;

    let mut json_output_file = File::create("output.json")?;
    let json_string = serde_json::to_string_pretty(&map)?;
    write!(json_output_file, "{}", json_string)?;
    println!("Parsed map data written to output.json");

    // main render loop
    loop {
        // --- input for play/pause ---
        if is_key_pressed(KeyCode::Space) {
            is_playing_visuals = !is_playing_visuals;
            if is_playing_visuals {
                audio_manager.play();
            } else {
                audio_manager.pause();
            }
        }
        if is_key_pressed(KeyCode::R) {
            is_playing_visuals = true;
            audio_manager.restart();
            audio_manager.play();
        }
        if is_key_pressed(KeyCode::Up) {
            let new_vol = (audio_manager.get_volume() + 0.05).min(1.5);
            audio_manager.set_volume(new_vol);
        }
        if is_key_pressed(KeyCode::Down) {
            let new_vol = (audio_manager.get_volume() - 0.05).max(0.0);
            audio_manager.set_volume(new_vol);
        }
        if is_key_pressed(KeyCode::Right) {
            let new_speed = (audio_manager.get_speed() + 0.1).min(2.0);
            audio_manager.set_speed(new_speed);
        }
        if is_key_pressed(KeyCode::Left) {
            let new_speed = (audio_manager.get_speed() - 0.1).max(0.5);
            audio_manager.set_speed(new_speed);
        }

        let song_time_ms = audio_manager.get_current_song_time_ms();

        let mut macroquad_draw = MacroquadDraw;
        let frame_state = FrameState {
            song_time_ms,
            map: &map,
            playback_speed: audio_manager.get_speed(),
        };

        render_frame(&frame_state, &mut macroquad_draw);

        // draw ui / debug info
        let mut y_offset = 20.0;
        let line_height = 20.0;

        let total_duration_str = match audio_manager.get_total_duration_ms() {
            Some(d) => format!("{:.2}s", d / 1000.0),
            None => "N/A".to_string(),
        };
        draw_text(
            &format!(
                "Time: {:.2}s / {}",
                song_time_ms / 1000.0,
                total_duration_str
            ),
            10.0,
            y_offset,
            20.0,
            WHITE,
        );
        y_offset += line_height;

        let visual_state_text = if is_playing_visuals {
            "Playing"
        } else {
            "Paused"
        };
        let audio_actual_state_text = if audio_manager.is_playing() {
            "Audio engine playing"
        } else if audio_manager.sink.as_ref().map_or(false, |s| s.is_paused()) {
            "Audio engine paused"
        } else {
            "Audio engine stopped/empty"
        };
        draw_text(
            &format!(
                "Visuals: {} | Audio: {} (space, r)",
                visual_state_text, audio_actual_state_text
            ),
            10.0,
            y_offset,
            20.0,
            WHITE,
        );
        y_offset += line_height;

        draw_text(
            &format!(
                "Volume: {:.2} (up/down) | Speed: {:.1}x (left/right)",
                audio_manager.get_volume(),
                audio_manager.get_speed()
            ),
            10.0,
            y_offset,
            20.0,
            WHITE,
        );
        y_offset += line_height;


        if let (Some(title), Some(artist), Some(difficulty)) = (&map.title, &map.artist, &map.difficulty_name) {
            draw_text(
                &format!("Map: {} - {} [{}]", title, artist, difficulty),
                10.0,
                y_offset,
                20.0,
                LIGHTGRAY,
            );
            y_offset += line_height;
        }
        draw_text(&format!("FPS: {}", get_fps()), 10.0, y_offset, 20.0, WHITE);
        y_offset += line_height;

        if let Some(err_msg) = audio_manager.get_error() {
            draw_text(
                &format!("Audio status: {}", err_msg),
                10.0,
                y_offset,
                18.0,
                YELLOW,
            );
        } else if audio_manager.audio_source_path.is_none() && map.audio_file.is_some() {
            draw_text(
                &format!(
                    "Audio status: no path set for '{}'",
                    map.audio_file.as_ref().unwrap()
                ),
                10.0,
                y_offset,
                18.0,
                YELLOW,
            );
        }

        next_frame().await;
    }
}
