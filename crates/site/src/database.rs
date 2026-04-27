use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
};

use color_eyre::{Result, eyre::ContextCompat};
use redb::{Database, ReadableDatabase, ReadableTable, TableDefinition, backends::InMemoryBackend};

use crate::page::Page;

const PAGES: TableDefinition<&str, &[u8]> = TableDefinition::new("pages");
const HASHES: TableDefinition<&str, &[u8]> = TableDefinition::new("hashes");

#[derive(Debug, Clone, Copy)]
pub enum DatabaseSource<'a> {
    Memory,
    File(&'a Path),
}

/// Initializes the database, either in-memory or from a file on disk.
pub fn setup_database(source: DatabaseSource) -> Result<Database> {
    let db = match source {
        DatabaseSource::File(p) => Database::create(p)?,
        DatabaseSource::Memory => Database::builder().create_with_backend(InMemoryBackend::new())?,
    };

    Ok(db)
}

/// Get all hashes
pub fn get_hashes(db: &Database) -> Result<HashMap<PathBuf, [u8; 32]>> {
    let read_txn = db.begin_read()?;
    let table = read_txn.open_table(HASHES)?;

    Ok(table
        .iter()?
        .filter_map(|e| {
            let (k, v) = e.ok()?;
            let hash: [u8; 32] = v.value().try_into().ok()?;
            Some((PathBuf::from(k.value()), hash))
        })
        .collect())
}

/// Get all the pages stored in the database, filtering out any ones with invalidated paths that were passed in.
pub fn get_pages<S: ::std::hash::BuildHasher>(db: &Database, invalidated: &HashSet<PathBuf, S>) -> Result<Vec<Page>> {
    let read_txn = db.begin_read()?;
    let table = read_txn.open_table(PAGES)?;

    table
        .iter()?
        .filter_map(|res| {
            let (k, bytes) = res.ok()?;
            let path = PathBuf::from(k.value());
            if invalidated.contains(&path) {
                return None;
            }
            let page = postcard::from_bytes(bytes.value()).map_err(Into::into);
            Some(page)
        })
        .collect::<Result<Vec<Page>>>()
}

/// Insert a hash into the database. If there is already a hash for the given path, the existing entry is updated.
pub fn insert_hash<P: AsRef<Path>, B: AsRef<[u8]>>(db: &Database, path: P, hash: B) -> Result<()> {
    let write_txn = db.begin_write()?;
    {
        let mut table = write_txn.open_table(HASHES)?;
        let path_str = path
            .as_ref()
            .to_str()
            .context("Could not convert path to string.")?;

        table.insert(path_str, hash.as_ref())?;
    }

    Ok(())
}

/// Insert a page into the database. If the page already exists, the existing entry is updated.
pub fn insert_page(db: &Database, page: &Page) -> Result<()> {
    let write_txn = db.begin_write()?;
    {
        let mut table = write_txn.open_table(PAGES)?;

        let path_str = page
            .path
            .to_str()
            .context("Could not convert path to string.")?;
        let serialized_page = postcard::to_stdvec(page)?;

        table.insert(path_str, serialized_page.as_slice())?;
        insert_hash(db, path_str, page.source_hash.as_bytes())?;
    }

    Ok(())
}
