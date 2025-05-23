use std::fs;
use std::path::{Path, PathBuf};

use color_eyre::Result;
use ignore::Walk;

/// Any item that is to be processed by the static site generator.
#[derive(Debug, PartialEq, Eq)]
pub struct Entry {
    pub path: PathBuf,
    pub raw_content: Vec<u8>,
    pub hash: String,
}

impl Entry {
    pub fn new(path: PathBuf, raw_content: Vec<u8>, hash: String) -> Self {
        Self {
            path,
            raw_content,
            hash,
        }
    }
}
