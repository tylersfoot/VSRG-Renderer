#![allow(clippy::eq_op)]
#![allow(unused_imports)]

mod audio_manager;
mod draw;
mod map;
mod render;
mod utils;
mod logger;

use audio_manager::AudioManager;
use draw::MacroquadDraw;
use map::Map;
use render::{render_frame, set_reference_positions, FrameState};
use utils::{index_at_time, lerp, object_at_time, sort_by_start_time, HasStartTime, Time, JudgementType, SKIN};

use anyhow::Result;
use clap::Parser;
use core::f64;
use macroquad::prelude::*;
// use macroquad::miniquad::{BlendFactor, PipelineParams};

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
    map_dir: PathBuf, // directory containing the map (.qua) file
    #[arg(long)]
    fullscreen: bool, // start in fullscreen
    #[arg(long, default_value_t = 1.0)]
    rate: f64,        // playback rate
    #[arg(long, default_value_t = 0.03)]
    volume: f64,      // initial audio volume
    #[arg(long)]
    mirror: bool,     // mirror notes horizontally
    #[arg(long)]
    no_sv: bool,      // ignore scroll velocities
    #[arg(long)]
    no_ssf: bool,     // ignore scroll speed factors
    #[arg(long)]
    autoplay: bool,   // autoplay mode
    #[arg(long)]
    debug: bool,      // enable debug text
    #[arg(long)]
    no_ui: bool,      // disable UI elements
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
    let args = CliArgs::parse();
    let mut is_fullscreen = args.fullscreen;

    // --- audio setup ---
    let mut audio_manager = AudioManager::new().map_err(|e| {
        logger::error(&format!(
            "Critical audio error on init: {e}"
        ));
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
        logger::error(&err_msg);
        anyhow::bail!(err_msg);
    };
    logger::info(&format!(
        "Loading map: {map_file_name}"
    ));
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
    map.mods.autoplay = args.autoplay;
    map.mods.debug = args.debug;
    map.mods.no_ui = args.no_ui;

    // map processing functions / preload
    let receptor_texture: Texture2D = load_texture("skins/receptor.png").await.unwrap();
    let field_positions = set_reference_positions(&receptor_texture);
    map.initialize_default_timing_group();
    map.sort();
    map.initialize_control_points();
    map.initialize_hit_objects(&field_positions).map_err(|e| {
        logger::error(&format!("Failed to initialize hit objects: {e}"));
        e
    })?;
    map.initialize_timing_lines(&field_positions).map_err(|e| {
        logger::error(&format!("Failed to initialize timing lines: {e}"));
        e
    })?;
    map.initialize_beat_snaps().map_err(|e| {
        logger::error(&format!("Failed to initialize beat snaps: {e}"));
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
    logger::info(&format!(
        "Map loaded successfully: {total_hit_objects} Hit Objects, {total_timing_points} Timing Points, {total_svs} SVs, {total_ssfs} SSFs, {total_timing_groups} Timing Groups, {total_timing_lines} Timing Lines"
    ));

    // this is the visual play state, audio is handled by audio_manager
    let mut is_playing_visuals = false;

    // let mut json_output_file = File::create("output.json")?;
    // let json_string = serde_json::to_string_pretty(&map)?;
    // write!(json_output_file, "{json_string}")?;
    // logger::info("Parsed map data written to output.json");

    let start_instant = Instant::now();
    let mut frame_count: u64 = 0;

    // let vert_src = r#"#version 100
    // attribute vec3 position;
    // attribute vec2 texcoord;

    // varying lowp vec2 v_uv;

    // void main() {
    //     gl_Position = vec4(position,1);
    //     v_uv = texcoord;
    // }
    // "#;

    // // build a full-screen pass that *computes* density from an array of circles
    // let density_frag = r#"#version 100
    // precision mediump float;

    // varying lowp vec2 v_uv;

    // uniform vec2 centers[32];
    // uniform float radii[32];
    // uniform int ball_count;

    // void main() {
    //     float d = 0.0;

    //     for (int i=0; i<32; i++) {
    //         if (i >= ball_count) {
    //             break;
    //         }

    //         // quadratic falloff: 1 at center → 0 at radius
    //         float r = radii[i];
    //         float dist = length(v_uv - centers[i]) / r;
    //         d += max(0.0, 1.0 - dist*dist);
    //     }

    //     gl_FragColor = vec4(0.0, 0.0, 0.0, d);
    // }
    // "#;

    // let outline_frag = r#"#version 100
    // precision mediump float;

    // varying lowp vec2 v_uv;

    // uniform sampler2D densityMap;
    // uniform sampler2D fillMap;
    // uniform vec2 density_size;
    // uniform vec2 screen_size;
    // uniform float density_threshold;
    // uniform float outline_radius;
    // uniform vec4 outline_color;

    // void main() {
    //     // uv for density sampling
    //     vec2 d_uv = v_uv * (screen_size / density_size);
    //     float den = texture2D(densityMap, d_uv).a;

    //     // neighbor-sampling outline
    //     float neigh = 0.0;
    //     vec2 px = 1.0 / density_size;
    //     int r = int(outline_radius);
    //     for (int x=-r; x<=r; x++) {
    //         for (int y=-r; y<=r; y++) {
    //             if (float(x*x + y*y) <= outline_radius*outline_radius) {
    //                 float nd = texture2D(densityMap, d_uv + px * vec2(float(x),float(y))).a;
    //                 neigh += step(density_threshold, nd);
    //             }
    //         }
    //     }
    //     float is_outline = clamp(neigh, 0.0, 1.0) * (1.0 - step(density_threshold, den));
    //     if (den < density_threshold && is_outline <= 0.0) {
    //         discard;
    //     }
        
    //     // pick fill vs outline
    //     vec4 fillc = texture2D(fillMap, v_uv);
    //     gl_FragColor = mix(fillc, outline_color, is_outline);
    // }
    // "#;

    // let density_material = load_material(
    //     ShaderSource::Glsl {
    //         vertex: vert_src,
    //         fragment: density_frag,
    //     },
    //     MaterialParams {
    //         uniforms: vec![
    //             UniformDesc::new("centers", UniformType::Float2Array(32)),
    //             UniformDesc::new("radii", UniformType::Float1Array(32)),
    //             UniformDesc::new("ball_count", UniformType::Int1),
    //         ],
    //         ..Default::default()
    //     },
    // )
    // .unwrap();

    // let outline_material = load_material(
    //     ShaderSource::Glsl {
    //         vertex: vert_src,
    //         fragment: outline_frag,
    //     },
    //     MaterialParams {
    //         uniforms: vec![
    //             UniformDesc::new("densityMap", UniformType::Sampler2D),
    //             UniformDesc::new("fillMap", UniformType::Sampler2D),
    //             UniformDesc::new("density_size", UniformType::Float2),
    //             UniformDesc::new("screen_size", UniformType::Float2),
    //             UniformDesc::new("density_threshold", UniformType::Float1),
    //             UniformDesc::new("outline_radius", UniformType::Float1),
    //             UniformDesc::new("outline_color", UniformType::Float4),
    //         ],
    //         ..Default::default()
    //     },
    // )
    // .unwrap();


    // let combined_rt = render_target(
    //     screen_width() as u32,
    //     screen_height() as u32,
    // );
    // combined_rt.texture.set_filter(FilterMode::Nearest);

    // let scene_params = PipelineParams {
    //     color_blend: BlendFactor::Alpha,    // default
    //     alpha_blend: BlendFactor::Alpha,
    //     color_mask: ColorMask { r:true, g:true, b:true, a:false }, // block alpha writes
    //     ..Default::default()
    // };

    // let density_params = PipelineParams {
    //     color_blend: BlendFactor::One,      // additive
    //     alpha_blend: BlendFactor::One,
    //     color_mask: ColorMask { r:false, g:false, b:false, a:true },// only alpha
    //     ..Default::default()
    // };

    // main render loop
    loop {
        frame_count += 1;

        let time = audio_manager.get_current_song_time_ms() + SKIN.offset;
        map.time = time;

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

        // gameplay keybinds
        if !map.mods.autoplay {
            if is_key_pressed(KeyCode::A) {
                map.handle_gameplay_key_press(map.time, 0);
            }
            if is_key_pressed(KeyCode::S) {
                map.handle_gameplay_key_press(map.time, 1);
            }
            if is_key_pressed(KeyCode::Semicolon) {
                map.handle_gameplay_key_press(map.time, 2);
            }
            if is_key_pressed(KeyCode::Apostrophe) {
                map.handle_gameplay_key_press(map.time, 3);
            }
        }

        let mut macroquad_draw = MacroquadDraw;
        let mut frame_state = FrameState {
            map: &mut map,
            field_positions: &field_positions,
        };

        // --------- render stuff --------

        clear_background(BLACK); // resets frame to all black
        render_frame(&mut frame_state, &mut macroquad_draw).map_err(|e| {
            logger::error(&format!("Render error: {e}"));
            e
        })?;

        // -------- draw ui / debug info --------
        let line_height = 20.0;
        if map.mods.debug {
            let mut y_offset = 20.0;

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
            } else if audio_manager
                .sink
                .as_ref()
                .is_some_and(rodio::Sink::is_paused)
            {
                "Paused"
            } else {
                "Stopped/empty"
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
        }

        if !map.mods.no_ui {
            // -------- judgements --------
            let mut right_y = 400.0;
            for judgement in [
                JudgementType::Marvelous,
                JudgementType::Perfect,
                JudgementType::Great,
                JudgementType::Good,
                JudgementType::Okay,
                JudgementType::Miss,
            ] {
                let count = map.judgement_counts.get(&judgement).copied().unwrap_or(0);
                draw_text(
                &format!("{judgement}: {count}"),
                screen_width() - 400.0,
                right_y,
                50.0,
                WHITE,
                );
                right_y += line_height * 2.0;
            }

            // -------- judgement splash --------
            let splash_length = 500.0; // duration of the splash effect in ms
            if let Some((judgement, time, offset_ms)) = map.last_judgement {
                let elapsed = audio_manager.get_current_song_time_ms() - time;
                if elapsed < splash_length {
                    let alpha = (1.0 - (elapsed / splash_length)).clamp(0.0, 1.0);
                    let color = match judgement {
                        JudgementType::Marvelous => WHITE,
                        JudgementType::Perfect => GOLD,
                        JudgementType::Great => GREEN,
                        JudgementType::Good => BLUE,
                        JudgementType::Okay => DARKGRAY,
                        JudgementType::Miss => RED,
                    };
                    draw_text_ex(
                        &judgement.to_string(),
                        screen_width() / 2.0 - 100.0,
                        screen_height() / 2.0,
                        TextParams {
                            font_size: 60,
                            color: Color {
                                r: color.r,
                                g: color.g,
                                b: color.b,
                                a: alpha as f32,
                            },
                            ..Default::default()
                        },
                    );
                    let offset = if offset_ms.abs() >= 1.0 {
                        &format!("{offset_ms:+.0}")
                    } else {
                        ""
                    };
                    draw_text_ex(
                        offset,
                        screen_width() / 2.0 - 50.0,
                        screen_height() / 2.0 + 50.0,
                        TextParams {
                            font_size: 30,
                            color: GRAY,
                            ..Default::default()
                        },
                    );
                }
            }

            // -------- combo --------
            if map.combo > 0 {
                draw_text(
                    &format!("{}", map.combo),
                    screen_width() / 2.0 - 10.0,
                    screen_height() / 2.0 - 200.0,
                    60.0,
                    WHITE,
                );
            }

            // -------- accuracy --------
            let mut points = 0.0;
            let total_judgements = map.judgement_counts.values().sum::<usize>() as f64;
            points += map.judgement_counts.get(&JudgementType::Marvelous).copied().unwrap_or(0) as f64 * 100.0;
            points += map.judgement_counts.get(&JudgementType::Perfect).copied().unwrap_or(0) as f64   * 98.25;
            points += map.judgement_counts.get(&JudgementType::Great).copied().unwrap_or(0) as f64     * 65.0;
            points += map.judgement_counts.get(&JudgementType::Good).copied().unwrap_or(0) as f64      * 25.0;
            points += map.judgement_counts.get(&JudgementType::Okay).copied().unwrap_or(0) as f64      * -100.0;
            points += map.judgement_counts.get(&JudgementType::Miss).copied().unwrap_or(0) as f64      * -50.0;
            let accuracy_display = if total_judgements <= 0.0 {
                "100.00%".to_string()
            } else {
                format!("{:.2}%", (points / total_judgements).max(0.0))
            };
            draw_text(
                &accuracy_display,
                screen_width() - 300.0,
                80.0,
                80.0,
                WHITE,
            );
        }


        next_frame().await;
    }

    Ok(())
}
