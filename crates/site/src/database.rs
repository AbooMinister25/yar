use std::path::Path;

use color_eyre::{Result, eyre::ContextCompat};
use redb::{AccessGuard, Database, ReadableDatabase, ReadableTable, TableDefinition};

use crate::page::Page;

const PAGES: TableDefinition<&str, &[u8]> = TableDefinition::new("pages");
const HASHES: TableDefinition<&str, &[u8]> = TableDefinition::new("hashes");

/// Get the hash for a given path.
pub fn get_hash<P: AsRef<Path>>(db: &Database, path: P) -> Result<Option<AccessGuard<'_, &[u8]>>> {
    let read_txn = db.begin_read()?;
    let table = read_txn.open_table(HASHES)?;

    let path_str = path
        .as_ref()
        .to_str()
        .context("Could not convert path to string.")?;

    let hash = table.get(path_str)?;
    Ok(hash)
}

/// Get all the pages stored in the database.
pub fn load_pages(db: &Database) -> Result<Vec<Page>> {
    let read_txn = db.begin_read()?;
    let table = read_txn.open_table(PAGES)?;

    table
        .iter()?
        .map(|res| {
            let (_, bytes) = res?;
            postcard::from_bytes(bytes.value()).map_err(Into::into)
        })
        .collect::<Result<Vec<Page>>>()
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
    }

    Ok(())
}
