// #![allow(unused)]
#![allow(clippy::needless_return)]
mod audio_manager;
use audio_manager::AudioManager;
use vsrg_renderer::{index_at_time, lerp, object_at_time, sort_by_start_time, HasStartTime, Time};

use anyhow::Result;
use core::f64;
use macroquad::{
    color::Color,
    prelude::*,
    window::{screen_height, screen_width},
};
use serde::{Deserialize, Serialize};

use std::{
    collections::{HashMap, VecDeque},
    fs::{self, File},
    io::{Error, Write as _},
    mem::take,
    path::Path,
    string::ToString,
    time::Instant,
};

const DEFAULT_TIMING_GROUP_ID: &str = "$Default";
// const GLOBAL_TIMING_GROUP_ID: &str = "$Global";

const TRACK_ROUNDING: f64 = 100.0; // rounding for track positions, for int/float conversion

struct FieldPositions {
    // positions from top of screen
    receptor_position_y: f64,       // receptors position
    hit_position_y: f64,            // hit object target position
    hold_hit_position_y: f64,       // held hit object target position
    hold_end_hit_position_y: f64,   // LN end target position
    timing_line_position_y: f64,    // timing line position
    long_note_size_adjustment: f64, // size adjustment for LN so LN end time snaps with start time
}

// snap colors
const BEAT_SNAPS: &[BeatSnap] = &[
    BeatSnap { divisor: 48,  color: Color::new(1.0,           96.0  / 255.0, 96.0  / 255.0, 1.0) }, // 1st (red)
    BeatSnap { divisor: 24,  color: Color::new(61.0  / 255.0, 132.0 / 255.0, 1.0,           1.0) }, // 2nd (blue)
    BeatSnap { divisor: 16,  color: Color::new(178.0 / 255.0, 71.0  / 255.0, 1.0,           1.0) }, // 3rd (purple)
    BeatSnap { divisor: 12,  color: Color::new(1.0,           238.0 / 255.0, 58.0  / 255.0, 1.0) }, // 4th (yellow)
    BeatSnap { divisor: 8,   color: Color::new(1.0,           146.0 / 255.0, 210.0 / 255.0, 1.0) }, // 6th (pink)
    BeatSnap { divisor: 6,   color: Color::new(1.0,           167.0 / 255.0, 61.0  / 255.0, 1.0) }, // 8th (orange)
    BeatSnap { divisor: 4,   color: Color::new(132.0 / 255.0, 1.0,           1.0,           1.0) }, // 12th (cyan)
    BeatSnap { divisor: 3,   color: Color::new(127.0 / 255.0, 1.0,           138.0 / 255.0, 1.0) }, // 16th (green)
    BeatSnap { divisor: 1,   color: Color::new(200.0 / 255.0, 200.0 / 255.0, 200.0 / 255.0, 1.0) }, // 48th (gray) + fallback
];

struct Skin {
    // skin settings
    note_shape: &'static str,  // shape of the notes ("circles", "bars")
    lane_width: f64,           // width of each lane/column
    note_width: f64,           // width of each note
    note_height: f64,          // height of each note
    receptors_y_position: f64, // y position of the receptors/hit line
    scroll_speed: f64,         // scroll speed of the notes
    // rate_affects_scroll_speed: bool, // whether the rate multiplies the scroll speed
    // draw_lanes: bool,          // whether to draw the lanes
    wide_timing_lines: bool,   // whether to draw timing lines to the sides of the screen
    downscroll: bool,          // downscroll (true) or upscroll (false)
    normalize_scroll_velocity_by_rate_percentage: usize, // percentage of scaling applied when changing rates
}

const SKIN: Skin = Skin {
    note_shape: "bars",
    lane_width: 145.0, // 136
    note_width: 145.0,
    note_height: 36.0, // 36
    receptors_y_position: 226.0, // 226
    scroll_speed: 320.0, // 200, // 20 in quaver
    // rate_affects_scroll_speed: false,
    // draw_lanes: true,
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

// anything representing a position on the track
type Position = i64;

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "PascalCase")]
#[derive(Default)]
enum GameMode {
    #[default]
    Keys4,
    Keys7,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "PascalCase")]
