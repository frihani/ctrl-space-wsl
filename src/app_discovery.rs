use std::collections::HashSet;
use std::env;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;

pub fn discover_apps() -> Vec<String> {
    let mut seen = HashSet::new();
    let mut apps = Vec::new();
    let path_var = env::var("PATH").unwrap_or_default();
    for dir in path_var.split(':') {
        if dir.is_empty() {
            continue;
        }
        if dir.starts_with("/mnt/") {
            continue;
        }
        let dir_path = PathBuf::from(dir);
        let entries = match fs::read_dir(&dir_path) {
            Ok(e) => e,
            Err(_) => continue,
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_file() {
                continue;
            }
            let meta = match path.metadata() {
                Ok(m) => m,
                Err(_) => continue,
            };
            if meta.permissions().mode() & 0o111 == 0 {
                continue;
            }
            if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                if seen.insert(name.to_string()) {
                    apps.push(name.to_string());
                }
            }
        }
    }
    apps.sort();
    apps
}
