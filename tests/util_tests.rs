use vsrg_renderer::{index_at_time, lerp, object_at_time, HasStartTime, Time};

#[derive(Clone)]
struct Item {
    start: Time,
}

impl HasStartTime for Item {
    fn start_time(&self) -> Time {
        self.start
    }
}

#[test]
fn test_lerp() {
    assert_eq!(lerp(0.0, 10.0, 0.0), 0.0);
    assert_eq!(lerp(0.0, 10.0, 1.0), 10.0);
    assert_eq!(lerp(0.0, 10.0, 0.5), 5.0);
}

#[test]
fn test_index_at_time() {
    let list = vec![
        Item { start: 10.0 },
        Item { start: 20.0 },
        Item { start: 30.0 },
    ];

    assert_eq!(index_at_time(&list, 5.0), None);
    assert_eq!(index_at_time(&list, 10.0), Some(0));
    assert_eq!(index_at_time(&list, 15.0), Some(0));
    assert_eq!(index_at_time(&list, 25.0), Some(1));
    assert_eq!(index_at_time(&list, 30.0), Some(2));
    assert_eq!(index_at_time(&list, 35.0), Some(2));
}

#[test]
fn test_object_at_time() {
    let list = vec![
        Item { start: 10.0 },
        Item { start: 20.0 },
        Item { start: 30.0 },
    ];

    assert!(object_at_time(&list, 5.0).is_none());
    assert_eq!(object_at_time(&list, 10.0).unwrap().start, 10.0);
    assert_eq!(object_at_time(&list, 15.0).unwrap().start, 10.0);
    assert_eq!(object_at_time(&list, 25.0).unwrap().start, 20.0);
    assert_eq!(object_at_time(&list, 30.0).unwrap().start, 30.0);
    assert_eq!(object_at_time(&list, 35.0).unwrap().start, 30.0);
}


#[cfg(test)]
mod map_tests {
    use vsrg_renderer::map::{GameMode, Map, Mods, TimingGroup};
    use std::collections::HashMap;

    fn minimal_map(mode: GameMode, has_scratch_key: bool) -> Map {
        Map {
            audio_file: None,
            song_preview_time: None,
            background_file: None,
            banner_file: None,
            map_id: None,
            map_set_id: None,
            mode,
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
            has_scratch_key,
            editor_layers: Vec::new(),
            bookmarks: Vec::new(),
            custom_audio_samples: Vec::new(),
            timing_points: Vec::new(),
            timing_lines: Vec::new(),
            scroll_velocities: Vec::new(),
            scroll_speed_factors: Vec::new(),
            hit_objects: Vec::new(),
            timing_groups: HashMap::new(),
            file_path: String::new(),
            time: 0.0,
            rate: 1.0,
            mods: Mods::default(),
            length: 0.0,
        }
    }

    #[test]
    fn test_get_key_count() {
        let map = minimal_map(GameMode::Keys4, false);
        assert_eq!(map.get_key_count(true), 4);
        let map = minimal_map(GameMode::Keys4, true);
        assert_eq!(map.get_key_count(false), 4);
        assert_eq!(map.get_key_count(true), 5);
    }

    #[test]
    fn test_timing_group_positions() {
        let tg = TimingGroup {
            initial_scroll_velocity: 1.0,
            scroll_velocities: Vec::new(),
            scroll_speed_factors: Vec::new(),
            color_rgb: None,
            current_track_position: 0,
            current_ssf_factor: 1.0,
            scroll_speed: 2.0,
        };

        // with no SV points, position should be time * sv * rounding
        assert_eq!(tg.get_position_from_time(1.5, false), (1.5 * 1.0 * 100.0) as i64);

        // object position with downscroll true, scroll_speed 2, no SSF
        let pos = tg.get_object_position(50.0, 100, true);
        // downscroll = true => scroll_speed negated
        let expected = 50.0 + ((100.0 - 0.0) * -2.0 / 100.0);
        assert_eq!(pos, expected as i64);
    }
}