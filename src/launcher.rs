use std::os::unix::process::CommandExt;
use std::process::{Command, Stdio};

pub struct LaunchResult {
    pub success: bool,
    pub command: String,
}

pub fn launch_command(input: &str) -> LaunchResult {
    let parts: Vec<&str> = input.split_whitespace().collect();
    if parts.is_empty() {
        return LaunchResult { success: false, command: String::new() };
    }
    let program = parts[0];
    let args = &parts[1..];
    let mut cmd = Command::new(program);
    cmd.args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    unsafe {
        cmd.pre_exec(|| {
            libc::setsid();
            Ok(())
        });
    }
    let normalized_cmd = parts.join(" ");
    match cmd.spawn() {
        Ok(_) => LaunchResult { success: true, command: normalized_cmd },
        Err(_) => LaunchResult { success: false, command: String::new() },
    }
}
