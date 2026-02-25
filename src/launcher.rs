use std::fs::OpenOptions;
use std::io::Write;
use std::process::Command;

use crate::config::config_dir;

pub struct LaunchResult {
    pub success: bool,
    pub command: String,
}

fn log(msg: &str) {
    let path = config_dir().join("ctrl-space-wsl.log");
    if let Ok(mut f) = OpenOptions::new().create(true).append(true).open(path) {
        let _ = writeln!(f, "{}", msg);
    }
}

pub fn launch_command(input: &str) -> LaunchResult {
    let parts: Vec<&str> = input.split_whitespace().collect();
    if parts.is_empty() {
        return LaunchResult { success: false, command: String::new() };
    }
    let normalized_cmd = parts.join(" ");
    let shell_cmd = format!("{} &", normalized_cmd);
    let mut cmd = Command::new("bash");
    cmd.args(["-l", "-c", &shell_cmd]);
    
    let home = std::env::var("HOME").unwrap_or_else(|_| "/home/dev".to_string());
    cmd.current_dir(&home);
    
    let distro = std::env::var("WSL_DISTRO_NAME").unwrap_or_default();
    if !distro.is_empty() {
        cmd.env("WSL_DISTRO_NAME", &distro);
    }
    
    let wslenv = std::env::var("WSLENV").unwrap_or_default();
    log(&format!("HOME={}", home));
    log(&format!("WSL_DISTRO_NAME={}", distro));
    log(&format!("WSLENV={}", wslenv));
    log(&format!("bash -l -c '{}'", shell_cmd));
    
    match cmd.spawn() {
        Ok(_) => {
            log("spawn: ok");
            LaunchResult { success: true, command: normalized_cmd }
        }
        Err(e) => {
            log(&format!("spawn: error {}", e));
            LaunchResult { success: false, command: String::new() }
        }
    }
}
