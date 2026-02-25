use fs2::FileExt;
use std::fs::{self, File, OpenOptions};
use std::io;

use crate::config::config_dir;

pub struct SingleInstance {
    _file: File,
}

impl SingleInstance {
    pub fn acquire() -> io::Result<Option<Self>> {
        let dir = config_dir();
        fs::create_dir_all(&dir)?;
        let lock_path = dir.join("lock");
        let file = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(false)
            .open(&lock_path)?;
        match file.try_lock_exclusive() {
            Ok(()) => Ok(Some(Self { _file: file })),
            Err(e) if e.kind() == io::ErrorKind::WouldBlock => Ok(None),
            Err(e) => Err(e),
        }
    }
}
