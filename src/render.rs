use crate::constants::{FieldPositions, SKIN, BEAT_SNAPS};
use crate::draw::Draw;
use crate::map::Map;
use crate::index_at_time;
use macroquad::{color::*};

pub struct FrameState<'map> {
    pub map: &'map mut Map,
    pub field_positions: &'map FieldPositions,
}

pub fn set_reference_positions() -> FieldPositions {
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

    field_positions
}

pub fn render_frame(state: &mut FrameState, draw: &mut impl Draw) {
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
    let first = index_at_time(&state.map.hit_objects, state.map.time)
        .unwrap_or(0); // first note to render
    for index in first..state.map.hit_objects.len() {
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
