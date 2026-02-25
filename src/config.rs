use serde::Deserialize;
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct Config {
    pub appearance: Appearance,
}

#[derive(Debug, Clone, Deserialize)]
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
    pub max_results: usize,
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
            match_highlight: "#50FA7B".to_string(),
            prompt_color: "#BD93F9".to_string(),
            font_family: "Monospace".to_string(),
            font_size: 10,
            max_results: 10,
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

fn config_path() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("ctrl-space-wsl")
        .join("config.toml")
}

pub fn parse_hex_color(hex: &str) -> Option<crossterm::style::Color> {
    let hex = hex.strip_prefix('#')?;
    if hex.len() != 6 {
        return None;
    }
    let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
    let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
    let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
    Some(crossterm::style::Color::Rgb { r, g, b })
}
