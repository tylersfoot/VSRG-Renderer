use std::fs::File;
use std::io::Write;
use std::path::PathBuf;

use vsrg_renderer::audio_manager::AudioManager;

fn write_silence_wav(path: &PathBuf, samples: u32) {
    let subchunk2_size = samples * 2;
    let chunk_size = 36 + subchunk2_size;
    let mut file = File::create(path).unwrap();

    file.write_all(b"RIFF").unwrap();
    file.write_all(&(chunk_size).to_le_bytes()).unwrap();
    file.write_all(b"WAVE").unwrap();

    file.write_all(b"fmt ").unwrap();
    file.write_all(&16u32.to_le_bytes()).unwrap();
    file.write_all(&1u16.to_le_bytes()).unwrap();
    file.write_all(&1u16.to_le_bytes()).unwrap();
    file.write_all(&44100u32.to_le_bytes()).unwrap();
    file.write_all(&(44100u32 * 2).to_le_bytes()).unwrap();
    file.write_all(&2u16.to_le_bytes()).unwrap();
    file.write_all(&16u16.to_le_bytes()).unwrap();

    file.write_all(b"data").unwrap();
    file.write_all(&(subchunk2_size).to_le_bytes()).unwrap();
    file.write_all(&vec![0u8; subchunk2_size as usize]).unwrap();
}

#[test]
fn test_seek_ms_updates_time() {
    let path = std::env::temp_dir().join("seek_test.wav");
    write_silence_wav(&path, 44100); // 1 second

    let mut manager = AudioManager::new().expect("create audio manager");
    manager.set_audio_path(Some(path.clone()));

    manager.seek_ms(500.0);
    let t = manager.get_current_song_time_ms();
    assert!((t - 500.0).abs() < 1.0);

    manager.seek_ms(800.0);
    let t = manager.get_current_song_time_ms();
    assert!((t - 800.0).abs() < 1.0);

    manager.seek_ms(1500.0);
    let total = manager.get_total_duration_ms().unwrap();
    let t = manager.get_current_song_time_ms();
    assert!((t - total).abs() < 1.0);
}
