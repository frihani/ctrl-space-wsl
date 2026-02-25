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

const WINDOW_HEIGHT: f32 = 28.0;

fn main() {
    let args: Vec<String> = env::args().collect();
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

    let native_options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_decorations(false)
            .with_always_on_top()
            .with_position(eframe::egui::pos2(0.0, 0.0))
            .with_inner_size(eframe::egui::vec2(1920.0, WINDOW_HEIGHT))
            .with_resizable(false)
            .with_window_type(eframe::egui::X11WindowType::Dock),
        ..Default::default()
    };

    if let Err(e) = eframe::run_native(
        "ctrl-space-wsl",
        native_options,
        Box::new(move |_cc| Ok(Box::new(LauncherApp::new(config, apps, frequency)))),
    ) {
        eprintln!("Failed to run: {}", e);
        std::process::exit(1);
    }
}
