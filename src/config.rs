use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct Config {
    pub appearance: Appearance,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct Appearance {
    pub foreground: String,
    pub background: String,
    pub selection_fg: String,
    pub selection_bg: String,
    pub match_highlight: String,
    pub prompt_color: String,
    pub font_family: String,
    pub font_size: u8,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            appearance: Appearance::default(),
        }
    }
}

impl Default for Appearance {
    fn default() -> Self {
        Self {
            foreground: "#F8F8F2".to_string(),
            background: "#21222C".to_string(),
            selection_fg: "#F8F8F2".to_string(),
            selection_bg: "#6272A4".to_string(),
            match_highlight: "#8be9fd".to_string(),
            prompt_color: "#BD93F9".to_string(),
            font_family: "Monospace".to_string(),
            font_size: 10,
        }
    }
}

impl Config {
    pub fn load() -> Self {
        let path = config_path();
        if path.exists() {
            if let Ok(content) = fs::read_to_string(&path) {
                if let Ok(config) = toml::from_str(&content) {
                    return config;
                }
            }
        }
        Self::default()
    }
}

pub fn config_dir() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("ctrl-space-wsl")
}

fn config_path() -> PathBuf {
    config_dir().join("config.toml")
}

pub enum CreateConfigResult {
    Created(PathBuf),
    NeedsConfirmation(PathBuf),
}

pub fn create_default_config(force: bool) -> std::io::Result<CreateConfigResult> {
    let dir = config_dir();
    fs::create_dir_all(&dir)?;
    let path = dir.join("config.toml");
    let new_content = toml::to_string_pretty(&Config::default())
        .unwrap_or_default();
    if path.exists() {
        let existing = fs::read_to_string(&path).unwrap_or_default();
        if existing != new_content && !force {
            return Ok(CreateConfigResult::NeedsConfirmation(path));
        }
    }
    fs::write(&path, new_content)?;
    Ok(CreateConfigResult::Created(path))
}

pub fn confirm_overwrite() -> bool {
    use std::io::{self, Write};
    print!("Config file differs from default. Overwrite? [y/N] ");
    io::stdout().flush().ok();
    let mut input = String::new();
    if io::stdin().read_line(&mut input).is_ok() {
        let input = input.trim().to_lowercase();
        return input == "y" || input == "yes";
    }
    false
}

pub fn parse_hex_color(hex: &str) -> Option<egui::Color32> {
    let hex = hex.strip_prefix('#')?;
    if hex.len() != 6 {
        return None;
    }
    let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
    let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
    let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
    Some(egui::Color32::from_rgb(r, g, b))
}
