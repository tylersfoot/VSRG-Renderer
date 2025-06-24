use macroquad::color::Color;
// use serde::{Deserialize, Serialize};

pub const DEFAULT_TIMING_GROUP_ID: &str = "$Default";
// pub const GLOBAL_TIMING_GROUP_ID: &str = "$Global";

// rounding for track positions, for int/float conversion
pub const TRACK_ROUNDING: f64 = 100.0;

#[derive(Debug, Clone)]
pub struct FieldPositions {
    // positions from top of screen
    pub receptor_position_y: f64,       // receptors position
    pub hit_position_y: f64,            // hit object target position
    pub hold_hit_position_y: f64,       // held hit object target position
    pub hold_end_hit_position_y: f64,   // LN end target position
    pub timing_line_position_y: f64,    // timing line position
    pub long_note_size_adjustment: f64, // size adjustment for LN so LN end time snaps with start time
}

pub struct BeatSnap {
    pub divisor: u32, // e.g. 4 for 1/4 notes, 6 for 1/6 notes
    pub color: Color,
}

// snap colors
pub const BEAT_SNAPS: &[BeatSnap] = &[
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

#[derive(Debug, Clone)]
pub struct Skin {
    // skin settings
    pub note_shape: &'static str,  // shape of the notes ("circles", "bars")
    pub lane_width: f64,           // width of each lane/column
    pub note_width: f64,           // width of each note
    pub note_height: f64,          // height of each note
    pub receptors_y_position: f64, // y position of the receptors/hit line
    pub scroll_speed: f64,         // scroll speed of the notes
    // pub rate_affects_scroll_speed: bool, // whether the rate multiplies the scroll speed
    // pub draw_lanes: bool,          // whether to draw the lanes
    pub wide_timing_lines: bool,   // whether to draw timing lines to the sides of the screen
    pub downscroll: bool,          // downscroll (true) or upscroll (false)
    pub normalize_scroll_velocity_by_rate_percentage: usize, // percentage of scaling applied when changing rates
}


pub const SKIN: Skin = Skin {
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
