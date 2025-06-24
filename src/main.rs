#[cfg(feature = "audio")]
mod audio_manager;
#[cfg(not(feature = "audio"))]
mod audio_manager_stub;
mod constants;
mod draw;
mod map;
mod render;

#[cfg(feature = "audio")]
use audio_manager::AudioManager;
#[cfg(not(feature = "audio"))]
use audio_manager_stub::AudioManager;
use draw::MacroquadDraw;
use map::Map;
use render::{render_frame, set_reference_positions, FrameState};
use vsrg_renderer::{index_at_time, lerp, object_at_time, sort_by_start_time, HasStartTime, Time};

use anyhow::Result;
use clap::Parser;
use core::f64;
use macroquad::prelude::*;

use std::{
    fs::{self, File},
    io::{Error, Write as _},
    path::{Path, PathBuf},
    string::ToString,
    time::Instant,
};

#[derive(Parser, Debug, Clone)]
#[command(author, version, about = "VSRG Renderer")]
struct CliArgs {
    /// Directory containing the map (.qua) file
    map_dir: PathBuf,

    /// Start in fullscreen
    #[arg(long)]
    fullscreen: bool,

    /// Playback rate
    #[arg(long, default_value_t = 1.0)]
    rate: f64,

    /// Initial audio volume
    #[arg(long, default_value_t = 0.03)]
    volume: f64,

    /// Mirror notes horizontally
    #[arg(long)]
    mirror: bool,

    /// Ignore scroll velocities
    #[arg(long)]
    no_sv: bool,

    /// Ignore scroll speed factors
    #[arg(long)]
    no_ssf: bool,
}

fn window_conf() -> Conf {
    let args = CliArgs::parse();
    Conf {
        window_title: "VSRG Renderer".to_string(),
        window_width: 1000,
        window_height: 1200,
        fullscreen: args.fullscreen,
        ..Default::default()
    }
}