struct Map {
    audio_file: Option<String>,      // audio file name
    song_preview_time: Option<f64>,  // time (ms) of the song where the preview starts
    background_file: Option<String>, // background file name
    banner_file: Option<String>,     // mapset banner name
    map_id: Option<f64>,             // unique Map Identifier (-1 if not submitted)
    map_set_id: Option<f64>,         // unique Map Set identifier (-1 if not submitted)
    #[serde(default)]
    mode: GameMode,                  // game mode for this map {Keys4, Keys7}
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
    #[serde(default = "one_f64")]
    initial_scroll_velocity: f64,    // the initial SV before the first SV change
    #[serde(default)]
    has_scratch_key: bool,           // +1 scratch key (5/8 key play)
    #[serde(default)]
    editor_layers: Vec<serde_yaml::Value>,
    #[serde(default)]
    bookmarks: Vec<serde_yaml::Value>,
    #[serde(default)]
    custom_audio_samples: Vec<serde_yaml::Value>,
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
    time: Time, // current time in the map
    #[serde(skip)]
    rate: f64,
    #[serde(skip)]
    mods: Mods,
    #[serde(skip)]
    length: Time, // length of the map in ms
}

impl Map {
    fn initialize_default_timing_group(&mut self) {
        // adds the default timing group to timing_groups
        self.timing_groups.insert(
            DEFAULT_TIMING_GROUP_ID.to_string(),
            TimingGroup {
                initial_scroll_velocity: self.initial_scroll_velocity, // set init sv
                scroll_velocities: take(&mut self.scroll_velocities), // copy svs
                scroll_speed_factors: take(&mut self.scroll_speed_factors), // copy ssfs
                color_rgb: None,
                current_track_position: 0,
                current_ssf_factor: 1.0,
                scroll_speed: 0.0,
            },
        );

        // set every hitobject whose timing group is null to the default group
        for hit_object in &mut self.hit_objects {
            if hit_object.timing_group.is_none() {
                hit_object.timing_group = Some(DEFAULT_TIMING_GROUP_ID.to_string());
            }
        }
    }

    fn initialize_control_points(&mut self) {
        // set cumulative positions for SV points
        for timing_group in self.timing_groups.values_mut() {
            if timing_group.scroll_velocities.is_empty() {
                continue; // no SVs, nothing to set
            }

            // start with first SV point (position = time * sv)
            let mut position = (timing_group.scroll_velocities[0].start_time
                * timing_group.initial_scroll_velocity
                * TRACK_ROUNDING) as Position;
            timing_group.scroll_velocities[0].cumulative_position = position;

            // loop through SV indexes 1 to SVs-1 (rest of SVs)
            for index in 1..timing_group.scroll_velocities.len() {
                let current_sv = &timing_group.scroll_velocities[index];
                let previous_sv = &timing_group.scroll_velocities[index - 1];

                // we are computing up to current SV's point, so we use the previous SV's multiplier
                let multiplier = previous_sv.multiplier;

                // distance between last and current SV, times the previous SV's multiplier
                let distance = (current_sv.start_time - previous_sv.start_time) * multiplier;

                position += (distance * TRACK_ROUNDING) as Position;
                timing_group.scroll_velocities[index].cumulative_position = position;
            }
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

            hit_object.start_position = timing_group.get_position_from_time(hit_object.start_time);

            hit_object.hit_position = if hit_object.end_time.is_some() {
                // LN
                field_positions.hold_end_hit_position_y
            } else {
                field_positions.hit_position_y
            };
            hit_object.hold_end_hit_position = field_positions.hold_end_hit_position_y;
        }
    }

