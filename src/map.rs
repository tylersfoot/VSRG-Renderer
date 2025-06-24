use crate::constants::{FieldPositions, BEAT_SNAPS, DEFAULT_TIMING_GROUP_ID, SKIN, TRACK_ROUNDING};
use crate::{index_at_time, lerp, object_at_time, sort_by_start_time, HasStartTime, Time};
use anyhow::{bail, Result};
use log::warn;
use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, VecDeque},
    mem::take,
};

// anything representing a position on the track
pub type Position = i64;

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct Mods {
    pub mirror: bool, // mirror notes horizontally
    pub no_sv: bool,  // ignore scroll velocity
    pub no_ssf: bool, // ignore scroll speed factor
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "PascalCase")]
#[derive(Default)]
pub enum GameMode {
    #[default]
    Keys4,
    Keys7,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "PascalCase")]
pub struct Map {
    pub audio_file: Option<String>,      // audio file name
    pub song_preview_time: Option<f64>,  // time (ms) of the song where the preview starts
    pub background_file: Option<String>, // background file name
    pub banner_file: Option<String>,     // mapset banner name
    pub map_id: Option<f64>,             // unique Map Identifier (-1 if not submitted)
    pub map_set_id: Option<f64>,         // unique Map Set identifier (-1 if not submitted)
    #[serde(default)]
    pub mode: GameMode,                  // game mode for this map {Keys4, Keys7}
    pub title: Option<String>,           // song title
    pub artist: Option<String>,          // song artist
    pub source: Option<String>,          // source of the song (album, mixtape, etc.)
    pub tags: Option<String>,            // any tags that could be used to help find the song
    pub creator: Option<String>,         // map creator
    pub difficulty_name: Option<String>, // map difficulty name
    pub description: Option<String>,     // map description
    pub genre: Option<String>,           // song genre
    #[serde(rename = "LegacyLNRendering")]
    #[serde(default)]
    pub legacy_ln_rendering: bool,       // whether to use the old LN rendering system (earliest/latest -> start/end)
    #[serde(rename = "BPMDoesNotAffectScrollVelocity")]
    #[serde(default)]
    pub bpm_does_not_affect_scroll_velocity: bool, // indicates if BPM changes affect SV
    #[serde(default = "one_f64")]
    pub initial_scroll_velocity: f64,    // the initial SV before the first SV change
    #[serde(default)]
    pub has_scratch_key: bool,           // +1 scratch key (5/8 key play)
    #[serde(default)]
    pub editor_layers: Vec<serde_yaml::Value>,
    #[serde(default)]
    pub bookmarks: Vec<serde_yaml::Value>,
    #[serde(default)]
    pub custom_audio_samples: Vec<serde_yaml::Value>,
    #[serde(default)]
    pub timing_points: Vec<TimingPoint>,
    #[serde(default)]
    pub timing_lines: Vec<TimingLine>,
    #[serde(rename = "SliderVelocities")]
    #[serde(default)]
    pub scroll_velocities: Vec<ControlPoint>,
    #[serde(default)]
    pub scroll_speed_factors: Vec<ControlPoint>,
    #[serde(default)]
    pub hit_objects: Vec<HitObject>,
    #[serde(default)]
    pub timing_groups: HashMap<String, TimingGroup>,
    #[serde(skip_deserializing)]
    pub file_path: String, // map file path
    #[serde(skip)]
    pub time: Time, // current time in the map
    #[serde(skip)]
    pub rate: f64,
    #[serde(skip)]
    pub mods: Mods,
    #[serde(skip)]
    pub length: Time, // length of the map in ms
}