#[macroquad::main(window_conf)]
pub async fn main() -> anyhow::Result<()> {
    simple_logger::init().unwrap();
    let args = CliArgs::parse();
    let mut is_fullscreen = args.fullscreen;

    // --- audio setup ---
    let mut audio_manager = AudioManager::new().map_err(|e| {
        log::error!("Critical audio error on init: {e}");
        Error::other(e)
    })?;

    audio_manager.set_rate(args.rate);
    audio_manager.set_volume(args.volume);

    // --- map loading ---
    let song_name = args.map_dir;
    let project_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let map_folder_path = project_dir.join("songs/").join(song_name);
    let map_file_name_option = fs::read_dir(&map_folder_path)
        .map_err(|e| anyhow::anyhow!("Failed to read map directory {:?}: {}", map_folder_path, e))?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .find(|path| path.extension().is_some_and(|ext| ext == "qua"))
        .and_then(|path| path.to_str().map(ToString::to_string));

    let Some(map_file_name) = map_file_name_option else {
        let err_msg = format!(
            "No .qua file found in directory {}",
            map_folder_path.display()
        );
        log::error!("{err_msg}");
        anyhow::bail!(err_msg);
    };

    log::info!("Loading map: {map_file_name}");
    let qua_file_content = fs::read_to_string(&map_file_name)
        .map_err(|e| anyhow::anyhow!("Failed to read map file '{}': {}", map_file_name, e))?;

    let mut map: Map = serde_yaml::from_str(&qua_file_content)
        .map_err(|e| anyhow::anyhow!("Failed to parse map data from '{}': {}", map_file_name, e))?;

    // set audio path in audio manager
    if let Some(audio_filename_str) = &map.audio_file {
        let map_dir = Path::new(&map_file_name)
            .parent()
            .unwrap_or_else(|| Path::new(""));
        let current_audio_path = map_dir.join(audio_filename_str);
        audio_manager.set_audio_path(Some(current_audio_path));
    } else {
        audio_manager.set_audio_path(None);
    }

    map.length = audio_manager.get_total_duration_ms().unwrap_or(0f64);
    map.rate = audio_manager.get_rate();
    map.mods.mirror = args.mirror;
    map.mods.no_sv = args.no_sv;
    map.mods.no_ssf = args.no_ssf;

    // map processing functions / preload
    let field_positions = set_reference_positions();
    map.initialize_default_timing_group();
    map.sort();
    map.initialize_control_points();
    map.initialize_hit_objects(&field_positions).map_err(|e| {
        log::error!("Failed to initialize hit objects: {e}");
        e
    })?;
    map.initialize_timing_lines(&field_positions).map_err(|e| {
        log::error!("Failed to initialize timing lines: {e}");
        e
    })?;
    map.initialize_beat_snaps().map_err(|e| {
        log::error!("Failed to initialize beat snaps: {e}");
        e
    })?;

    let total_hit_objects = map.hit_objects.len();
    let total_timing_points = map.timing_points.len();
    let total_svs = map
        .timing_groups
        .values()
        .map(|g| g.scroll_velocities.len())
        .sum::<usize>();
    let total_ssfs = map
        .timing_groups
        .values()
        .map(|g| g.scroll_speed_factors.len())
        .sum::<usize>();
    let total_timing_groups = map.timing_groups.len();
    let total_timing_lines = map.timing_lines.len();
    log::info!(
        "Map loaded successfully: {total_hit_objects} Hit Objects, {total_timing_points} Timing Points, {total_svs} SVs, {total_ssfs} SSFs, {total_timing_groups} Timing Groups, {total_timing_lines} Timing Lines",
    );

    // this is the visual play state, audio is handled by audio_manager
    let mut is_playing_visuals = false;

    let mut json_output_file = File::create("output.json")?;
    let json_string = serde_json::to_string_pretty(&map)?;
    write!(json_output_file, "{json_string}")?;
    log::info!("Parsed map data written to output.json");

    let start_instant = Instant::now();
    let mut frame_count: u64 = 0;

    // main render loop
    loop {
        frame_count += 1;

        // --- inputs ---
        if is_key_pressed(KeyCode::Escape) || is_key_pressed(KeyCode::Backspace) {
            break;
        }
        if is_key_pressed(KeyCode::F11) || is_key_pressed(KeyCode::F) {
            is_fullscreen = !is_fullscreen;
            set_fullscreen(is_fullscreen);
        }
        if is_key_pressed(KeyCode::Space) {
            is_playing_visuals = !is_playing_visuals;
            if is_playing_visuals {
                audio_manager.play();
            } else {
                audio_manager.pause();
            }
        }
        if is_key_pressed(KeyCode::R) {
            is_playing_visuals = true;
            audio_manager.restart();
            audio_manager.play();
        }
        if is_key_pressed(KeyCode::Up) {
            let new_vol = (audio_manager.get_volume() + 0.05).min(1.5);
            audio_manager.set_volume(new_vol);
        }
        if is_key_pressed(KeyCode::Down) {
            let new_vol = (audio_manager.get_volume() - 0.05).max(0.0);
            audio_manager.set_volume(new_vol);
        }
        if is_key_pressed(KeyCode::Equal) {
            let new_rate = (audio_manager.get_rate() + 0.1).min(2.0);
            audio_manager.set_rate(new_rate);
            map.rate = new_rate;
        }
        if is_key_pressed(KeyCode::Minus) {
            let new_rate = (audio_manager.get_rate() - 0.1).max(0.5);
            audio_manager.set_rate(new_rate);
            map.rate = new_rate;
        }
        if is_key_pressed(KeyCode::Left) {
            let offset = 5000.0;
            let mut new_time = audio_manager.get_current_song_time_ms() - offset;
            if let Some(total) = audio_manager.get_total_duration_ms() {
                new_time = new_time.clamp(0.0, total);
            } else {
                new_time = new_time.max(0.0);
            }
            audio_manager.seek_ms(new_time);
        }
        if is_key_pressed(KeyCode::Right) {
            let offset = 5000.0;
            let mut new_time = audio_manager.get_current_song_time_ms() + offset;
            if let Some(total) = audio_manager.get_total_duration_ms() {
                new_time = new_time.clamp(0.0, total);
            } else {
                new_time = new_time.max(0.0);
            }
            audio_manager.seek_ms(new_time);
        }

        let time = audio_manager.get_current_song_time_ms();
        map.time = time;
        let mut macroquad_draw = MacroquadDraw;
        let mut frame_state = FrameState {
            map: &mut map,
            field_positions: &field_positions,
        };

        // --------- render stuff --------

        clear_background(BLACK); // resets frame to all black
        render_frame(&mut frame_state, &mut macroquad_draw).map_err(|e| {
            log::error!("Render error: {e}");
            e
        })?;

        // -------- draw ui / debug info --------

        let mut y_offset = 20.0;
        let line_height = 20.0;

        if let (Some(title), Some(artist), Some(difficulty), Some(creator)) = (
            map.title.as_ref(),
            map.artist.as_ref(),
            map.difficulty_name.as_ref(),
            map.creator.as_ref(),
        ) {
            draw_text(
                &format!("Map: {title} - {artist} [{difficulty}] by {creator}"),
                10.0,
                y_offset,
                20.0,
                WHITE,
            );
            y_offset += line_height;
        }

        draw_text(
            &format!(
                "{total_hit_objects} Notes, {total_svs} SVs, {total_ssfs} SSFs, {total_timing_groups} Groups, {total_timing_points} Timing Points, {total_timing_lines} Timing Lines",
            ),
            10.0,
            y_offset,
            20.0,
            WHITE,
        );
        y_offset += line_height;
        y_offset += line_height;

        let visual_state_text = if is_playing_visuals {
            "Playing"
        } else {
            "Paused"
        };
        let audio_actual_state_text = if audio_manager.is_playing() {
            "Playing"
        } else {
            #[cfg(feature = "audio")]
            {
                if audio_manager
                    .sink
                    .as_ref()
                    .is_some_and(rodio::Sink::is_paused)
                {
                    "Paused"
                } else {
                    "Stopped/empty"
                }
            }
            #[cfg(not(feature = "audio"))]
            {
                "Paused"
            }
        };
        draw_text(
            &format!("Visuals: {visual_state_text} | Audio: {audio_actual_state_text} (space, r)"),
            10.0,
            y_offset,
            20.0,
            WHITE,
        );
        y_offset += line_height;

        draw_text(
            &format!(
                "Volume: {:.2} (up/down) | Rate: {:.1}x (left/right)",
                audio_manager.get_volume(),
                audio_manager.get_rate()
            ),
            10.0,
            y_offset,
            20.0,
            WHITE,
        );
        y_offset += line_height;

        let total_duration_str = match audio_manager.get_total_duration_ms() {
            Some(d) => format!("{:.2}s", d / 1000f64),
            None => "N/A".to_string(),
        };
        draw_text(
            &format!("Time: {:.2}s / {}", time / 1000f64, total_duration_str),
            10.0,
            y_offset,
            20.0,
            WHITE,
        );
        y_offset += line_height;

        let fps = format!("{:<3}", get_fps());
        let elapsed = start_instant.elapsed().as_secs_f64();
        let avg_fps = if elapsed > 0f64 {
            frame_count as f64 / elapsed
        } else {
            0f64
        };
        draw_text(
            &format!("FPS: {fps} | {avg_fps:.2}"),
            10.0,
            y_offset,
            20.0,
            WHITE,
        );
        y_offset += line_height;

        if let Some(err_msg) = audio_manager.get_error() {
            draw_text(
                &format!("Audio status: {err_msg}"),
                10.0,
                y_offset,
                18.0,
                YELLOW,
            );
        } else if audio_manager.audio_source_path.is_none() && map.audio_file.is_some() {
            draw_text(
                &format!(
                    "Audio status: no path set for '{}'",
                    map.audio_file.as_ref().unwrap()
                ),
                10.0,
                y_offset,
                18.0,
                YELLOW,
            );
        }

        next_frame().await;
    }

    Ok(())
}
