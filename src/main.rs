mod app_discovery;
mod config;
mod filter;
mod frequency;
mod launcher;
mod lock;
mod ui;

use config::Config;
use filter::filter_apps;
use frequency::Frequency;
use lock::SingleInstance;
use ui::{Action, Ui};

fn main() {
    let _lock = match SingleInstance::acquire() {
        Ok(Some(lock)) => lock,
        Ok(None) => {
            eprintln!("Another instance is already running");
            std::process::exit(1);
        }
        Err(e) => {
            eprintln!("Failed to acquire lock: {}", e);
            std::process::exit(1);
        }
    };
    let config = Config::load();
    let mut frequency = Frequency::load();
    let apps = app_discovery::discover_apps();
    let ui = Ui::new(config);
    if let Err(e) = ui.enter() {
        eprintln!("Failed to enter UI: {}", e);
        std::process::exit(1);
    }
    let mut query = String::new();
    let mut selected: usize = 0;
    let max_results = ui.max_results();
    loop {
        let results = filter_apps(&apps, &query, &frequency, max_results);
        if selected >= results.len() {
            selected = results.len().saturating_sub(1);
        }
        if let Err(e) = ui.render(&query, &results, selected) {
            let _ = ui.leave();
            eprintln!("Render error: {}", e);
            std::process::exit(1);
        }
        let action = match ui::read_key() {
            Ok(a) => a,
            Err(_) => continue,
        };
        match action {
            Action::Char(c) => {
                query.push(c);
                selected = 0;
            }
            Action::Backspace => {
                query.pop();
                selected = 0;
            }
            Action::Up => {
                selected = selected.saturating_sub(1);
            }
            Action::Down => {
                if selected + 1 < results.len() {
                    selected += 1;
                }
            }
            Action::Enter => {
                let _ = ui.leave();
                let command = if query.trim().is_empty() {
                    results.get(selected).map(|a| a.name.clone())
                } else {
                    Some(query.clone())
                };
                if let Some(cmd) = command {
                    let result = launcher::launch_command(&cmd);
                    if result.success && !result.program.is_empty() {
                        frequency.increment(&result.program);
                        let _ = frequency.save();
                    }
                }
                break;
            }
            Action::Escape => {
                let _ = ui.leave();
                break;
            }
            Action::None => {}
        }
    }
}
