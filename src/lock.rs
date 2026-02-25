use fs2::FileExt;
use std::fs::{File, OpenOptions};
use std::io;
use std::path::PathBuf;

pub struct SingleInstance {
    _file: File,
}

impl SingleInstance {
    pub fn acquire() -> io::Result<Option<Self>> {
        let lock_path = lock_path();
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

fn lock_path() -> PathBuf {
    PathBuf::from("/tmp/ctrl-space-wsl.lock")
}