impl Map {
    pub fn initialize_default_timing_group(&mut self) {
        // adds the default timing group to timing_groups
        self.timing_groups.insert(
            DEFAULT_TIMING_GROUP_ID.to_string(),
            TimingGroup {
                initial_scroll_velocity: self.initial_scroll_velocity,
                scroll_velocities: take(&mut self.scroll_velocities),
                scroll_speed_factors: take(&mut self.scroll_speed_factors),
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

    pub fn initialize_control_points(&mut self) {
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

    pub fn initialize_hit_objects(&mut self, field_positions: &FieldPositions) -> Result<()> {
        // initialize the hit objects
        // https://github.com/Quaver/Quaver/blob/develop/Quaver.Shared/Screens/Gameplay/Rulesets/Keys/HitObjects/GameplayHitObjectKeys.cs#L161
        for hit_object in &mut self.hit_objects {
            let Some(group_id) = hit_object.timing_group.as_ref() else {
                warn!(
                    "Hit object at time {} has no timing group",
                    hit_object.start_time
                );
                continue;
            };

            let Some(timing_group) = self.timing_groups.get_mut(group_id) else {
                warn!(
                    "Timing group '{}' not found for hit object at time {}",
                    group_id, hit_object.start_time
                );
                continue;
            };

            hit_object.start_position = timing_group.get_position_from_time(hit_object.start_time, false);
            hit_object.start_position_tail = if hit_object.end_time.is_some() {
                // if this is a long note, set the end position
                timing_group.get_position_from_time(hit_object.end_time.unwrap(), false)
            } else {
                // if not a long note, set end position to start position
                hit_object.start_position
            };
            hit_object.hit_position = field_positions.hit_position_y;
        }

        Ok(())
    }

    pub fn initialize_timing_lines(&mut self, field_positions: &FieldPositions) -> Result<()> {
        // creates timing lines based on timing points' signatures and BPMs
        self.timing_lines.clear();

        let Some(tg) = self.timing_groups.get(DEFAULT_TIMING_GROUP_ID) else {
            bail!(
                "Default timing group '{}' not found",
                DEFAULT_TIMING_GROUP_ID
            );
        };

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
                let start_position = tg.get_position_from_time(current_time, false);

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

        Ok(())
    }

    pub fn initialize_beat_snaps(&mut self) -> Result<()> {
        if self.timing_points.is_empty() {
            bail!("Cannot initialize beat snaps without timing points");
        }

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

        Ok(())
    }

    pub fn sort(&mut self) {
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

    pub fn update_track_position(&mut self, time: Time) {
        // update current track position of hit objects in each timing group
        self.time = time;
        for timing_group in self.timing_groups.values_mut() {
            timing_group.current_ssf_factor = timing_group.get_scroll_speed_factor_from_time(time);
            timing_group.current_track_position = timing_group.get_position_from_time(time, self.mods.no_sv);
        }
    }

    pub fn update_scroll_speed(&mut self) {
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

    pub fn update_timing_lines(&mut self) -> Result<()> {
        // updates the position of all timing lines
        let Some(timing_group) = self.timing_groups.get_mut(DEFAULT_TIMING_GROUP_ID) else {
            bail!("Default timing group '{}' not found", DEFAULT_TIMING_GROUP_ID);
        };
        for timing_line in &mut self.timing_lines {
            // timing_line.current_track_position = (timing_group.current_track_position - timing_line.start_position);
            timing_line.current_track_position = timing_group.get_object_position(
                timing_line.hit_position,
                if self.mods.no_sv {
                    (timing_line.start_time * TRACK_ROUNDING) as Position
                } else {
                    timing_line.start_position
                },
                self.mods.no_ssf,
            );
        }

        Ok(())
    }

    pub fn update_hit_objects(&mut self) -> Result<()> {
        // update the position of all hit objects
        // https://github.com/Quaver/Quaver/blob/develop/Quaver.Shared/Screens/Gameplay/Rulesets/Keys/HitObjects/GameplayHitObjectKeys.cs#L387
        for hit_object in &mut self.hit_objects {
            let Some(group_id) = hit_object.timing_group.as_ref() else {
                warn!(
                    "Hit object at time {} has no timing group",
                    hit_object.start_time
                );
                continue;
            };

            let Some(timing_group) = self.timing_groups.get_mut(group_id) else {
                warn!(
                    "Timing group '{}' not found for hit object at time {}",
                    group_id, hit_object.start_time
                );
                continue;
            };

            while hit_object.previous_positions.len() < 10 {
                // ensure we have at least 10 previous positions
                hit_object
                    .previous_positions
                    .push_front(hit_object.position);
            }

            hit_object
                .previous_positions
                .push_front(hit_object.position);
            if hit_object.previous_positions.len() > 10 {
                hit_object.previous_positions.pop_back();
            }

            hit_object.position = timing_group.get_object_position(
                hit_object.hit_position,
                if self.mods.no_sv {
                    (hit_object.start_time * TRACK_ROUNDING) as Position
                } else {
                    hit_object.start_position
                },
                self.mods.no_ssf,
            );

            hit_object.position_tail = timing_group.get_object_position(
                hit_object.hit_position,
                if self.mods.no_sv {
                    (hit_object.end_time.unwrap_or(hit_object.start_time) * TRACK_ROUNDING) as Position
                } else {
                    hit_object.start_position_tail
                },
                self.mods.no_ssf,
            );

        }

        Ok(())
    }

    pub const fn get_key_count(&self, include_scratch: bool) -> i64 {
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
pub struct TimingLine {
    #[serde(default)]
    pub start_time: Time, // time when the timing line reaches the receptor
    #[serde(default)]
    pub start_position: Position, // the timing line's position offset on track
    #[serde(default)]
    pub current_track_position: Position, // track position; >0 = hasnt passed receptors
    #[serde(skip)]
    pub hit_position: f64, // position of the timing line on the screen
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "PascalCase")]
pub struct TimingPoint {
    // sets values until next timing point
    // used for bpm change, or affine
    #[serde(default)]
    pub start_time: Time, // start time (ms)
    pub bpm: f64,
    pub time_signature: Option<TimeSignature>,
    #[serde(default)]
    pub hidden: bool, // show timing lines
}

impl HasStartTime for TimingPoint {
    fn start_time(&self) -> Time {
        self.start_time
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "PascalCase")]
pub struct ControlPoint {
    // represents either an SV or SSF point
    #[serde(default)]
    pub start_time: Time,
    #[serde(default)]
    pub multiplier: f64,
    #[serde(skip_deserializing)]
    pub length: Option<Time>, // none if last point
    #[serde(skip_deserializing)]
    pub cumulative_position: Position, // cumulative distance from the start of the map
}

impl HasStartTime for ControlPoint {
    fn start_time(&self) -> Time {
        self.start_time
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "PascalCase")]
pub struct HitObject {
    // a note
    #[serde(default)]
    pub start_time: Time,
    pub end_time: Option<Time>, // if Some, then its an LN
    pub lane: i64,
    pub key_sounds: Vec<KeySound>, // key sounds to play when this object is hit
    #[serde(default)]
    pub timing_group: Option<String>,
    #[serde(skip)]
    pub snap_index: usize, // index for snap color
    #[serde(skip)]
    pub hit_position: f64, // where the note is "hit", calculated from hit body height and hit position offset
    #[serde(skip)]
    pub start_position: Position, // track position at start_time (in timing group)
    #[serde(skip)]
    pub start_position_tail: Position, // track position at start_time for LN end
    #[serde(skip)]
    pub position: Position, // live map position, calculated with timing group
    #[serde(skip)]
    pub position_tail: Position, // live position of the LN end
    #[serde(skip)]
    pub previous_positions: VecDeque<Position>, // previous positions, used for rendering effects
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
pub struct KeySound {
    pub sample: i32, // the one-based index of the sound sample in the CustomAudioSamples array
    pub volume: i32, // the volume of the sound sample (defaults to 100)
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "PascalCase")]
pub struct TimingGroup {
    // group of hitobjects with seperate effects
    #[serde(default = "one_f64")]
    pub initial_scroll_velocity: f64,
    #[serde(default)]
    pub scroll_velocities: Vec<ControlPoint>,
    #[serde(default)]
    pub scroll_speed_factors: Vec<ControlPoint>,
    pub color_rgb: Option<String>,
    // info for playback
    #[serde(skip)]
    pub current_track_position: Position, // current playback position
    #[serde(default = "one_f64")]
    pub current_ssf_factor: f64, // current SSF multiplier
    #[serde(skip)]
    pub scroll_speed: f64, // speed at which objects travel across the screen
}

impl TimingGroup {
    pub fn get_scroll_speed_factor_from_time(&self, time: Time) -> f64 {
        // gets the SSF multiplier at a time, with linear interpolation
        let ssf_index = index_at_time(&self.scroll_speed_factors, time);

        match ssf_index {
            None => {
                // before first SSF point or no SSFs, so no effect applied
                1.0
            }
            Some(index) => {
                let ssf = &self.scroll_speed_factors[index];
                if index == self.scroll_speed_factors.len() - 1 {
                    // last point, no interpolation
                    return ssf.multiplier;
                }

                let next_ssf = &self.scroll_speed_factors[index + 1];
                // lerp between this and next point based on time between
                lerp(
                    ssf.multiplier,
                    next_ssf.multiplier,
                    (time - ssf.start_time) / (next_ssf.start_time - ssf.start_time),
                )
            }
        }
    }

    pub fn get_position_from_time(&self, time: Time, ignore_sv: bool) -> Position {
        // calculates the timing group's track position with time and SV
        if ignore_sv {
            return (time * TRACK_ROUNDING) as Position;
        }

        let sv_index = index_at_time(&self.scroll_velocities, time);

        match sv_index {
            None => {
                // before first SV point or no SVs, so use initial scroll velocity
                (time * self.initial_scroll_velocity * TRACK_ROUNDING) as Position
            }
            Some(index) => {
                // get the track position at the start of the current SV point
                let mut current_position = self.scroll_velocities[index].cumulative_position;

                // add the distance between the start of the current SV point and the time
                current_position += ((time - self.scroll_velocities[index].start_time)
                    * self.scroll_velocities[index].multiplier
                    * TRACK_ROUNDING) as Position;
                current_position
            }
        }
    }

    pub fn get_object_position(&self, hit_position: f64, initial_position: Position, ignore_ssf: bool) -> Position {
        // calculates the position of a hit object with a position offset
        // note: signs were swapped in quaver?
        let mut scroll_speed = if SKIN.downscroll {
            -self.scroll_speed
        } else {
            self.scroll_speed
        };

        if !ignore_ssf {
            // apply SSF factor
            scroll_speed *= self.current_ssf_factor;
        }

        let distance = (initial_position as f64) - (self.current_track_position as f64);
        let position = hit_position + (distance * scroll_speed / TRACK_ROUNDING);
        position as Position
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

#[derive(Serialize, Deserialize, Debug, Clone, Copy)]
#[serde(rename_all = "PascalCase")]
pub enum TimeSignature {
    Quadruple = 4,
    Triple = 3,
}

pub const fn one_f64() -> f64 {
    1.0
}
