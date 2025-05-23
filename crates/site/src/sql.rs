use std::path::Path;

use color_eyre::{Result, eyre::ContextCompat};
use rusqlite::Connection;

/// Set up SQLite database.
/// Create initial tables if they don't exist and acquire the connection.
pub fn setup_sql() -> Result<Connection> {
    let conn = Connection::open("site.db")?;

    conn.execute(
        "
        CREATE TABLE IF NOT EXISTS entries (
            entry_id INTEGER PRIMARY KEY,
            path VARCHAR NOT NULL,
            hash TEXT NOT NULL
        )
    ",
        (),
    )?;

    conn.execute(
        "
        CREATE TABLE IF NOT EXISTS pages (
            page_id INTEGER PRIMARY KEY,
            out_path VARCHAR NOT NULL,
            permalink TEXT NOT NULL,
            title TEXT NOT NULL,
            tags JSON NOT NULL,
            date TEXT NOT NULL,
            update TEXT NOT NULL,
            summary TEXT NOT NULL,
            content TEXT NOT NULL,
            entry INTEGER,
            FOREIGN KEY(integer) REFERENCES entries(entry_id)
        )
    ",
        (),
    )?;

    conn.execute(
        "
        CREATE TABLE IF NOT EXISTS assets (
            asset_id INTEGER PRIMARY KEY,
            out_path VARCHAR NOT NULL,
            permalink TEXT NOT NULL,
            entry INTEGER,
            FOREIGN KEY(integer) REFERENCES entries(entry_id)
        )
    ",
        (),
    )?;

    conn.execute(
        "
        CREATE TABLE IF NOT EXISTS staticfile (
            file_id INTEGER PRIMARY KEY,
            out_path VARCHAR NOT NULL,
            permalink TEXT NOT NULL,
            entry INTEGER,
            FOREIGN KEY(integer) REFERENCES entries(entry_id)
        )
    ",
        (),
    )?;

    Ok(conn)
}

pub fn get_hashes<P: AsRef<Path>>(conn: &Connection, path: P) -> Result<Vec<String>> {
    let mut stmt = conn.prepare("SELECT hash FROM entries WHERE path = :path")?;
    let path_str = path
        .as_ref()
        .to_str()
        .context("Error while converting path to string")?;

    let hashes_iter = stmt.query_map(&[(":path", path_str)], |row| row.get(0))?;
    let mut hashes: Vec<String> = Vec::new();

    for hash in hashes_iter {
        hashes.push(hash?);
    }

    Ok(hashes)
}
