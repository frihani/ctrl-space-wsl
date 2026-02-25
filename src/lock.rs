use std::fs;
use std::process;

use crate::config::config_dir;

pub fn kill_others() {
    let my_pid = process::id();
    let my_exe = fs::read_link("/proc/self/exe").ok();
    
    std::thread::spawn(move || {
        let pid_path = config_dir().join("pid");
        
        if let Ok(old_pid_str) = fs::read_to_string(&pid_path) {
            if let Ok(old_pid) = old_pid_str.trim().parse::<u32>() {
                if old_pid != my_pid {
                    let old_exe = fs::read_link(format!("/proc/{}/exe", old_pid)).ok();
                    if old_exe == my_exe {
                        unsafe { libc::kill(old_pid as i32, libc::SIGTERM); }
                    }
                }
            }
        }
        
        let _ = fs::create_dir_all(config_dir());
        let _ = fs::write(&pid_path, my_pid.to_string());
    });
}
