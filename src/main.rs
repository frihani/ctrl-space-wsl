mod app_discovery;
mod config;
mod filter;
mod frequency;
mod launcher;
mod lock;

#[cfg(feature = "x11-backend")]
mod backend_x11;

#[cfg(feature = "sdl2-backend")]
mod backend_sdl2;
#[cfg(feature = "sdl2-backend")]
mod ui;

use std::env;

use config::Config;
use frequency::Frequency;
use lock::kill_others;

const VERSION: &str = env!("CARGO_PKG_VERSION");

fn print_info() {
    let dir = config::config_dir();
    #[cfg(feature = "x11-backend")]
    let backend = "x11";
    #[cfg(feature = "sdl2-backend")]
    let backend = "sdl2";
    #[cfg(not(any(feature = "x11-backend", feature = "sdl2-backend")))]
    let backend = "unknown";

    println!("ctrl-space-wsl ({})\n", backend);
    println!("Version:          v{}", VERSION);
    println!("Config:           {}", dir.join("config.toml").display());
    println!("Cache:            {}", dir.join("freq.txt").display());
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
        app_discovery::discover_apps()
    } else {
        frequency.refresh_in_background();
        frequency.apps()
    };

    #[cfg(feature = "x11-backend")]
    {
        if let Err(e) = backend_x11::run(config, frequency, apps) {
            eprintln!("Error: {}", e);
            std::process::exit(1);
        }
    }

    #[cfg(feature = "sdl2-backend")]
    {
        backend_sdl2::run(config, frequency, apps);
    }

    #[cfg(not(any(feature = "x11-backend", feature = "sdl2-backend")))]
    {
        eprintln!("No backend enabled. Enable x11-backend or sdl2-backend feature.");
        std::process::exit(1);
    }
}
