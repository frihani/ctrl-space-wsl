mod app_discovery;
mod config;
mod filter;
mod frequency;
mod launcher;
mod lock;
mod ui;

use std::env;

use config::Config;
use frequency::Frequency;
use lock::kill_others;
use ui::LauncherApp;

use egui_sdl2_gl::sdl2;
use egui_sdl2_gl::sdl2::event::Event;
use egui_sdl2_gl::sdl2::video::SwapInterval;
use egui_sdl2_gl::{DpiScaling, ShaderVersion};
use config::parse_hex_color;

const VERSION: &str = env!("CARGO_PKG_VERSION");
const WINDOW_HEIGHT: u32 = 28;

fn print_info() {
    let dir = config::config_dir();
    println!("ctrl-space-wsl\n");
    println!("Version:          v{}", VERSION);
    println!("Config:           {}", dir.join("config.toml").display());
    println!("Cache:            {}", dir.join("freq.txt").display());
    println!("Logs:             {}", dir.join("ctrl-space-wsl.log").display());
}

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.iter().any(|a| a == "--info" || a == "-i") {
        print_info();
        std::process::exit(0);
    }
    if args.iter().any(|a| a == "--init-config") {
        match config::create_default_config(false) {
            Ok(config::CreateConfigResult::Created(path)) => {
                println!("Created config file: {}", path.display());
                std::process::exit(0);
            }
            Ok(config::CreateConfigResult::NeedsConfirmation(path)) => {
                if config::confirm_overwrite() {
                    match config::create_default_config(true) {
                        Ok(config::CreateConfigResult::Created(p)) => {
                            println!("Created config file: {}", p.display());
                            std::process::exit(0);
                        }
                        _ => {
                            eprintln!("Failed to create config");
                            std::process::exit(1);
                        }
                    }
                } else {
                    println!("Cancelled. Config file unchanged: {}", path.display());
                    std::process::exit(0);
                }
            }

            Err(e) => {
                eprintln!("Failed to create config: {}", e);
                std::process::exit(1);
            }
        }
    }

    kill_others();

    let config = Config::load();
    let frequency = Frequency::load();

    let apps = if frequency.is_empty() {
        let discovered = app_discovery::discover_apps();
        discovered
    } else {
        frequency.refresh_in_background();
        frequency.apps()
    };

    let sdl_context = sdl2::init().expect("Failed to init SDL2");
    let video_subsystem = sdl_context.video().expect("Failed to init SDL2 video");

    let display_bounds = video_subsystem.display_bounds(0).unwrap_or(sdl2::rect::Rect::new(0, 0, 1920, 1080));
    let window_width = display_bounds.width();

    let gl_attr = video_subsystem.gl_attr();
    gl_attr.set_context_profile(sdl2::video::GLProfile::Core);
    gl_attr.set_context_version(3, 2);
    gl_attr.set_red_size(8);
    gl_attr.set_green_size(8);
    gl_attr.set_blue_size(8);
    gl_attr.set_alpha_size(8);

    let mut window = video_subsystem
        .window("ctrl-space-wsl", window_width, WINDOW_HEIGHT)
        .position(0, 0)
        .borderless()
        .opengl()
        .build()
        .expect("Failed to create window");

    let _gl_context = window.gl_create_context().expect("Failed to create GL context");

    sdl_context.mouse().show_cursor(false);

    let shader_ver = ShaderVersion::Default;

    let (mut painter, mut egui_state) = egui_sdl2_gl::with_sdl2(
        &window,
        shader_ver,
        DpiScaling::Custom((96.0 / 72.0) as f32),
    );

    let egui_ctx = egui::Context::default();
    let mut event_pump = sdl_context.event_pump().expect("Failed to get event pump");

    let _ = video_subsystem.gl_set_swap_interval(SwapInterval::VSync);

    let clear_color = parse_hex_color(&config.appearance.background).unwrap_or(egui::Color32::from_rgb(33, 34, 44));

    let mut app = LauncherApp::new(config, apps, frequency);
    let mut window_hidden = false;

    'main_loop: loop {
        if app.should_hide() && !window_hidden {
            window.hide();
            window_hidden = true;
        }

        if app.should_quit() {
            break 'main_loop;
        }

        if window_hidden {
            std::thread::sleep(std::time::Duration::from_millis(100));
            continue;
        }

        egui_state.input.time = Some(std::time::Instant::now().elapsed().as_secs_f64());

        for event in event_pump.poll_iter() {
            match event {
                Event::Quit { .. } => break 'main_loop,
                _ => {
                    egui_state.process_input(&window, event, &mut painter);
                }
            }
        }

        let egui_output = egui_ctx.run(egui_state.input.take(), |ctx| {
            app.update(ctx);
        });

        egui_state.process_output(&window, &egui_output.platform_output);

        let paint_jobs = egui_ctx.tessellate(egui_output.shapes, egui_output.pixels_per_point);
        painter.paint_jobs(Some(clear_color), egui_output.textures_delta, paint_jobs);
        window.gl_swap_window();
    }
}
