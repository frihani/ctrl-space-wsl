use std::collections::HashMap;
use std::fs::{self, File};
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::app_discovery::discover_apps;
use crate::config::config_dir;

pub struct Frequency {
    counts: HashMap<String, u32>,
    path: PathBuf,
    dirty: Arc<AtomicBool>,
}

impl Frequency {
    pub fn load() -> Self {
        let path = data_path();
        let counts = if path.exists() {
            let file = match File::open(&path) {
                Ok(f) => f,
                Err(_) => return Self::empty(path),
            };
            let reader = BufReader::new(file);
            let mut counts = HashMap::new();
            for line in reader.lines().flatten() {
                if let Some((name, count_str)) = line.rsplit_once('\t') {
                    if let Ok(count) = count_str.parse::<u32>() {
                        counts.insert(name.to_string(), count);
                    }
                }
            }
            counts
        } else {
            HashMap::new()
        };
        Self { counts, path, dirty: Arc::new(AtomicBool::new(false)) }
    }

    fn empty(path: PathBuf) -> Self {
        Self { counts: HashMap::new(), path, dirty: Arc::new(AtomicBool::new(false)) }
    }

    pub fn apps(&self) -> Vec<String> {
        self.counts.keys().cloned().collect()
    }

    pub fn is_empty(&self) -> bool {
        self.counts.is_empty()
    }

    pub fn get(&self, name: &str) -> u32 {
        self.counts.get(name).copied().unwrap_or(0)
    }

    pub fn increment(&mut self, name: &str) {
        *self.counts.entry(name.to_string()).or_insert(0) += 1;
    }

    pub fn remove(&mut self, name: &str) {
        self.counts.remove(name);
    }

    pub fn save(&self) -> std::io::Result<()> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)?;
        }
        let mut file = File::create(&self.path)?;
        for (name, count) in &self.counts {
            writeln!(file, "{}\t{}", name, count)?;
        }
        Ok(())
    }

    pub fn refresh_in_background(&self) {
        let path = self.path.clone();
        let dirty = self.dirty.clone();
        std::thread::spawn(move || {
            let apps = discover_apps();
            let mut counts = HashMap::new();
            
            if let Ok(file) = File::open(&path) {
                let reader = BufReader::new(file);
                for line in reader.lines().flatten() {
                    if let Some((name, count_str)) = line.rsplit_once('\t') {
                        if let Ok(count) = count_str.parse::<u32>() {
                            counts.insert(name.to_string(), count);
                        }
                    }
                }
            }
            
            for app in apps {
                counts.entry(app).or_insert(0);
            }
            
            if let Some(parent) = path.parent() {
                let _ = fs::create_dir_all(parent);
            }
            if let Ok(mut file) = File::create(&path) {
                for (name, count) in &counts {
                    let _ = writeln!(file, "{}\t{}", name, count);
                }
            }
            dirty.store(true, Ordering::Relaxed);
        });
    }
}

fn data_path() -> std::path::PathBuf {
    config_dir().join("freq.txt")
}
