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
