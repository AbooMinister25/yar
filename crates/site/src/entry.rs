use std::fs;
use std::path::{Path, PathBuf};

use color_eyre::Result;
use ignore::Walk;
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
        .iter()
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
    let mut ret = Vec::new();
    for entry in Walk::new(path.as_ref())
        .filter_map(Result::ok)
        .filter(|e| !e.path().is_dir())
    {
        let content = fs::read(entry.path())?;
        ret.push((entry.into_path(), content));
    }

    Ok(ret)
}