    fn initialize_timing_lines(&mut self, field_positions: &FieldPositions) {
        // creates timing lines based on timing points' signatures and BPMs
        self.timing_lines.clear();

        let tg = &self.timing_groups[DEFAULT_TIMING_GROUP_ID];

        // loop through timing points
        for tp_index in 0..self.timing_points.len() {
            // no timing lines if hidden
            if self.timing_points[tp_index].hidden {
                continue;
            }

            // the "current time" that will be incrementing)
            let mut current_time = self.timing_points[tp_index].start_time;
            let end_time = if tp_index + 1 < self.timing_points.len() {
                // this isn't the last timing point
                // end 1ms earlier to avoid possible timing line overlap
                self.timing_points[tp_index + 1].start_time - 1f64
            } else {
                // last timing point, end at map length
                self.length
            };

            // time signature (3/4 or 4/4)
            let signature = f64::from(
                self.timing_points[tp_index]
                    .time_signature
                    .clone()
                    .unwrap_or(TimeSignature::Quadruple) as u32,
            );

            // "max possible sane value for timing lines" - quaver devs
            const MAX_BPM: f64 = 9999.0;
            let ms_per_beat = 60000f64 / MAX_BPM.min(self.timing_points[tp_index].bpm.abs());
            // how many ms between measures/timing lines
            let ms_increment = signature * ms_per_beat;
            if ms_increment <= 0f64 {
                continue; // no increment, skip this timing point
            }

            while current_time < end_time {
                // position for the timing line
                let start_position = tg.get_position_from_time(current_time);

                // create and add new timing line
                self.timing_lines.push(TimingLine {
                    start_time: current_time,
                    start_position,
                    current_track_position: 0,
                    hit_position: field_positions.timing_line_position_y,
                });

                // increment time for next timing line
                current_time += ms_increment;
            }
        }
    }

    fn initialize_beat_snaps(&mut self) {
        for hit_object in &mut self.hit_objects {
            // get active timing point at hit object's start time
            let timing_point = object_at_time(&self.timing_points, hit_object.start_time)
                .unwrap_or(&self.timing_points[0]);

            // get beat length (ms per beat)
            let beat_length = 60000f64 / timing_point.bpm;
            // calculate offset from timing point start time
            let offset = hit_object.start_time - timing_point.start_time;

            // calculate note's snap index
            let index = (48.0 * offset / beat_length).round() as u32;

            // defualt value; will be overwritten unless
            // not snapped to 1/16 or less, snap to 1/48
            hit_object.snap_index = 8;

            // loop through beat snaps to find the correct one
            for (i, snap_type) in BEAT_SNAPS.iter().enumerate() {
                if index % snap_type.divisor == 0 {
                    // snap to this color
                    hit_object.snap_index = i;
                    break;
                }
            }
        }
    }

    fn sort(&mut self) {
        // sort hit objects
        sort_by_start_time(&mut self.hit_objects);

        // sort timing points
        sort_by_start_time(&mut self.timing_points);

        // sort scroll velocities
        for timing_group in self.timing_groups.values_mut() {
            sort_by_start_time(&mut timing_group.scroll_velocities);
        }

        // sort scroll speed factors
        for timing_group in self.timing_groups.values_mut() {
            sort_by_start_time(&mut timing_group.scroll_speed_factors);
        }
    }

    fn update_track_position(&mut self, time: Time) {
        // update current track position of hit objects in each timing group
        self.time = time;
        for timing_group in self.timing_groups.values_mut() {
            timing_group.current_ssf_factor = timing_group.get_scroll_speed_factor_from_time(time);
            timing_group.current_track_position = timing_group.get_position_from_time(time);
        }
    }

    fn update_scroll_speed(&mut self) {
        // updates the scroll speed of all timing groups
        let speed = SKIN.scroll_speed;
        let rate_scaling = 1f64
            + (self.rate - 1f64)
            * (SKIN.normalize_scroll_velocity_by_rate_percentage as f64 / 100f64);
        let adjusted_scroll_speed = (speed * rate_scaling).clamp(50.0, 1000.0);
        let scaling_factor = 1920f64 / 1366f64; // quaver's scaling

        let scroll_speed = (adjusted_scroll_speed / 10f64)
            / (20f64 * self.rate)
            * scaling_factor; // * base_to_virtual_ratio

        for timing_group in self.timing_groups.values_mut() {
            timing_group.scroll_speed = scroll_speed;
        }
    }

    fn update_timing_lines(&mut self) {
        // updates the position of all timing lines
        let timing_group = self
            .timing_groups
            .get_mut(DEFAULT_TIMING_GROUP_ID)
            .unwrap();
        for timing_line in &mut self.timing_lines {
            // timing_line.current_track_position = (timing_group.current_track_position - timing_line.start_position);
            timing_line.current_track_position = timing_group.get_object_position(
                timing_line.hit_position,
                timing_line.start_position,
                true,
            );
        }
    }

