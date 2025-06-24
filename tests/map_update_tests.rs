use std::collections::{HashMap, VecDeque};

use vsrg_renderer::constants::{FieldPositions, DEFAULT_TIMING_GROUP_ID, SKIN};
use vsrg_renderer::map::{HitObject, Map, Mods, TimingPoint, TimeSignature};

fn create_basic_map() -> (Map, FieldPositions) {
    let mut map = Map {
        audio_file: None,
        song_preview_time: None,
        background_file: None,
        banner_file: None,
        map_id: None,
        map_set_id: None,
        mode: Default::default(),
        title: None,
        artist: None,
        source: None,
        tags: None,
        creator: None,
        difficulty_name: None,
        description: None,
        genre: None,
        legacy_ln_rendering: false,
        bpm_does_not_affect_scroll_velocity: false,
        initial_scroll_velocity: 1.0,
        has_scratch_key: false,
        editor_layers: vec![],
        bookmarks: vec![],
        custom_audio_samples: vec![],
        timing_points: vec![TimingPoint {
            start_time: 0.0,
            bpm: 120.0,
            time_signature: Some(TimeSignature::Quadruple),
            hidden: false,
        }],
        timing_lines: Vec::new(),
        scroll_velocities: Vec::new(),
        scroll_speed_factors: Vec::new(),
        hit_objects: vec![HitObject {
            start_time: 500.0,
            end_time: None,
            lane: 0,
            key_sounds: vec![],
            timing_group: None,
            snap_index: 0,
            hit_position: 0.0,
            start_position: 0,
            start_position_tail: 0,
            position: 0,
            position_tail: 0,
            previous_positions: VecDeque::new(),
        }],
        timing_groups: HashMap::new(),
        file_path: String::new(),
        time: 0.0,
        rate: 1.0,
        mods: Mods {
            mirror: false,
            no_sv: false,
            no_ssf: false,
        },
        length: 2500.0,
    };

    let field_positions = FieldPositions {
        receptor_position_y: 0.0,
        hit_position_y: 0.0,
        timing_line_position_y: 0.0,
    };

    map.initialize_default_timing_group();
    map.initialize_control_points();
    map.initialize_hit_objects(&field_positions)
        .unwrap_or_else(|e| {
            log::error!("Failed to initialize hit objects: {e}");
            panic!("Failed to initialize hit objects: {e}");
        });
    map.initialize_timing_lines(&field_positions)
        .unwrap_or_else(|e| {
            log::error!("Failed to initialize timing lines: {e}");
            panic!("Failed to initialize timing lines: {e}");
        });

    (map, field_positions)
}

#[test]
fn test_update_track_position() {
    let (mut map, _) = create_basic_map();

    map.update_track_position(0.0);
    let tg = &map.timing_groups[DEFAULT_TIMING_GROUP_ID];
    assert_eq!(tg.current_track_position, 0);

    map.update_track_position(1000.0);
    let tg = &map.timing_groups[DEFAULT_TIMING_GROUP_ID];
    assert_eq!(tg.current_track_position, 100_000);
}

#[test]
fn test_update_scroll_speed() {
    let (mut map, _) = create_basic_map();
    map.update_scroll_speed();
    let tg = &map.timing_groups[DEFAULT_TIMING_GROUP_ID];

    let speed = SKIN.scroll_speed;
    let rate_scaling =
        1f64 + (map.rate - 1f64) * (SKIN.normalize_scroll_velocity_by_rate_percentage as f64 / 100f64);
    let adjusted = (speed * rate_scaling).clamp(50.0, 1000.0);
    let scaling_factor = 1920f64 / 1366f64;
    let expected = (adjusted / 10f64) / (20f64 * map.rate) * scaling_factor;

    assert!((tg.scroll_speed - expected).abs() < 1e-6);
}

#[test]
fn test_update_timing_lines() {
    let (mut map, _) = create_basic_map();

    map.update_scroll_speed();
    map.update_track_position(1000.0);
    let _ = map.update_timing_lines();

    assert_eq!(map.timing_lines.len(), 2);

    let first = map.timing_lines[0].current_track_position;
    let second = map.timing_lines[1].current_track_position;

    assert_eq!(first, 2248);
    assert_eq!(second, -2248);
}