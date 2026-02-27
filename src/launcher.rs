use std::fs::OpenOptions;
use std::io::Write;
use std::process::Command;

use crate::config::config_dir;

pub struct LaunchResult {
    pub success: bool,
    pub command: String,
    pub needs_delay: bool,
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
        return LaunchResult {
            success: false,
            command: String::new(),
            needs_delay: false,
        };
    }

    let program = parts[0];
    let normalized_cmd = parts.join(" ");
    let home = std::env::var("HOME").unwrap_or_else(|_| "/home/dev".to_string());

    let resolved_program = std::fs::canonicalize(program)
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|_| program.to_string());

    let is_windows_exe = resolved_program.to_lowercase().ends_with(".exe");

    log(&format!(
        "launching: {} -> {} (windows_exe={})",
        program, resolved_program, is_windows_exe
    ));

    let result = if is_windows_exe {
        let mut cmd = Command::new(&resolved_program);
        cmd.args(&parts[1..]);
        cmd.current_dir(&home);
        cmd.stdin(std::process::Stdio::null());
        cmd.stdout(std::process::Stdio::null());
        cmd.stderr(std::process::Stdio::null());
        cmd.spawn()
    } else {
        let shell_cmd = format!("nohup {} >/dev/null 2>&1 &", normalized_cmd);
        let mut cmd = Command::new("bash");
        cmd.args(["-c", &shell_cmd]);
        cmd.current_dir(&home);
        cmd.spawn()
    };

    match result {
        Ok(_) => {
            log("spawn: ok");
            LaunchResult {
                success: true,
                command: normalized_cmd,
                needs_delay: is_windows_exe,
            }
        }
        Err(e) => {
            log(&format!("spawn: error {}", e));
            LaunchResult {
                success: false,
                command: String::new(),
                needs_delay: false,
            }
        }
    }
}