    fn update_hit_objects(&mut self) {
        // update the position of all hit objects
        // https://github.com/Quaver/Quaver/blob/develop/Quaver.Shared/Screens/Gameplay/Rulesets/Keys/HitObjects/GameplayHitObjectKeys.cs#L387
        for hit_object in &mut self.hit_objects {
            let timing_group = self
                .timing_groups
                .get_mut(hit_object.timing_group.as_ref().unwrap())
                .unwrap();

            while hit_object.previous_positions.len() < 10 {
                // ensure we have at least 10 previous positions
                hit_object
                    .previous_positions
                    .push_front(hit_object.position);
            }

            hit_object.previous_positions.push_front(hit_object.position);
            if hit_object.previous_positions.len() > 10 {
                hit_object
                    .previous_positions
                    .pop_back();
            }

            hit_object.position = timing_group.get_object_position(
                hit_object.hit_position,
                hit_object.start_position,
                true,
            );
        }
    }

    const fn get_key_count(&self, include_scratch: bool) -> i64 {
        // returns the number of keys in the map
        let key_count = match self.mode {
            GameMode::Keys4 => 4,
            GameMode::Keys7 => 7,
        };

        if self.has_scratch_key && include_scratch {
            key_count + 1
        } else {
            key_count
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
struct TimingLine {
    #[serde(default)]
    start_time: Time, // time when the timing line reaches the receptor
    #[serde(default)]
    start_position: Position, // the timing line's position offset on track
    #[serde(default)]
    current_track_position: Position, // track position; >0 = hasnt passed receptors
    #[serde(skip)]
    hit_position: f64, // position of the timing line on the screen
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "PascalCase")]
struct TimingPoint {
    // sets values until next timing point
    // used for bpm change, or affine
    #[serde(default)]
    start_time: Time, // start time (ms)
    bpm: f64,
    time_signature: Option<TimeSignature>,
    #[serde(default)]
    hidden: bool, // show timing lines
}

impl HasStartTime for TimingPoint {
    fn start_time(&self) -> Time {
        self.start_time
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "PascalCase")]
struct ControlPoint {
    // represents either an SV or SSF point
    #[serde(default)]
    start_time: Time,
    #[serde(default)]
    multiplier: f64,
    #[serde(skip_deserializing)]
    length: Option<Time>, // none if last point
    #[serde(skip_deserializing)]
    cumulative_position: Position, // cumulative distance from the start of the map
}

impl HasStartTime for ControlPoint {
    fn start_time(&self) -> Time {
        self.start_time
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "PascalCase")]
struct HitObject {
    // a note
    #[serde(default)]
    start_time: Time,
    end_time: Option<Time>, // if Some, then its an LN
    lane: i64,
    key_sounds: Vec<KeySound>, // key sounds to play when this object is hit
    #[serde(default)]
    timing_group: Option<String>,
    #[serde(skip)]
    snap_index: usize, // index for snap color
    #[serde(skip)]
    hold_end_hit_position: f64, // position for LN ends
    #[serde(skip)]
    hit_position: f64, // where the note is "hit", calculated from hit body height and hit position offset
    #[serde(skip)]
    start_position: Position, // track position at start_time (in timing group)
    #[serde(skip)]
    position: Position, // live map position, calculated with timing group
    #[serde(skip)]
    previous_positions: VecDeque<Position>, // previous positions, used for rendering effects
}

// public virtual float CurrentLongNoteBodySize => (LatestHeldPosition - EarliestHeldPosition) *
//     TimingGroupController.ScrollSpeed / HitObjectManagerKeys.TrackRounding;

impl HasStartTime for HitObject {
    fn start_time(&self) -> Time {
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
    #[serde(default = "one_f64")]
    initial_scroll_velocity: f64,
    #[serde(default)]
    scroll_velocities: Vec<ControlPoint>,
    #[serde(default)]
    scroll_speed_factors: Vec<ControlPoint>,
    color_rgb: Option<String>,
    // log::info for playback
    #[serde(skip)]
    current_track_position: Position, // current playback position
    #[serde(default = "one_f64")]
    current_ssf_factor: f64, // current SSF multiplier
    #[serde(skip)]
    scroll_speed: f64, // speed at which objects travel across the screen
}

impl TimingGroup {
    fn get_scroll_speed_factor_from_time(&self, time: Time) -> f64 {
        // gets the SSF multiplier at a time, with linear interpolation
        let ssf_index = index_at_time(&self.scroll_speed_factors, time);

        match ssf_index {
            None => {
                // before first SSF point or no SSFs, so no effect applied
                return 1.0;
            }
            Some(index) => {
                let ssf = &self.scroll_speed_factors[index];
                if index == self.scroll_speed_factors.len() - 1 {
                    // last point, no interpolation
                    return ssf.multiplier;
                }

                let next_ssf = &self.scroll_speed_factors[index + 1];
                // lerp between this and next point based on time between
                return lerp(
                    ssf.multiplier,
                    next_ssf.multiplier,
                    (time - ssf.start_time) / (next_ssf.start_time - ssf.start_time),
                );
            }
        }
    }

    fn get_position_from_time(&self, time: Time) -> Position {
        // calculates the timing group's track position with time and SV
        let sv_index = index_at_time(&self.scroll_velocities, time);

        match sv_index {
            None => {
                // before first SV point or no SVs, so use initial scroll velocity
                return (time
                    * self.initial_scroll_velocity
                    * TRACK_ROUNDING
                ) as Position;
            }
            Some(index) => {
                // get the track position at the start of the current SV point
                let mut current_position = self.scroll_velocities[index].cumulative_position;

                // add the distance between the start of the current SV point and the time
                current_position += ((time - self.scroll_velocities[index].start_time)
                    * self.scroll_velocities[index].multiplier
                    * TRACK_ROUNDING) as Position;
                return current_position;
            }
        }
    }

    fn get_object_position(&self, hit_position: f64, initial_position: Position, use_ssf: bool) -> Position {
        // calculates the position of a hit object with a position offset
        // note: signs were swapped in quaver?
        let mut scroll_speed = if SKIN.downscroll {
            -self.scroll_speed
        } else {
            self.scroll_speed
        };

        if use_ssf {
            // apply SSF factor
            scroll_speed *= self.current_ssf_factor;
        }

        let distance = (initial_position as f64) - (self.current_track_position as f64);
        let position = hit_position + (distance * scroll_speed / TRACK_ROUNDING);
        return position as Position;
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
            current_ssf_factor: 1.0,
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

const fn one_f64() -> f64 {
    return 1.0;
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

    // let lane_size = SKIN.lane_width; // * base_to_virtual_ratio

    // let hit_object_offset = lane_size * SKIN.note_height / SKIN.note_width;
    let hit_object_offset = 0f64; // temp
    // let hold_hit_object_offset = lane_size * SKIN.hold_note_height / SKIN.hold_note_width;
    let hold_hit_object_offset = 0f64; // temp
    // let hold_end_offset = lane_size * SKIN.hold_note_height / SKIN.hold_note_width;
    let hold_end_offset = 0f64; // temp
    // let receptors_height = 0.0; // temp
    // let receptors_width = lane_size; // temp
    // let receptor_offset = lane_size * receptors_height / receptors_width;
    let receptor_offset = 0f64; // temp

    field_positions.long_note_size_adjustment = hold_hit_object_offset / 2f64;

    if SKIN.downscroll {
        field_positions.receptor_position_y = -SKIN.receptors_y_position - receptor_offset; // + window_height
        field_positions.hit_position_y = field_positions.receptor_position_y + 0f64 - hit_object_offset; // 0.0 = hit_pos_offset
        field_positions.hold_hit_position_y = field_positions.receptor_position_y + 0f64 - hold_hit_object_offset; // 0.0 = hit_pos_offset
        field_positions.hold_end_hit_position_y = field_positions.receptor_position_y + 0f64 - hold_end_offset; // 0.0 = hit_pos_offset
        field_positions.timing_line_position_y = field_positions.receptor_position_y + 0f64; // 0.0 = hit_pos_offset
    } else {
        // i dont care about upscroll right now
    }

    return field_positions;
}

struct FrameState<'map> {
    pub map: &'map mut Map,
    pub field_positions: &'map FieldPositions,
}

trait Draw {
    fn draw_rectangle(&mut self, x: f64, y: f64, w: f64, h: f64, color: Color);
    fn draw_line(&mut self, x1: f64, y1: f64, x2: f64, y2: f64, thickness: f64, color: Color);
    fn draw_circle(&mut self, x: f64, y: f64, radius: f64, color: Color);
    fn draw_circle_outline(&mut self, x: f64, y: f64, radius: f64, thickness: f64, color: Color);
    // fn draw_text(&mut self, text: &str, x: f64, y: f64, size: f64, color: macroquad::color::Color);
    fn screen_height(&self) -> f64;
    fn screen_width(&self) -> f64;
}

struct MacroquadDraw;

impl Draw for MacroquadDraw {
    fn draw_rectangle(&mut self, x: f64, y: f64, w: f64, h: f64, color: Color) {
        draw_rectangle(x as f32, y as f32, w as f32, h as f32, color);
    }
    fn draw_line(&mut self, x1: f64, y1: f64, x2: f64, y2: f64, thickness: f64, color: Color) {
        draw_line(x1 as f32, y1 as f32, x2 as f32, y2 as f32, thickness as f32, color);
    }
    fn draw_circle(&mut self, x: f64, y: f64, radius: f64, color: Color) {
        draw_circle(x as f32, y as f32, radius as f32, color);
    }
    fn draw_circle_outline(&mut self, x: f64, y: f64, radius: f64, thickness: f64, color: Color) {
        draw_circle_lines(x as f32, y as f32, radius as f32, thickness as f32, color);
    }
    // fn draw_text(&mut self, text: &str, x: f64, y: f64, size: f64, color: macroquad::color::Color) {
    //     macroquad::text::draw_text(text, x as f32, y as f32, size as f32, color);
    // }
    fn screen_height(&self) -> f64 {
        return f64::from(screen_height());
    }
    fn screen_width(&self) -> f64 {
        return f64::from(screen_width());
    }
}

fn render_frame(state: &mut FrameState, draw: &mut impl Draw) {
    // calculates the positions of all objects and renders the current frame given the framestate

    // update functions
    state.map.update_track_position(state.map.time);
    state.map.update_scroll_speed();
    state.map.update_timing_lines();
    state.map.update_hit_objects();

    // reference/base screen size
    // let base_height = 1440.0;
    // let base_width = 2560.0;

    // for scaling
    let window_height = draw.screen_height();
    let window_width = draw.screen_width();
    // let base_to_virtual_ratio = window_height / base_height;

    let num_lanes = state.map.get_key_count(false);
    let playfield_width = num_lanes as f64 * SKIN.lane_width;
    let playfield_x = (window_width - playfield_width) / 2f64;

    // receptors (above notes)
    match SKIN.note_shape {
        "bars" => {
            draw.draw_line(
                0.0,
                window_height + state.field_positions.receptor_position_y,
                window_width,
                window_height + state.field_positions.receptor_position_y,
                3.0,
                GRAY,
            );
        }
        "circles" => {
            for i in 0i32..4i32 {
                // draw receptors
                let receptor_x = playfield_x
                    + (f64::from(i) * SKIN.lane_width)
                    + (SKIN.lane_width / 2f64); // center in lane;

                draw.draw_circle_outline(
                    receptor_x,
                    window_height + state.field_positions.receptor_position_y,
                    SKIN.note_width / 2.2,
                    2.0,
                    GRAY,
                );
            }
        }
        _ => {}
    }

    let line_color = GRAY;
    let line_thickness = 1f64;

    // timing lines
    for timing_line in &state.map.timing_lines {
        let timing_line_y = (timing_line.current_track_position as f64) + window_height;

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
                playfield_x + (num_lanes as f64 * SKIN.lane_width)
            },
            timing_line_y,
            line_thickness,
            line_color,
        );
    }

    // notes
    for index in 0..state.map.hit_objects.len() {
        let note = &state.map.hit_objects[index];
        if note.start_time <= state.map.time {
            // past receptors, skip rendering
            continue;
        }

        let note_y = (note.position as f64) + window_height; // real hitbox

        // calculate x position based on lane (1-indexed in quaver)
        // adjust lane to be 0-indexed for calculation
        let lane_index = if state.map.mods.mirror {
            num_lanes - note.lane
        } else {
            note.lane - 1
        };

        let note_x = playfield_x
            + (lane_index as f64 * SKIN.lane_width)
            + (SKIN.lane_width / 2f64) // center in lane
            - (SKIN.note_width / 2f64);

        let mut note_top_offset = SKIN.note_height / 2f64;
        let mut note_bottom_offset = SKIN.note_height / 2f64;
        let middle_position = note_y - SKIN.note_height / 2f64;
        let frame_behind = 0;
        let stretch_limit = SKIN.note_height * 8f64; // max stretch limit

        // calculate stretch from previous positions
        for i in 0..note.previous_positions.len() {
            if i >= frame_behind {
                break;
            }
            // pos = moving down (top), neg = moving up (bottom)
            let stretch = (note.position - note.previous_positions[i]) as f64;

            if stretch.abs() > stretch_limit {
                // stretch is too big, ignore
                continue;
            }
            if stretch > 0f64 {
                note_top_offset = note_top_offset.max(stretch);
            } else {
                // moving up
                note_bottom_offset = note_bottom_offset.max(-stretch);
            }
        }

        // snap colors
        let color = BEAT_SNAPS[note.snap_index].color;

        match SKIN.note_shape {
            "bars" => {
                draw.draw_rectangle(
                    note_x,
                    middle_position - note_top_offset,
                    SKIN.note_width,
                    note_top_offset + note_bottom_offset,
                    color,
                );
                // draw.draw_rectangle( // middle of note
                //     note_x,
                //     middle_position,
                //     SKIN.note_width,
                //     1.0,
                //     WHITE,
                // );
                // draw.draw_rectangle( // hitbox
                //     note_x,
                //     note_y,
                //     SKIN.note_width,
                //     1.0,
                //     WHITE,
                // );
            }
            "circles" => {
                draw.draw_circle(
                    note_x + (SKIN.note_width / 2.0),
                    note_y,
                    SKIN.note_width / 2.4,
                    color,
                );
            }
            _ => {}
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
async fn main() -> anyhow::Result<()> {
    simple_logger::init().unwrap();
    let mut is_fullscreen = false;
    let song_name = "funky";

    // --- audio setup ---
    let mut audio_manager = AudioManager::new().map_err(|e| {
        log::error!("Critical audio error on init: {e}");
        Error::other(e)
    })?;

    // --- map loading ---
    let project_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let map_folder_path = project_dir.join("songs/").join(song_name);
    let map_file_name_option = fs::read_dir(&map_folder_path)
        .map_err(|e| anyhow::anyhow!("Failed to read map directory {:?}: {}", map_folder_path, e))?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .find(|path| path.extension().is_some_and(|ext| ext == "qua"))
        .and_then(|path| path.to_str().map(ToString::to_string));

    let Some(map_file_name) = map_file_name_option else {
        let err_msg = format!(
            "Error: No .qua file found in directory {}",
            map_folder_path.display()
        );
        log::error!("{err_msg}");
        loop {
            clear_background(DARKGRAY);
            draw_text(&err_msg, 20.0, 20.0, 20.0, RED);
            next_frame().await;
        }
    };

    log::info!("Loading map: {map_file_name}");
    let qua_file_content = fs::read_to_string(&map_file_name)
        .map_err(|e| anyhow::anyhow!("Failed to read map file '{}': {}", map_file_name, e))?;

    let mut map: Map = serde_yaml::from_str(&qua_file_content)
        .map_err(|e| anyhow::anyhow!("Failed to parse map data from '{}': {}", map_file_name, e))?;

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

    map.length = audio_manager.get_total_duration_ms().unwrap_or(0f64);
    map.rate = audio_manager.get_rate();
    map.mods.mirror = true;

    // map processing functions / preload
    let field_positions = set_reference_positions();
    map.initialize_default_timing_group();
    map.sort();
    map.initialize_control_points();
    map.initialize_hit_objects(&field_positions);
    map.initialize_timing_lines(&field_positions);
    map.initialize_beat_snaps();

    let total_hit_objects = map.hit_objects.len();
    let total_timing_points = map.timing_points.len();
    let total_svs = map.timing_groups.values().map(|g| g.scroll_velocities.len()).sum::<usize>();
    let total_ssfs = map.timing_groups.values().map(|g| g.scroll_speed_factors.len()).sum::<usize>();
    let total_timing_groups = map.timing_groups.len();
    let total_timing_lines = map.timing_lines.len();
    log::info!(
        "Map loaded successfully: {total_hit_objects} Hit Objects, {total_timing_points} Timing Points, {total_svs} SVs, {total_ssfs} SSFs, {total_timing_groups} Timing Groups, {total_timing_lines} Timing Lines",
    );

    // this is the visual play state, audio is handled by audio_manager
    let mut is_playing_visuals = false;

    let mut json_output_file = File::create("output.json")?;
    let json_string = serde_json::to_string_pretty(&map)?;
    write!(json_output_file, "{json_string}")?;
    log::info!("Parsed map data written to output.json");

    let start_instant = Instant::now();
    let mut frame_count: u64 = 0;

    // main render loop
    loop {
        frame_count += 1;

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
        }
        if is_key_pressed(KeyCode::Left) {
            let new_rate = (audio_manager.get_rate() - 0.1).max(0.5);
            audio_manager.set_rate(new_rate);
            map.rate = new_rate;
        }

        let time = audio_manager.get_current_song_time_ms();
        map.time = time;
        let mut macroquad_draw = MacroquadDraw;
        let mut frame_state = FrameState {
            map: &mut map,
            field_positions: &field_positions,
        };

        // --------- render stuff --------

        clear_background(BLACK); // resets frame to all black
        render_frame(&mut frame_state, &mut macroquad_draw);

        // -------- draw ui / debug info --------

        let mut y_offset = 20.0;
        let line_height = 20.0;

        if let (
            Some(title),
            Some(artist),
            Some(difficulty),
            Some(creator),
        ) = (
            map.title.as_ref(),
            map.artist.as_ref(),
            map.difficulty_name.as_ref(),
            map.creator.as_ref(),
        ) {
            draw_text(
                &format!("Map: {title} - {artist} [{difficulty}] by {creator}"),
                10.0,
                y_offset,
                20.0,
                WHITE,
            );
            y_offset += line_height;
        }

        draw_text(
            &format!(
                "{total_hit_objects} Notes, {total_svs} SVs, {total_ssfs} SSFs, {total_timing_groups} Groups, {total_timing_points} Timing Points, {total_timing_lines} Timing Lines",
            ),
            10.0,
            y_offset,
            20.0,
            WHITE,
        );
        y_offset += line_height;
        y_offset += line_height;

        let visual_state_text = if is_playing_visuals {
            "Playing"
        } else {
            "Paused"
        };
        let audio_actual_state_text = if audio_manager.is_playing() {
            "Playing"
        } else if audio_manager
            .sink
            .as_ref()
            .is_some_and(rodio::Sink::is_paused)
        {
            "Paused"
        } else {
            "Stopped/empty"
        };
        draw_text(
            &format!(
                "Visuals: {visual_state_text} | Audio: {audio_actual_state_text} (space, r)"
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

        let total_duration_str = match audio_manager.get_total_duration_ms() {
            Some(d) => format!("{:.2}s", d / 1000f64),
            None => "N/A".to_string(),
        };
        draw_text(
            &format!("Time: {:.2}s / {}", time / 1000f64, total_duration_str),
            10.0,
            y_offset,
            20.0,
            WHITE,
        );
        y_offset += line_height;

        let fps = format!("{:<3}", get_fps());
        let elapsed = start_instant.elapsed().as_secs_f64();
        let avg_fps = if elapsed > 0f64 {
            frame_count as f64 / elapsed
        } else {
            0f64
        };
        draw_text(
            &format!("FPS: {fps} | {avg_fps:.2}"),
            10.0,
            y_offset,
            20.0,
            WHITE,
        );
        y_offset += line_height;

        if let Some(err_msg) = audio_manager.get_error() {
            draw_text(
                &format!("Audio status: {err_msg}"),
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
