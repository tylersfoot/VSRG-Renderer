use std::collections::{HashMap, VecDeque};

use vsrg_renderer::constants::{FieldPositions, DEFAULT_TIMING_GROUP_ID, SKIN};
use vsrg_renderer::map::{
    ControlPoint, HitObject, Map, Mods, TimingPoint, TimeSignature,
};

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

fn default_hit_object(time: f64) -> HitObject {
    HitObject {
        start_time: time,
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
    }
}

fn create_map_with_params(
    note_times: &[f64],
    sv: Vec<ControlPoint>,
    ssf: Vec<ControlPoint>,
) -> (Map, FieldPositions) {
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
        scroll_velocities: sv,
        scroll_speed_factors: ssf,
        hit_objects: note_times.iter().map(|&t| default_hit_object(t)).collect(),
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
    map
        .initialize_hit_objects(&field_positions)
        .unwrap_or_else(|e| {
            panic!("Failed to initialize hit objects: {e}");
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

#[test]
fn test_initialize_beat_snaps() {
    let note_times = [0.0, 20.0, 62.5, 125.0, 250.0];
    let (mut map, _) = create_map_with_params(&note_times, Vec::new(), Vec::new());
    let _ = map.initialize_beat_snaps();

    let snaps: Vec<usize> = map.hit_objects.iter().map(|h| h.snap_index).collect();
    assert_eq!(snaps, vec![0, 8, 5, 3, 1]);
}

#[test]
fn test_initialize_beat_snaps_no_timing_points() {
    let (mut map, _) = create_basic_map();
    map.timing_points.clear();

    let result = map.initialize_beat_snaps();
    assert!(result.is_err());
}

#[test]
fn test_update_hit_objects() {
    let sv = vec![ControlPoint {
        start_time: 1000.0,
        multiplier: 2.0,
        length: None,
        cumulative_position: 0,
    }];
    let ssf = vec![ControlPoint {
        start_time: 0.0,
        multiplier: 2.0,
        length: None,
        cumulative_position: 0,
    }];

    let (mut map, _) = create_map_with_params(&[1500.0], sv, ssf);
    map.update_scroll_speed();

    // SV and SSF enabled
    map.mods.no_sv = false;
    map.mods.no_ssf = false;
    map.update_track_position(0.0);
    map.update_hit_objects().unwrap();
    let pos_with_all = map.hit_objects[0].position;
    let tail_with_all = map.hit_objects[0].position_tail;

    // no SV
    map.mods.no_sv = true;
    map.mods.no_ssf = false;
    map.update_track_position(0.0);
    map.update_hit_objects().unwrap();
    let pos_no_sv = map.hit_objects[0].position;
    let tail_no_sv = map.hit_objects[0].position_tail;

    // no SSF
    map.mods.no_sv = false;
    map.mods.no_ssf = true;
    map.update_track_position(0.0);
    map.update_hit_objects().unwrap();
    let pos_no_ssf = map.hit_objects[0].position;
    let tail_no_ssf = map.hit_objects[0].position_tail;

    // neither SV nor SSF
    map.mods.no_sv = true;
    map.mods.no_ssf = true;
    map.update_track_position(0.0);
    map.update_hit_objects().unwrap();
    let pos_none = map.hit_objects[0].position;
    let tail_none = map.hit_objects[0].position_tail;

    assert_eq!(pos_with_all, -8995);
    assert_eq!(tail_with_all, -8995);
    assert_eq!(pos_no_sv, -6746);
    assert_eq!(tail_no_sv, -6746);
    assert_eq!(pos_no_ssf, -4497);
    assert_eq!(tail_no_ssf, -4497);
    assert_eq!(pos_none, -3373);
    assert_eq!(tail_none, -3373);
}
