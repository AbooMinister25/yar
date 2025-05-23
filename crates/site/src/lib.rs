mod config;
mod entry;
mod sql;

use config::Config;
use entry::Entry;
use rusqlite::Connection;

/// A site to be built.
pub struct Site {
    conn: Connection,
    config: Config,
    entries: Vec<Entry>,
}

impl Site {
    pub fn new(conn: Connection, config: Config) -> Self {
        Self {
            conn,
            config,
            entries: Vec::new(),
        }
    }
}
