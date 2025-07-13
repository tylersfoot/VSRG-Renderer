use crate::utils::{FieldPositions, BEAT_SNAPS, SKIN, JudgementType};
use crate::draw::Draw;
use crate::map::Map;
// use crate::index_at_time;
use anyhow::Result;
use macroquad::{color::Color, prelude::*};

pub struct FrameState<'map> {
    pub map: &'map mut Map,
    pub field_positions: &'map FieldPositions<'map>,
}

pub fn set_reference_positions(receptor_texture: &'_ Texture2D) -> FieldPositions<'_> {
    let mut field_positions = FieldPositions {
        receptor_position_y: 0.0,
        hit_position_y: 0.0,
        timing_line_position_y: 0.0,
        receptor_texture,
    };

    if SKIN.downscroll {
        field_positions.receptor_position_y = -SKIN.receptors_y_position;
        field_positions.hit_position_y = field_positions.receptor_position_y;
        field_positions.timing_line_position_y = field_positions.receptor_position_y;
    } else {
        // i dont care about upscroll right now
    }

    field_positions
}

pub fn render_frame(state: &mut FrameState, draw: &mut impl Draw) -> Result<()> {
    // calculates the positions of all objects and renders the current frame given the framestate

    // update functions
    state.map.update_track_position(state.map.time);
    state.map.update_scroll_speed();
    state.map.update_timing_lines()?;
    state.map.update_hit_objects()?;

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
            // draw.draw_line(
            //     0.0,
            //     window_height + state.field_positions.receptor_position_y,
            //     window_width,
            //     window_height + state.field_positions.receptor_position_y,
            //     3.0,
            //     GRAY,
            // );
            draw.draw_texture(
                state.field_positions.receptor_texture,
                0.0,
                window_height + state.field_positions.receptor_position_y * 1.88,
                WHITE,
            )
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
    // let first = index_at_time(&state.map.hit_objects, state.map.time)
    //     .unwrap_or(0); // first note to render
    for index in 0..state.map.hit_objects.len() {
        let note = &state.map.hit_objects[index];
        // skip note if hit
        if note.hit {
            continue;
        }
        let is_long_note = note.end_time.is_some();
        
        // calculate x position based on lane (1-indexed in quaver)
        // adjust lane to be 0-indexed for calculation
        let lane_index = if state.map.mods.mirror {
            num_lanes - note.lane
        } else {
            note.lane - 1
        };

        if state.map.mods.autoplay
            && (note.start_time <= state.map.time)
            && (!is_long_note || note.end_time.unwrap() <= state.map.time) {
            // past receptors in autoplay mode = hit note perfectly
            state.map.handle_gameplay_key_press(note.start_time, lane_index);
            continue;
        }
        if state.map.time - note.start_time >= state.map.judgement_windows[&JudgementType::Miss] {
            *state.map.judgement_counts.get_mut(&JudgementType::Miss).unwrap() += 1;
            state.map.hit_objects[index].hit = true;
            state.map.last_judgement = Some((JudgementType::Miss, state.map.time, 0.0));
            state.map.combo = 0;
            continue;
        }

        let is_held = note.start_time <= state.map.time;

        // real hitbox
        let note_y = if is_long_note && is_held {
            // held notes are rendered at the receptors
            state.field_positions.receptor_position_y + window_height
        } else {
            (note.position as f64) + window_height
        };

        let note_tail_y = (note.position_tail as f64) + window_height; // long note end position

        let note_x = playfield_x
            + (lane_index as f64 * SKIN.lane_width)
            + (SKIN.lane_width / 2f64) // center in lane
            - (SKIN.note_width / 2f64);

        let half_note_height = SKIN.note_height / 2f64;

        let mut note_top_offset = half_note_height;
        let mut note_bottom_offset = half_note_height;
        let middle_position = note_y - half_note_height;
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
                if is_long_note {
                    let height = note_tail_y - note_y;
                    draw.draw_rectangle(
                        note_x,
                        note_y, // bottom of ln
                        SKIN.note_width,
                        height, // top/height of ln
                        DARKGRAY,
                    );
                }
                draw.draw_rectangle(
                    note_x,
                    middle_position - note_top_offset, // bottom of note
                    SKIN.note_width,
                    note_top_offset + note_bottom_offset, // top/height of note
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
                // if let Some(end_y) = long_end_y {
                //     let top = note_y.min(end_y);
                //     let height = (end_y - note_y).abs();
                //     let center_x = note_x + (SKIN.note_width / 2.0);
                //     draw.draw_rectangle(center_x - 2.0, top, 4.0, height, color);
                // }
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

    Ok(())
}
