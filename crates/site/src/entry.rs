use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use blake3::Hash;
use color_eyre::Result;
use crossbeam::channel::bounded;
use ignore::{WalkBuilder, WalkState};
use redb::Database;

use crate::database::get_hashes;

/// An enum representing the type an entry can take
pub enum Typ {
    Markdown,
    Asset,
    Template,
    TemplatePage,
    StaticFile,
}

/// Any item that is to be processed by the static site generator.
#[derive(Debug, PartialEq, Eq)]
pub struct Entry {
    pub path: PathBuf,
    pub raw_content: Vec<u8>,
    pub hash: Hash,
}

impl Entry {
    pub const fn new(path: PathBuf, raw_content: Vec<u8>, hash: Hash) -> Self {
        Self {
            path,
            raw_content,
            hash,
        }
    }

    pub fn entry_type(&self) -> Typ {
        match self.path.extension().and_then(OsStr::to_str) {
            Some("md") => Typ::Markdown,
            Some("css" | "scss" | "js") => Typ::Asset,
            Some("html") => {
                if self
                    .path
                    .parent()
                    .is_some_and(|p| p.file_name().is_some_and(|s| s == "templates"))
                {
                    Typ::Template
                } else {
                    Typ::TemplatePage
                }
            }
            _ => Typ::StaticFile,
        }
    }
}

/// Recursively traverse the files in the given path, read each one, hash it, and
/// filter out only the ones that have changed or have been newly created since the
/// last run of yar.
pub fn discover_entries<P: AsRef<Path>>(db: &Database, path: P) -> Result<Vec<Entry>> {
    let (tx, rx) = bounded(100);

    let hashes = Arc::new(get_hashes(db)?);

    WalkBuilder::new(path).build_parallel().run(|| {
        let tx = tx.clone();
        let hashes = hashes.clone();

        Box::new(move |entry| {
            let entry = match entry {
                Ok(e) if e.file_type().is_some_and(|t| t.is_file()) => e,
                _ => return WalkState::Continue,
            };

            let path = entry.into_path();
            let content = fs::read(&path).expect("Error reading from file.");

            let hash = blake3::hash(&content);
            let original_hash = hashes.get(&path);

            // Create a new entry to be built if the hash has changed since or is newly created.
            if original_hash.is_none_or(|h| h == hash.as_bytes()) {
                tx.send(Entry::new(path, content, hash)).expect("Error while sending");
            }

            WalkState::Continue
        })
    });

    let ret: Vec<Entry> = rx.into_iter().collect();
    Ok(ret)
}
