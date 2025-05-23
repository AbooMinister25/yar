mod config;
mod entry;
mod sql;

use color_eyre::Result;
use config::Config;
use entry::{Entry, discover_entries};
use rusqlite::Connection;

/// A site to be built.
pub struct Site {
    conn: Connection,
    config: Config,
    entries: Vec<Entry>,
}

impl Site {
    pub fn new(conn: Connection, config: Config) -> Result<Self> {
        let entries = discover_entries(&config.root, &conn)?;

        Ok(Self {
            conn,
            config,
            entries,
        })
    }
}
