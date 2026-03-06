mod app_discovery;
mod config;
mod filter;
mod frequency;
mod launcher;
mod lock;

mod ui;

use std::env;
use std::io::{self, BufRead, IsTerminal};

use config::Config;
use frequency::Frequency;
use lock::kill_others;

const VERSION: &str = env!("CARGO_PKG_VERSION");

fn print_info() {
    let dir = config::config_dir();

    println!("ctrl-space-wsl \n");
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

    // Read from stdin if it's not a terminal (piped)
    let stdin_items: Vec<String> = if !io::stdin().is_terminal() {
        io::stdin()
            .lock()
            .lines()
            .map_while(Result::ok)
            .filter(|l| !l.is_empty())
            .collect()
    } else {
        Vec::new()
    };

    // Filter mode only if we actually received piped content
    let filter_mode = !stdin_items.is_empty();

    let config = Config::load();

    let (frequency, apps) = if filter_mode {
        (Frequency::default(), stdin_items)
    } else {
        kill_others();
        let freq = Frequency::load();
        let apps = if freq.is_empty() {
            app_discovery::discover_apps()
        } else {
            freq.refresh_in_background();
            freq.apps()
        };
        (freq, apps)
    };

    if let Err(e) = ui::run(config, frequency, apps, filter_mode) {
        eprintln!("Error: {}", e);
        std::process::exit(1);
    }
}
