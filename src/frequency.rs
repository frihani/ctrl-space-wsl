use std::collections::HashMap;
use std::fs::{self, File};
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;

pub struct Frequency {
    counts: HashMap<String, u32>,
    path: PathBuf,
}

impl Frequency {
    pub fn load() -> Self {
        let path = data_path();
        let counts = if path.exists() {
            let file = match File::open(&path) {
                Ok(f) => f,
                Err(_) => return Self { counts: HashMap::new(), path },
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
        Self { counts, path }
    }

    pub fn get(&self, name: &str) -> u32 {
        self.counts.get(name).copied().unwrap_or(0)
    }

    pub fn increment(&mut self, name: &str) {
        *self.counts.entry(name.to_string()).or_insert(0) += 1;
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

    pub fn commands(&self) -> Vec<String> {
        self.counts.keys().cloned().collect()
    }
}

fn data_path() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("ctrl-space-wsl")
        .join("freq.txt")
}
