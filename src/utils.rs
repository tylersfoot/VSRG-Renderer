
use macroquad::color::Color;
use std::fmt;
// use serde::{Deserialize, Serialize};

pub const DEFAULT_TIMING_GROUP_ID: &str = "$Default";
// pub const GLOBAL_TIMING_GROUP_ID: &str = "$Global";

// rounding for track positions, for int/float conversion - 100.0 for Quaver compatibility
pub const TRACK_ROUNDING: f64 = 100.0;

#[derive(Debug, Clone)]
pub struct FieldPositions {
    // positions from top of screen
    pub receptor_position_y: f64,    // receptors position
    pub hit_position_y: f64,         // hit object target position
    pub timing_line_position_y: f64, // timing line position
}

pub struct BeatSnap {
    pub divisor: u32, // e.g. 4 for 1/4 notes, 6 for 1/6 notes
    pub color: Color,
}

// snap colors
pub const BEAT_SNAPS: &[BeatSnap] = &[
    BeatSnap { divisor: 48, color: Color::new(255.0 / 255.0, 96.0  / 255.0, 96.0  / 255.0, 1.0) }, // 1st (red)
    BeatSnap { divisor: 24, color: Color::new(61.0  / 255.0, 132.0 / 255.0, 255.0 / 255.0, 1.0) }, // 2nd (blue)
    BeatSnap { divisor: 16, color: Color::new(178.0 / 255.0, 71.0  / 255.0, 255.0 / 255.0, 1.0) }, // 3rd (purple)
    BeatSnap { divisor: 12, color: Color::new(255.0 / 255.0, 238.0 / 255.0, 58.0  / 255.0, 1.0) }, // 4th (yellow)
    BeatSnap { divisor: 8,  color: Color::new(255.0 / 255.0, 146.0 / 255.0, 210.0 / 255.0, 1.0) }, // 6th (pink)
    BeatSnap { divisor: 6,  color: Color::new(255.0 / 255.0, 167.0 / 255.0, 61.0  / 255.0, 1.0) }, // 8th (orange)
    BeatSnap { divisor: 4,  color: Color::new(132.0 / 255.0, 255.0 / 255.0, 255.0 / 255.0, 1.0) }, // 12th (cyan)
    BeatSnap { divisor: 3,  color: Color::new(127.0 / 255.0, 255.0 / 255.0, 138.0 / 255.0, 1.0) }, // 16th (green)
    BeatSnap { divisor: 1,  color: Color::new(200.0 / 255.0, 200.0 / 255.0, 200.0 / 255.0, 1.0) }, // 48th (gray) + fallback
];

#[derive(Debug, Clone)]
pub struct Skin {
    // skin settings
    pub note_shape: &'static str,  // shape of the notes ("circles", "bars")
    pub lane_width: f64,           // width of each lane/column
    pub note_width: f64,           // width of each note
    pub note_height: f64,          // height of each note
    pub receptors_y_position: f64, // y position of the receptors/hit line
    pub scroll_speed: f64,         // scroll speed of the notes
    pub wide_timing_lines: bool,   // whether to draw timing lines to the sides of the screen
    pub downscroll: bool,          // downscroll (true) or upscroll (false)
    pub normalize_scroll_velocity_by_rate_percentage: usize, // percentage of scaling applied when changing rates
    pub offset: f64,               // audio offset in milliseconds
}


pub const SKIN: Skin = Skin {
    note_shape: "bars",
    lane_width: 145.0,           // 136
    note_width: 145.0,           // 136
    note_height: 36.0,           // 36
    receptors_y_position: 226.0, // 226
    scroll_speed: 320.0,         // 200 = 20 in quaver
    wide_timing_lines: true,
    downscroll: true,
    normalize_scroll_velocity_by_rate_percentage: 100,
    offset: -50.0,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum JudgementType {
    Marvelous,
    Perfect,
    Great,
    Good,
    Okay,
    Miss,
}

#[derive(Debug, Clone)]
pub struct Judgement {
    pub kind: JudgementType,
    pub window: f64, // hit window in ms
}


impl fmt::Display for JudgementType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            JudgementType::Marvelous => "Marvelous",
            JudgementType::Perfect => "Perfect",
            JudgementType::Great => "Great",
            JudgementType::Good => "Good",
            JudgementType::Okay => "Okay",
            JudgementType::Miss => "Miss",
        };
        write!(f, "{}", s)
    }
}

pub const JUDGEMENTS: &[Judgement] = &[
    Judgement { kind: JudgementType::Marvelous, window: 18.0 },
    Judgement { kind: JudgementType::Perfect,   window: 43.0 },
    Judgement { kind: JudgementType::Great,     window: 76.0 },
    Judgement { kind: JudgementType::Good,      window: 106.0 },
    Judgement { kind: JudgementType::Okay,       window: 127.0 },
    Judgement { kind: JudgementType::Miss,      window: 164.0 },
];


// anything representing a time in milliseconds
pub type Time = f64;

// for objects with a start time
pub trait HasStartTime {
    fn start_time(&self) -> Time;
}

// linear interpolation between a and b based on time t
pub fn lerp(a: f64, b: f64, t: f64) -> f64 {
    a + (b - a) * t
}

// returns index of currently active item (start_time <= time)
pub fn index_at_time<T: HasStartTime>(list: &[T], time: Time) -> Option<usize> {
    match list.binary_search_by(|item| item.start_time().partial_cmp(&time).unwrap()) {
        Ok(mut idx) => {
            while idx + 1 < list.len() && list[idx + 1].start_time() <= time {
                idx += 1;
            }
            Some(idx)
        }
        Err(0) => None,
        Err(idx) => Some(idx - 1),
    }
}

// returns currently active item (start_time <= time)
pub fn object_at_time<T: HasStartTime>(list: &[T], time: Time) -> Option<&T> {
    index_at_time(list, time).map(|i| &list[i])
}

// sorts a vector of items by their start time
pub fn sort_by_start_time<T: HasStartTime>(items: &mut [T]) {
    items.sort_by(|a, b| a.start_time().partial_cmp(&b.start_time()).unwrap());
}
