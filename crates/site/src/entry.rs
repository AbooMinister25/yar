use std::path::{Path, PathBuf};
use std::{fs, io};

use color_eyre::Result;
use crossbeam::channel::bounded;
use ignore::{WalkBuilder, WalkState};
use rayon::prelude::*;
use rusqlite::Connection;

use crate::sql::get_hashes;

/// Any item that is to be processed by the static site generator.
#[derive(Debug, PartialEq, Eq)]
pub struct Entry {
    pub path: PathBuf,
    pub raw_content: Vec<u8>,
    pub hash: String,
}

impl Entry {
    pub const fn new(path: PathBuf, raw_content: Vec<u8>, hash: String) -> Self {
        Self {
            path,
            raw_content,
            hash,
        }
    }
}

/// Recursively traverse the files in the given path, read each one, hash it, and
/// filter out only the ones that have changed or have been newly created since the
/// last run.
pub fn discover_entries<T: AsRef<Path>>(path: T, conn: &Connection) -> Result<Vec<Entry>> {
    let mut ret = Vec::new();

    let entries = read_entries(path)?;
    let hashes = entries
        .par_iter()
        .map(|(_, s)| format!("{:016x}", seahash::hash(s)))
        .collect::<Vec<String>>();

    for ((path, content), hash) in entries.into_iter().zip(hashes) {
        let hashes = get_hashes(conn, &path)?;

        // Either a new file was created, or an existing file was changed.
        if hashes.is_empty() || hashes[0] != hash {
            ret.push(Entry::new(path, content, hash));
        }
    }

    Ok(ret)
}

fn read_entries<T: AsRef<Path>>(path: T) -> Result<Vec<(PathBuf, Vec<u8>)>> {
    let (tx, rx) = bounded(100);

    let handle = std::thread::spawn(|| {
        let mut entries = Vec::new();

        for entry in rx {
            entries.push(entry);
        }

        entries
    });

    WalkBuilder::new(path).build_parallel().run(|| {
        let tx = tx.clone();

        Box::new(move |path| {
            if let Ok(p) = path {
                if !p.path().is_dir() {
                    let content = fs::read(p.path()).expect("Error reading from file.");
                    tx.send((p.into_path(), content))
                        .expect("Error while sending.");
                }
            }

            WalkState::Continue
        })
    });

    drop(tx);

    let ret = handle
        .join()
        .map_err(|e| io::Error::other(format!("Collector thread panicked: {e:?}")))?;

    Ok(ret)
}
