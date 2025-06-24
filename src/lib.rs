// anything representing a time in milliseconds
pub type Time = f64;

// For objects with a start time
pub trait HasStartTime {
    fn start_time(&self) -> Time;
}

/// Linear interpolation between a and b based on time t
pub fn lerp(a: f64, b: f64, t: f64) -> f64 {
    a + (b - a) * t
}

/// Returns index of currently active item (start_time <= time)
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

/// Returns currently active item (start_time <= time)
pub fn object_at_time<T: HasStartTime>(list: &[T], time: Time) -> Option<&T> {
    index_at_time(list, time).map(|i| &list[i])
}

/// Sorts a vector of items by their start time
pub fn sort_by_start_time<T: HasStartTime>(items: &mut [T]) {
    items.sort_by(|a, b| a.start_time().partial_cmp(&b.start_time()).unwrap());
}


#[cfg(feature = "audio")]
pub mod audio_manager;
#[cfg(not(feature = "audio"))]
pub mod audio_manager_stub;
pub mod constants;
pub mod map;

#[cfg(feature = "audio")]
pub use audio_manager::AudioManager;
#[cfg(not(feature = "audio"))]
pub use audio_manager_stub as audio_manager;
#[cfg(not(feature = "audio"))]
pub use audio_manager_stub::AudioManager;
