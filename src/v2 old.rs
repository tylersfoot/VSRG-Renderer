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
use std::cmp::{min, max};

const DEFAULT_TIMING_GROUP_ID: &str = "$Default";
const GLOBAL_TIMING_GROUP_ID: &str = "$Global";
const INITIAL_AUDIO_VOLUME: f32 = 0.05;
const INITIAL_AUDIO_RATE: f32 = 1.0;
const SNAP_EPSILON: f32 = 0.01; // tolerance for float comparisons
const TRACK_ROUNDING: f32 = 1.0; // rounding for track positions, for int/float conversion

struct AudioManager {
    _stream: OutputStream,
    stream_handle: OutputStreamHandle,
    sink: Option<Sink>,
    audio_source_path: Option<PathBuf>,
    current_error: Option<String>,

    // timing related fields
    playback_start_instant: Option<Instant>, // when current play segment started
    accumulated_play_time_ms: f64,           // total time audio has played across pauses
    is_audio_engine_paused: bool,            // to reflect actual sink state

    length: Option<f64>, // length of audio
    rate: f32,           // playback rate
    is_playing: bool,
    is_paused: bool,
    is_stopped: bool,
    time: f64,
    position: f64,
    volume: f32,
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
        initial_sink.set_speed(INITIAL_AUDIO_RATE);
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
            length: None,
            rate: INITIAL_AUDIO_RATE,
            is_playing: false,
            is_paused: false,
            is_stopped: true,
            time: 0.0,
            position: 0.0,
            volume: INITIAL_AUDIO_VOLUME,
        })
    }

    pub fn set_audio_path(&mut self, path: Option<PathBuf>) {
        self.audio_source_path = path;
        self.current_error = None;
        self.length = None; // reset duration when path changes

        if self.audio_source_path.is_none() {
            self.current_error = Some("No audio file specified in map.".to_string());
        } else {
            if let Some(p) = &self.audio_source_path {
                match File::open(p) {
                    Ok(file_handle) => {
                        match Decoder::new(BufReader::new(file_handle)) {
                            Ok(decoder) => {
                                if let Some(duration) = decoder.total_duration() {
                                    self.length = Some(duration.as_millis() as f64);
                                }
                                println!("Audio path set and verified decodable: {:?}, Duration: {:?} ms", p.display(), self.length);
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
                            if self.length.is_none() {
                                if let Some(duration) = source.total_duration() {
                                    self.length = Some(duration.as_millis() as f64);
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
                    self.accumulated_play_time_ms += start_instant.elapsed().as_millis() as f64;
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
                    new_sink.set_volume(self.volume);
                    new_sink.set_speed(self.rate);
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

    pub fn get_current_song_time_ms(&self) -> f64 {
        let mut current_time = self.accumulated_play_time_ms;
        if !self.is_audio_engine_paused {
            if let Some(start_instant) = self.playback_start_instant {
                current_time = self.accumulated_play_time_ms
                    + (start_instant.elapsed().as_millis() as f32 * self.rate) as f64;
            }
        }
        // clamp time to total duration if available
        if let Some(total_duration) = self.length {
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

    pub fn get_total_duration_ms(&self) -> Option<f64> {
        self.length
    }

    pub fn set_volume(&mut self, volume: f32) {
        self.volume = volume.clamp(0.0, 1.5); // clamp volume
        if let Some(s) = self.sink.as_mut() {
            s.set_volume(self.volume);
        }
        println!("Audiomanager: Volume set to {}", self.volume);
    }

    pub fn get_volume(&self) -> f32 {
        self.volume
    }

    pub fn set_rate(&mut self, rate: f32) {
        self.rate = rate.max(0.1); // prevent rate from being too low or zero
        if let Some(s) = self.sink.as_mut() {
            s.set_speed(self.rate);
        }
        if !self.is_audio_engine_paused {
            if let Some(start_instant) = self.playback_start_instant.take() {
                self.accumulated_play_time_ms += start_instant.elapsed().as_millis() as f64;
            }
            self.playback_start_instant = Some(Instant::now());
        }
        println!("Audiomanager: Rate set to {}", self.rate);
    }

    pub fn get_rate(&self) -> f32 {
        self.rate
    }

    pub fn get_error(&self) -> Option<&String> {
        self.current_error.as_ref()
    }
}

struct FieldPositions {
    // positions from top of screen
    receptor_position_y: f32, // receptors position
    hit_position_y: f32, // hit object target position
    hold_hit_position_y: f32, // held hit object target position
    hold_end_hit_position_y: f32, // LN end target position
    timing_line_position_y: f32, // timing line position
    long_note_size_adjustment: f32, // size adjustment for LN so LN end time snaps with start time
}

// snap colors
const BEAT_SNAPS: &[BeatSnap] = &[
    BeatSnap { divisor: 48,  color: Color::new(255.0 / 255.0, 96.0  / 255.0, 96.0  / 255.0, 1.0) }, // 1st (red)
    BeatSnap { divisor: 24,  color: Color::new(61.0  / 255.0, 132.0 / 255.0, 255.0 / 255.0, 1.0) }, // 2nd (blue)
    BeatSnap { divisor: 16,  color: Color::new(178.0 / 255.0, 71.0  / 255.0, 255.0 / 255.0, 1.0) }, // 3rd (purple)
    BeatSnap { divisor: 12,  color: Color::new(255.0 / 255.0, 238.0 / 255.0, 58.0  / 255.0, 1.0) }, // 4th (yellow)
    BeatSnap { divisor: 8,   color: Color::new(255.0 / 255.0, 146.0 / 255.0, 210.0 / 255.0, 1.0) }, // 6th (pink)
    BeatSnap { divisor: 6,   color: Color::new(255.0 / 255.0, 167.0 / 255.0, 61.0  / 255.0, 1.0) }, // 8th (orange)
    BeatSnap { divisor: 4,   color: Color::new(132.0 / 255.0, 255.0 / 255.0, 255.0 / 255.0, 1.0) }, // 12th (cyan)
    BeatSnap { divisor: 3,   color: Color::new(127.0 / 255.0, 255.0 / 255.0, 138.0 / 255.0, 1.0) }, // 16th (green)
    BeatSnap { divisor: 1,   color: Color::new(200.0 / 255.0, 200.0 / 255.0, 200.0 / 255.0, 1.0) }, // 48th (gray) + fallback
];

struct Skin {
    // skin settings
    lane_width: f32, // width of each lane/column
    note_width: f32, // width of each note
    note_height: f32, // height of each note
    hold_note_width: f32, // width of each hold note
    hold_note_height: f32, // height of each hold note
    receptors_y_position: f32, // y position of the receptors/hit line
    scroll_speed: f32, // scroll speed of the notes
    rate_affects_scroll_speed: bool, // whether the rate multiplies the scroll speed
    draw_lanes: bool, // whether to draw the lanes
    wide_timing_lines: bool, // whether to draw timing lines to the sides of the screen
    downscroll: bool, // downscroll (true) or upscroll (false)
    normalize_scroll_velocity_by_rate_percentage: usize, // percentage of scaling applied when changing rates
}

const SKIN: Skin = Skin {
    lane_width: 136.0,
    note_width: 136.0,
    note_height: 36.0,
    hold_note_width: 136.0,
    hold_note_height: 36.0,
    receptors_y_position: 226.0,
    scroll_speed: 200.0, // 20.0 in quaver
    rate_affects_scroll_speed: false,
    draw_lanes: true,
    wide_timing_lines: true,
    downscroll: true,
    normalize_scroll_velocity_by_rate_percentage: 0,

};

struct BeatSnap {
    divisor: u32, // e.g. 4 for 1/4 notes, 6 for 1/6 notes
    color: Color,
}

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
struct Mods {
    mirror: bool, // mirror notes horizontally
    no_sv: bool,  // ignore scroll velocity
    no_ssf: bool, // ignore scroll speed factor
}

impl Mods {
    fn new() -> Self {
        Self {
            mirror: false,
            no_sv: false,
            no_ssf: false,
        }
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
    legacy_ln_rendering: bool,       // whether to use the old LN rendering system (earliest/latest -> start/end)
    #[serde(rename = "BPMDoesNotAffectScrollVelocity")]
    #[serde(default)]
    bpm_does_not_affect_scroll_velocity: bool, // indicates if BPM changes affect SV
    #[serde(default = "one_f32")]
    initial_scroll_velocity: f32,    // the initial SV before the first SV change
    #[serde(default)]
    has_scratch_key: bool,           // +1 scratch key (5/8 key play)
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
    #[serde(default)]
    timing_lines: Vec<TimingLine>,
    #[serde(rename = "SliderVelocities")]
    #[serde(default)]
    scroll_velocities: Vec<ControlPoint>,
    #[serde(default)]
    scroll_speed_factors: Vec<ControlPoint>,
    #[serde(default)]
    hit_objects: Vec<HitObject>,
    #[serde(default)]
    timing_groups: HashMap<String, TimingGroup>,
    #[serde(skip_deserializing)]
    file_path: String, // map file path
    #[serde(skip)]
    time: f64, // current time in the map
    #[serde(skip)]
    rate: f32,
    #[serde(skip)]
    mods: Mods,
    #[serde(skip)]
    length: f64, // length of the map in ms
}

impl Map {
    fn update_current_track_position(&mut self, time: f64) {
        // update the current position of all hit objects
        self.time = time;
        for timing_group in self.timing_groups.values_mut() {
            timing_group.update_current_track_position(time);
        }
    }

    fn update_timing_lines(&mut self) {
        // update the position of all timing lines
        let timing_group = self
            .timing_groups
            .get_mut(DEFAULT_TIMING_GROUP_ID)
            .unwrap();
        let offset = timing_group.current_track_position;
        for timing_line in &mut self.timing_lines {
            timing_line.current_track_position = (offset - timing_line.track_offset) as f32;
        }
    }

    // fn update_hit_objects(&mut self, field_positions: &FieldPositions) {
    //     // update the position of all hit objects
    //     // https://github.com/Quaver/Quaver/blob/develop/Quaver.Shared/Screens/Gameplay/Rulesets/Keys/HitObjects/GameplayHitObjectKeys.cs#L387
    //     for hit_object in &mut self.hit_objects {
    //         let timing_group = self
    //             .timing_groups
    //             .get_mut(hit_object.timing_group.as_ref().unwrap())
    //             .unwrap();
    //         let mut position = 0.0;

    //         // update_long_note_size(map.time);
    //         // if hit object held, logic
    //         position = timing_group.get_hit_object_position(hit_object.hit_position, hit_object.initial_track_position)
    //         // HitObjectSprite.Y = spritePosition;
    //     }
    // }

    fn initialize_scroll_speed(&mut self) {
        // initialize the scroll speed of all timing groups
        for timing_group in self.timing_groups.values_mut() {
            timing_group.initialize_scroll_speed(self.rate);
        }
    }

    fn initialize_hit_objects(&mut self, field_positions: &FieldPositions) {
        // initialize the hit objects
        // https://github.com/Quaver/Quaver/blob/develop/Quaver.Shared/Screens/Gameplay/Rulesets/Keys/HitObjects/GameplayHitObjectKeys.cs#L161
        for hit_object in &mut self.hit_objects {
            let timing_group = self
                .timing_groups
                .get_mut(hit_object.timing_group.as_ref().unwrap())
                .unwrap();

            hit_object.initial_track_position = timing_group.get_position_from_time(
                hit_object.start_time as f64,
                index_at_time(&self.scroll_velocities, hit_object.start_time)
            ) as f32;

            hit_object.hit_position = if hit_object.end_time.is_some() {
                // LN
                field_positions.hold_end_hit_position_y
            } else {
                field_positions.hit_position_y
            };
            hit_object.hold_end_hit_position = field_positions.hold_end_hit_position_y;
        }
    }

    fn update_scroll_speed(&mut self) {
        // updates the scroll speed of all timing groups
        for timing_group in self.timing_groups.values_mut() {
            timing_group.update_scroll_speed(self.rate);
        }
    }

    fn initialize_position_markers(&mut self) {
        // initialize the position markers for all hit objects
        for timing_group in self.timing_groups.values_mut() {
            timing_group.initialize_position_markers();
        }
    }

    fn initialize_timing_lines(&mut self) {
        // initialize the timing lines
        self.timing_lines.clear();
        for tp_index in 0..self.timing_points.len() {
            if self.timing_points[tp_index].hidden {
                continue;
            }

            let mut end_target: f32; // end time of the timing line

            // set the end to 1ms before the next timing point to avoid possible timing line overlap
            let end_target = if tp_index + 1 < self.timing_points.len() {
                self.timing_points[tp_index + 1].start_time - 1.0
            } else {
                self.length as f32
            };


            let signature = self.timing_points[tp_index]
                .time_signature
                .as_ref()
                .cloned()
                .unwrap_or(TimeSignature::Quadruple) as u32 as f32;

            // "max possible sane value for timing lines"
            const MAX_BPM: f32 = 9999.0;

            let ms_per_beat = 60000.0 / MAX_BPM.min(self.timing_points[tp_index].bpm.abs());
            let increment = signature * ms_per_beat; // how many ms between measures

            if increment <= 0.0 {
                continue;
            }

            // initialize timing lines between current current timing point and target position
            let mut song_position = self.timing_points[tp_index].start_time;
            while song_position < end_target {
                let offset = self.timing_groups
                    .get(DEFAULT_TIMING_GROUP_ID)
                    .unwrap()
                    .get_position_from_time(
                        song_position as f64,
                        index_at_time(
                            &self.scroll_velocities,
                            song_position
                        )
                    );

                let timing_line = TimingLine {
                    start_time: song_position,
                    track_offset: offset,
                    current_track_position: 0.0,
                };

                self.timing_lines.push(timing_line);

                song_position += increment;
            }
        }
    }

    fn initialize_beat_snaps(&mut self) {
        for hit_object in &mut self.hit_objects {
            let timing_point = at_time(&self.timing_points, hit_object.start_time)
                .unwrap_or(&self.timing_points[0]);

            // get beat length
            let beat_length = 60000.0 / timing_point.bpm;
            let position = hit_object.start_time - timing_point.start_time;

            // calculate note's snap index
            let index = (48.0 * position / beat_length).round() as u32;


            // defualt value; will be overwritten unless
            // not snapped to 1/16 or less, snap to 1/48
            hit_object.snap_index = 8;

            for (i, snap_type) in BEAT_SNAPS.iter().enumerate() {
                if index % snap_type.divisor == 0 {
                    // snap to this color
                    hit_object.snap_index = i;
                    break;
                }
            }
        }
    }

    fn link_default_timing_group(&mut self) {
        // links DefaultScrollGroup to TimingGroups so that
        // TimingGroups[DefaultScrollGroupId] points to that group
        self.timing_groups
            .insert(DEFAULT_TIMING_GROUP_ID.to_string(), TimingGroup::default());

        // copy over initial scroll velocity
        self.timing_groups
            .get_mut(DEFAULT_TIMING_GROUP_ID)
            .unwrap().initial_scroll_velocity = self.initial_scroll_velocity;
        // copy over SVs
        self.timing_groups
            .get_mut(DEFAULT_TIMING_GROUP_ID)
            .unwrap()
            .scroll_velocities = self.scroll_velocities.clone();
        // copy over SSFs
        self.timing_groups
            .get_mut(DEFAULT_TIMING_GROUP_ID)
            .unwrap()
            .scroll_speed_factors = self.scroll_speed_factors.clone();

        // Set every hitobject whose timing group is null to the default group
        for hit_object in &mut self.hit_objects {
            if hit_object.timing_group.is_none() {
                hit_object.timing_group = Some(DEFAULT_TIMING_GROUP_ID.to_string());
            }
        }

        // ! removing from main map for debug
        self.scroll_velocities.clear();
        self.scroll_speed_factors.clear();
    }

    fn get_timing_point_at(&self, time: f32) -> Option<&TimingPoint> {
        // gets the timing point at a particular time in the map
        if self.timing_points.is_empty() {
            return None;
        }

        Some(at_time(self.timing_points.as_slice(), time).unwrap_or(&self.timing_points[0]))
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

    fn sort_timing_points(&mut self) {
        // sort_by_start_time(&mut self.timing_points);
        self.timing_points.sort_by(|a, b| a.start_time().partial_cmp(&b.start_time()).unwrap());
    }

    fn sort(&mut self) {
        self.sort_hit_objects();
        self.sort_timing_points();
        self.sort_scroll_velocities();
        self.sort_scroll_speed_factors();
    }

}

trait HasStartTime {

    // for objects with a start time
    fn start_time(&self) -> f32;
}
#[derive(Serialize, Deserialize, Debug, Clone)]
struct TimingLine {
    #[serde(default)]
    start_time: f32, // time when the timing line reaches the receptor
    #[serde(default)]
    track_offset: i64, // the timing line's y offset from the receptor; target position when track position = 0
    #[serde(default)]
    current_track_position: f32, // track position; >0 = hasnt passed receptors
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
    #[serde(default = "one_f32")]
    multiplier: f32,
    #[serde(skip_deserializing)]
    length: Option<f32>, // none if last point
    #[serde(skip_deserializing)]
    effective_length: Option<f32>, // none if last point
    #[serde(skip_deserializing)]
    position_marker: i64, // cumulative distance from the start of the map
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
    end_time: Option<f32>, // if Some, then its an LN
    lane: i32,
    key_sounds: Vec<KeySound>, // key sounds to play when this object is hit
    #[serde(default)]
    #[serde(deserialize_with = "deserialize_hit_sounds")]
    hit_sound: Vec<HitSounds>,
    timing_group: Option<String>,
    #[serde(default)]
    removed: bool, // if object is out of range
    #[serde(default)]
    snap_index: usize, // index for snap color
    #[serde(default)]
    hit_position: f32, // general position for hitting, calculated from hit body height and hit position offset
    #[serde(default)]
    hold_end_hit_position: f32, // position for LN ends
    #[serde(default)]
    current_long_note_body_size: f32, // current size of the LN body
    #[serde(default)]
    scroll_direction: bool, // true for upscroll, false for downscroll
    #[serde(default)]
    initial_track_position: f32, // Y-offset from the origin
    
}

    // public virtual float CurrentLongNoteBodySize => (LatestHeldPosition - EarliestHeldPosition) *
    //     TimingGroupController.ScrollSpeed / HitObjectManagerKeys.TrackRounding;

impl HasStartTime for HitObject {
    fn start_time(&self) -> f32 {
        self.start_time
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
    #[serde(default = "one_f32")]
    initial_scroll_velocity: f32,
    #[serde(default)]
    scroll_velocities: Vec<ControlPoint>,
    #[serde(default)]
    scroll_speed_factors: Vec<ControlPoint>,
    color_rgb: Option<String>,
    // info for playback
    #[serde(skip)]
    current_track_position: i64, // current playback position
    #[serde(skip)]
    current_sv_index: Option<usize>, // index of current SV point
    #[serde(skip)]
    current_ssf_index: Option<usize>, // index of current SSF point
    #[serde(default = "one_f32")]
    current_ssf_factor: f32, // current SSF multiplier
    #[serde(skip)]
    adjusted_scroll_speed: f32,
    #[serde(skip)]
    scroll_speed: f32, // speed at which objects travel across the screen
}

impl TimingGroup {
    fn initialize_position_markers(&mut self) {
        // calculates the timing group's track position at each SV point
        if self.scroll_velocities.is_empty() {
            // no SVs, nothing to set
            return;
        }

        // start with first SV point
        let mut position = (self.scroll_velocities[0].start_time
            * self.initial_scroll_velocity
            * TRACK_ROUNDING) as i64;
        self.scroll_velocities[0].position_marker = position;

        // loop through SV indexes 1 to SVs-1 (rest of SVs)
        for index in 1..self.scroll_velocities.len() {
            let current_sv = &self.scroll_velocities[index];
            let previous_sv = &self.scroll_velocities[index - 1];
            // we are computing up to current SV's point, so we use the previous SV's multiplier
            let multiplier = previous_sv.multiplier;
            // distance between last and current SV, times the previous SV's multiplier
            let distance = (current_sv.start_time - previous_sv.start_time) * multiplier;

            position += (distance * TRACK_ROUNDING) as i64;
            self.scroll_velocities[index].position_marker = position;
        }
    }

    fn initialize_scroll_speed(&mut self, rate: f32) {
        // initialize the scroll speed of this timing group; gets re-called on rate change
        // set adjusted scroll speed
        let speed = SKIN.scroll_speed;
        let rate_scaling = 1.0
            + (rate - 1.0)
            * (SKIN.normalize_scroll_velocity_by_rate_percentage as f32 / 100.0);
        self.adjusted_scroll_speed = (speed * rate_scaling).clamp(50.0, 1000.0);
        
        // set scroll speed
        self.update_scroll_speed(rate);
    }

    fn update_scroll_speed(&mut self, rate: f32) {
        // update scroll speed with ssf factor
        let scaling_factor = 1920.0 / 1366.0; // quaver's scaling
        self.scroll_speed = (self.adjusted_scroll_speed / 10.0)
            / (20.0 * rate)
            * scaling_factor
            * self.current_ssf_factor; // * base_to_virtual_ratio
    }

    fn get_hit_object_position(&self, hit_position: f32, initial_position: f32) -> f32 {
        // calculates the position of a hit object with a position offset
        let scroll_speed = if SKIN.downscroll { 
                -self.scroll_speed 
            } else { 
                self.scroll_speed 
            }; 

        let position =  hit_position + (
            (initial_position - self.current_track_position as f32)
            * scroll_speed 
            / TRACK_ROUNDING
        );

        return position;

    }

    fn get_scroll_speed_factor_from_time(&self, time: f64, mut ssf_index: Option<usize>) -> f32 {
        // gets the SSF multiplier at a time, with linear interpolation
        // uses index for optimization
        if ssf_index.is_none() || self.scroll_speed_factors.is_empty() {
            // before first SSF point, or no SSFs
            return 1.0;
        }

        let index = if ssf_index.unwrap() >= self.scroll_speed_factors.len() {
            // somehow out of bounds, set to last SSF point
            self.scroll_speed_factors.len() - 1
        } else {
            ssf_index.unwrap()
        };

        let ssf = &self.scroll_speed_factors[index];
        if index == self.scroll_speed_factors.len() - 1 {
            // last point, no interpolation
            return ssf.multiplier;
        }
        
        let next_ssf = &self.scroll_speed_factors[index + 1];
        // linear interpolation between current and next SSF
        return lerp(
            ssf.multiplier,
            next_ssf.multiplier,
            ((time as f32 - ssf.start_time) / (next_ssf.start_time - ssf.start_time))
        );
    }

    fn get_position_from_time(&self, time: f64, sv_index: Option<usize>) -> i64 {
        // calculates the timing group's track position with time and SV
        // uses index for optimization
        if sv_index.is_none() || self.scroll_velocities.is_empty() {
            // before first sv point
            return (time as f32 * self.initial_scroll_velocity * TRACK_ROUNDING) as i64;
        }
        let index = if sv_index.unwrap() >= self.scroll_velocities.len() {
            // somehow out of bounds, set to last SV point
            self.scroll_velocities.len() - 1
        } else {
            sv_index.unwrap()
        };

        // grab the track position at the start of the current SV point
        let mut current_position = self.scroll_velocities[index].position_marker;

        // add the distance between now and the start of the current SV point
        current_position += ((time as f32 - self.scroll_velocities[index].start_time)
            * self.scroll_velocities[index].multiplier
            * TRACK_ROUNDING) as i64;
        return current_position;
    }

    fn update_current_track_position(&mut self, time: f64) {
        // updates the current track position of the timing group

        // update SV index
        loop {
            if self.scroll_velocities.is_empty() {
                // there are no SVs, indicate using initial scroll velocity by setting index to None
                self.current_sv_index = None;
                break;
            }

            if self.current_sv_index.is_none() {
                // check if we are past the first SV point yet
                if time > self.scroll_velocities[0].start_time as f64 {
                    // start with first SV
                    self.current_sv_index = Some(0);
                } else {
                    // set index to None to use initial scroll velocity
                    self.current_sv_index = None;
                    break;
                }
            }

            // update the SV index if we aren't at the last SV point and
            // the next SV point is past the current time
            if self.current_sv_index.unwrap() < self.scroll_velocities.len() - 1
                && time as f32 >= self.scroll_velocities[self.current_sv_index.unwrap() + 1].start_time 
            {
                self.current_sv_index = Some(self.current_sv_index.unwrap() + 1);
            } else {
                // SV index is up to date
                break;
            }
        }

        // update SSF index
        loop {
            if self.scroll_speed_factors.is_empty() {
                // there are no SSFs
                self.current_ssf_index = None;
                break;
            }

            if self.current_ssf_index.is_none() {
                // check if we are past the first SSF point yet
                if time > self.scroll_speed_factors[0].start_time as f64 {
                    // start with first SSF
                    self.current_ssf_index = Some(0);
                } else {
                    // not past first SSF point
                    self.current_ssf_index = None;
                    break;
                }
            }

            // update the SSF index if we aren't at the last SSF point and
            // the next SSF point is past the current time
            if self.current_ssf_index.unwrap() < self.scroll_speed_factors.len() - 1
                && time as f32 >= self.scroll_speed_factors[self.current_ssf_index.unwrap() + 1].start_time 
            {
                self.current_ssf_index = Some(self.current_ssf_index.unwrap() + 1);
            } else {
                // SSF index is up to date
                break;
            }
        }

        self.current_ssf_factor = self.get_scroll_speed_factor_from_time(time, self.current_ssf_index);
        self.current_track_position = self.get_position_from_time(time, self.current_sv_index);
    }

    fn in_range(&self, hit_object: &HitObject) -> bool {
        // checks if the hit object is in rendering range of the timing group
        match hit_object.end_time{
            Some(end_time) => {
                // LN logic
                if self.scroll_velocities.is_empty() {
                    return hit_object.start_time >= self.scroll_velocities[0].start_time && end_time <= self.scroll_velocities[self.scroll_velocities.len() - 1].start_time;
                }
                return hit_object.start_time >= self.scroll_velocities[0].start_time && end_time <= self.scroll_velocities[self.scroll_velocities.len() - 1].start_time;
            }
            None => {
                // normal hit object logic
                return hit_object.start_time >= self.scroll_velocities[0].start_time && hit_object.start_time <= self.scroll_velocities[self.scroll_velocities.len() - 1].start_time;
            }
        }
    }

}

impl Default for TimingGroup {
    fn default() -> Self {
        Self {
            initial_scroll_velocity: 1.0,
            scroll_velocities: Vec::new(),
            scroll_speed_factors: Vec::new(),
            color_rgb: None,
            current_track_position: 0,
            current_sv_index: None,
            current_ssf_index: None,
            current_ssf_factor: 1.0,
            adjusted_scroll_speed: 0.0,
            scroll_speed: 0.0,
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "PascalCase")]
enum TimeSignature {
    Quadruple = 4,
    Triple = 3,
}

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


fn one_f32() -> f32 { 
    return 1.0;
}

// linear interpolation between a and b based on time t
fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

// returns index of currently active item (start_time <= time)
fn index_at_time<T: HasStartTime>(list: &[T], time: f32) -> Option<usize> {
    // binary search
    if list.is_empty() || list[0].start_time() > time {
        // no items before or list is empty
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

// returns currently active item (start_time <= time)
fn at_time<T: HasStartTime>(list: &[T], time: f32) -> Option<&T> {
    index_at_time(list, time).map(|i| &list[i])
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

    for snap in BEAT_SNAPS {
        for n in 0..snap.divisor {
            let snap_pos = n as f32 / snap.divisor as f32;
            if (fraction - snap_pos).abs() < SNAP_EPSILON {
                return snap.color;
            }
        }
    }

    // fallback, last snap (48th)
    // return BEAT_SNAPS[BEAT_SNAPS.len() - 1].color;
    return WHITE;
}

fn set_reference_positions() -> FieldPositions {
    let mut field_positions = FieldPositions {
        receptor_position_y: 0.0,
        hit_position_y: 0.0,
        hold_hit_position_y: 0.0,
        hold_end_hit_position_y: 0.0,
        timing_line_position_y: 0.0,
        long_note_size_adjustment: 0.0,
    };

    let lane_size = SKIN.lane_width; // * base_to_virtual_ratio

    let hit_object_offset = lane_size * SKIN.note_height / SKIN.note_width;
    let hold_hit_object_offset = lane_size * SKIN.hold_note_height / SKIN.hold_note_width;
    let hold_end_offset = lane_size * SKIN.hold_note_height / SKIN.hold_note_width;
    let receptors_height = 0.0; // temp
    let receptors_width = lane_size; // temp
    let receptor_offset = lane_size * receptors_height / receptors_width;

    field_positions.long_note_size_adjustment = hold_hit_object_offset / 2.0;

    if SKIN.downscroll {
        field_positions.receptor_position_y = -SKIN.receptors_y_position - receptor_offset; // + window_height
        field_positions.hit_position_y = field_positions.receptor_position_y + 0.0 - hit_object_offset; // 0.0 = hit_pos_offset
        field_positions.hold_hit_position_y = field_positions.receptor_position_y + 0.0 - hold_hit_object_offset; // 0.0 = hit_pos_offset
        field_positions.hold_end_hit_position_y = field_positions.receptor_position_y + 0.0 - hold_end_offset; // 0.0 = hit_pos_offset
        field_positions.timing_line_position_y = field_positions.receptor_position_y + 0.0 // 0.0 = hit_pos_offset
    } else {
        // i dont care about upscroll right now
    }

    return field_positions;

}

struct FrameState<'a> {
    pub map: &'a mut Map,
    pub field_positions: &'a FieldPositions,
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

fn render_frame(state: &mut FrameState, draw: &mut impl Draw) {
    // calculates the positions of all objects and renders the current frame given the framestate


    state.map.update_current_track_position(state.map.time);
    state.map.update_scroll_speed();
    // state.map.update_hit_objects(state.field_positions);
    state.map.update_timing_lines();

    // reference/base screen size
    let base_height = 1440.0;
    let base_width = 2560.0;

    // for scaling
    let window_height = draw.screen_height();
    let window_width = draw.screen_width();
    let base_to_virtual_ratio = window_height / base_height;

    // background
    draw.draw_rectangle(
        0.0,
        0.0,
        window_width,
        window_height,
        macroquad::color::BLACK,
    );

    let num_lanes = state.map.get_key_count(false);
    let playfield_width = num_lanes as f32 * SKIN.lane_width;
    let playfield_x = (window_width - playfield_width) / 2.0;
    
    let scroll_speed_px_per_second = SKIN.scroll_speed * 10.0 * (window_height / 1400.0);

    // calculated scroll speed (still in px/s)
    let mut scroll_speed = if SKIN.rate_affects_scroll_speed {
        scroll_speed_px_per_second * state.map.rate
    } else {
        scroll_speed_px_per_second
    };

    // PER TIMING GROUP: USE SCROLL SPEED * CURRENTSSF 

    let line_color = GRAY;
    let line_thickness = 1.0;

    // timing lines
    for timing_line in &state.map.timing_lines {
        let timing_group = state.map.timing_groups.get(DEFAULT_TIMING_GROUP_ID).unwrap();

        // let sv_index = index_at_time(&timing_group.scroll_velocities, timing_line.start_time);
        // let track_position = timing_group.get_position_from_time(timing_line.start_time as f64, sv_index);

        // // how far in track units from the receptor (current time)
        // let delta_track = track_position - timing_group.current_track_position;

        // let pixels_per_effective_ms = scroll_speed / 1000.0;
        // let pixel_offset_from_receptor = delta_track as f32 * pixels_per_effective_ms;
        // let timing_line_y = (window_height - HIT_LINE_Y_POS_PX) - pixel_offset_from_receptor;


        // Y = TrackOffset + (CurrentTrackPosition * (ScrollDirection.Equals(ScrollDirection.Down) ? globalGroupController.ScrollSpeed : -globalGroupController.ScrollSpeed) / HitObjectManagerKeys.TrackRounding);
        let ss = timing_group.scroll_speed * timing_group.current_ssf_factor;
        let timing_line_y = (timing_line.track_offset as f32)
            + (timing_line.current_track_position as f32 * 
                (if SKIN.downscroll {ss} else {-ss}) / TRACK_ROUNDING
            );

        draw.draw_line(
            if SKIN.wide_timing_lines {
                0.0
            } else {
                playfield_x
            },
            timing_line_y,
            if SKIN.wide_timing_lines {
                window_width
            } else {
                playfield_x + (num_lanes as f32 * SKIN.lane_width)
            },
            timing_line_y,
            line_thickness,
            line_color,
        );
    }

    // notes
    for index in 0..state.map.hit_objects.len() {
        let note = &mut state.map.hit_objects[index].clone();
        let timing_group_id = &note.timing_group.clone().unwrap_or(DEFAULT_TIMING_GROUP_ID.to_string());
        let timing_group = state.map.timing_groups.get(timing_group_id).unwrap();
        // possibly change later
        // if note.removed {
        //     continue;
        // }

        // if note.start_time < state.map.time as f32 {
        //     note.removed = true;
        //     continue;
        // }

        // from Quaver.Shared/Screens/Gameplay/Rulesets/Keys/HitObjects/ScrollNoteController.cs
        //     Calculates the position of the Hit Object with a position offset.
        //
        // public override float GetSpritePosition(float hitPosition, float initialPos) =>
        //     hitPosition + ((initialPos - TimingGroupController.CurrentTrackPosition) *
        //                    (ScrollDirection == ScrollDirection.Down
        //                        ? -ScrollGroupController.ScrollSpeed
        //                        : ScrollGroupController.ScrollSpeed)
        //                    / HitObjectManagerKeys.TrackRounding);

        let note_y = timing_group.get_hit_object_position(
            note.hit_position,
            note.initial_track_position
        );

        // let sv_index = index_at_time(&timing_group.scroll_velocities, note.start_time);
        // let note_track_position = timing_group.get_position_from_time(note.start_time as f64, sv_index);

        // // how far in track units from the receptor (current time)
        // let delta_track = note_track_position - timing_group.current_track_position;

        // let pixels_per_effective_ms = scroll_speed / 1000.0;
        // let pixel_offset_from_receptor = delta_track as f32 * pixels_per_effective_ms;
        // // this is the bottom of the note
        // let note_y = (window_height - SKIN.receptors_y_position) - pixel_offset_from_receptor;

        // calculate x position based on lane (1-indexed in quaver)
        // adjust lane to be 0-indexed for calculation
        let lane_index = if state.map.mods.mirror {
            num_lanes - note.lane
        } else {
            note.lane - 1
        };

        let note_x = playfield_x
            + (lane_index as f32 * SKIN.lane_width)
            + (SKIN.lane_width / 2.0) // center in lane
            - (SKIN.note_width / 2.0);

        // snap colors
        let color = BEAT_SNAPS[note.snap_index].color;

                // if past receptors
        // let color = if note.start_time > state.time as f32 {
        //     BEAT_SNAPS[note.snap_index].color
        // } else {
        //     macroquad::color::BLACK
        // };

        draw.draw_rectangle(
            note_x,
            note_y - SKIN.note_height,
            SKIN.note_width,
            SKIN.note_height,
            color,
        );

    }

    // receptors (above notes)
    draw.draw_line(
        0.0,
        window_height - SKIN.receptors_y_position,
        window_width,
        draw.screen_height() - SKIN.receptors_y_position,
        2.0,
        macroquad::color::GRAY,
    );
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
    let mut is_fullscreen = false;

    // --- audio setup ---
    let mut audio_manager = AudioManager::new().map_err(|e| {
        eprintln!("Critical audio error on init: {}", e);
        std::io::Error::new(std::io::ErrorKind::Other, e)
    })?;

    // --- map loading ---
    let project_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let map_folder_path = project_dir.join("songs/femboymusic");
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

    map.length = audio_manager.get_total_duration_ms().unwrap();
    map.rate = audio_manager.get_rate();
    map.mods.mirror = true;

    // map processing functions
    let field_positions = set_reference_positions();
    map.link_default_timing_group();
    map.sort();
    map.initialize_position_markers();
    map.initialize_timing_lines();
    map.initialize_beat_snaps();
    map.initialize_scroll_speed();
    map.initialize_hit_objects(&field_positions);

    let total_svs = map.timing_groups.values().map(|g| g.scroll_velocities.len()).sum::<usize>();
    let total_ssfs = map.timing_groups.values().map(|g| g.scroll_speed_factors.len()).sum::<usize>();
    println!(
        "Map loaded successfully: {} Hit Objects, {} Timing Points, {} SVs, {} SSFs, {} Timing Groups",
        map.hit_objects.len(),
        map.timing_points.len(),
        total_svs,
        total_ssfs,
        map.timing_groups.len()
    );

    // this is the visual play state, audio is handled by audio_manager
    let mut is_playing_visuals = false;

    let mut json_output_file = File::create("output.json")?;
    let json_string = serde_json::to_string_pretty(&map)?;
    write!(json_output_file, "{}", json_string)?;
    println!("Parsed map data written to output.json");

    // main render loop
    loop {
        // --- inputs ---
        if is_key_pressed(KeyCode::Escape) || is_key_pressed(KeyCode::Backspace) {
            break;
        }
        if is_key_pressed(KeyCode::F11) || is_key_pressed(KeyCode::F) {
            is_fullscreen = !is_fullscreen;
            set_fullscreen(is_fullscreen);

        }
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
            let new_rate = (audio_manager.get_rate() + 0.1).min(2.0);
            audio_manager.set_rate(new_rate);
            map.rate = new_rate;
            map.initialize_scroll_speed();
        }
        if is_key_pressed(KeyCode::Left) {
            let new_rate = (audio_manager.get_rate() - 0.1).max(0.5);
            audio_manager.set_rate(new_rate);
            map.rate = new_rate;
            map.initialize_scroll_speed();
        }

        let time = audio_manager.get_current_song_time_ms();
        map.time = time;
        let mut macroquad_draw = MacroquadDraw;
        let mut frame_state = FrameState {
            map: &mut map,
            field_positions: &field_positions,
        };

        render_frame(&mut frame_state, &mut macroquad_draw);

        // --- draw ui / debug info ---
        let mut y_offset = 20.0;
        let line_height = 20.0;

        let total_duration_str = match audio_manager.get_total_duration_ms() {
            Some(d) => format!("{:.2}s", d / 1000.0),
            None => "N/A".to_string(),
        };
        draw_text(
            &format!(
                "Time: {:.2}s / {}",
                time / 1000.0,
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
                "Volume: {:.2} (up/down) | Rate: {:.1}x (left/right)",
                audio_manager.get_volume(),
                audio_manager.get_rate()
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

    Ok(())
}
