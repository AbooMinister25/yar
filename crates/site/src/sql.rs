use std::path::{Path, PathBuf};

use color_eyre::{Result, eyre::ContextCompat};
use markdown::{Document, Frontmatter};
use rusqlite::Connection;
use url::Url;

use crate::{asset::Asset, page::Page, static_file::StaticFile};

/// Set up sqlite database.
/// Create initial tables if they don't exist and acquire the connection.
pub fn setup_sql() -> Result<Connection> {
    let conn = Connection::open("site.db")?;

    conn.execute(
        "
        CREATE TABLE IF NOT EXISTS entries (
            path VARCHAR NOT NULL PRIMARY KEY,
            hash TEXT NOT NULL
        )
    ",
        (),
    )?;

    conn.execute(
        "
        CREATE TABLE IF NOT EXISTS pages (
            out_path VARCHAR NOT NULL PRIMARY KEY,
            permalink TEXT NOT NULL,
            date TEXT NOT NULL,
            updated TEXT NOT NULL,
            content TEXT NOT NULL,
            toc JSON NOT NULL,
            summary TEXT NOT NULL,
            title TEXT NOT NULL,
            tags JSON NOT NULL,
            template TEXT,
            slug TEXT,
            draft BOOLEAN NOT NULL,
            requires JSON NOT NULL,
            entry VARCHAR NOT NULL,
            FOREIGN KEY(entry) REFERENCES entries(path)
        )
    ",
        (),
    )?;

    conn.execute(
        "
        CREATE TABLE IF NOT EXISTS assets (
            out_path VARCHAR NOT NULL PRIMARY KEY,
            permalink TEXT NOT NULL,
            content TEXT NOT NULL,
            entry VARCHAR NOT NULL,
            FOREIGN KEY(entry) REFERENCES entries(path)
        )
    ",
        (),
    )?;

    conn.execute(
        "
        CREATE TABLE IF NOT EXISTS static_files (
            out_path VARCHAR NOT NULL PRIMARY KEY,
            permalink TEXT NOT NULL,
            content BLOB NOT NULL,
            entry VARCHAR NOT NULL,
            FOREIGN KEY(entry) REFERENCES entries(path)
        )
    ",
        (),
    )?;

    conn.execute(
        "
        CREATE TABLE IF NOT EXISTS tags (
            name TEXT NOT NULL PRIMARY KEY
        )
    ",
        (),
    )?;

    conn.execute(
        "
        CREATE TABLE IF NOT EXISTS tags_pages (
            tag_name TEXT NOT NULL,
            page_path VARCHAR NOT NULL,
            PRIMARY KEY (tag_name, page_path),
            FOREIGN KEY (tag_name) REFERENCES tags(name),
            FOREIGN KEY (page_path) REFERENCES pages(out_path)
        )
    ",
        (),
    )?;

    Ok(conn)
}

/// Get hashes for a given path.
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

/// Get all pages in the database.
///
/// Excludes any pages with the paths provided.
#[allow(clippy::needless_pass_by_value)]
pub fn get_pages(conn: &Connection, exclusions: Vec<&Path>) -> Result<Vec<Page>> {
    let mut stmt = conn.prepare(
        "
    SELECT out_path, 
        permalink, 
        date, 
        updated, 
        content, 
        toc, 
        summary, 
        title, 
        tags, 
        template, 
        slug, 
        draft,
        requires,
        entry
    FROM pages
    ",
    )?;

    let pages_iter = stmt.query_map([], |row| {
        let out_path: String = row.get(0)?;
        let permalink: String = row.get(1)?;
        let tags: String = row.get(8)?;
        let parsed_tags = serde_json::from_str(&tags).expect("JSON should be valid.");
        let toc: String = row.get(5)?;
        let parsed_toc = serde_json::from_str(&toc).expect("JSON should be valid.");
        let requires: String = row.get(12)?;
        let parsed_requires = serde_json::from_str(&requires).expect("JSON should be valid.");

        let frontmatter = Frontmatter {
            title: row.get(7)?,
            tags: parsed_tags,
            template: row.get(9)?,
            date: Some(row.get(2)?),
            updated: Some(row.get(3)?),
            slug: row.get(10)?,
            draft: row.get(11)?,
            requires: parsed_requires,
        };

        let document = Document {
            date: row.get(2)?,
            updated: row.get(3)?,
            content: row.get(4)?,
            toc: parsed_toc,
            summary: row.get(6)?,
            frontmatter,
        };

        let entry_path: String = row.get(13)?;
        let mut entry_stmt = conn.prepare("SELECT hash FROM entries WHERE path = ?")?;
        let hash = entry_stmt
            .query_map([&entry_path], |row| row.get(0))?
            .next()
            .expect("No corresponding entry for page in database?")?;

        Ok(Page {
            path: PathBuf::from(entry_path),
            source_hash: hash,
            out_path: PathBuf::from(out_path),
            permalink: Url::parse(&permalink).expect("URL should be valid."),
            document,
        })
    })?;

    let mut pages = Vec::new();
    for page in pages_iter {
        let p = page?;
        if !exclusions.contains(&p.path.as_path()) {
            pages.push(p);
        }
    }

    Ok(pages)
}

/// Get all tags in the database
pub fn get_tags(conn: &Connection) -> Result<Vec<String>> {
    let mut stmt = conn.prepare("SELECT name FROM tags")?;
    let mut rows = stmt.query([])?;

    let mut tags = Vec::new();
    while let Some(row) = rows.next()? {
        tags.push(row.get(0)?);
    }

    Ok(tags)
}

/// Insert a page into the database. If it already exists, update the existing entry.
pub fn insert_or_update_page(conn: &Connection, page: &Page) -> Result<()> {
    conn.execute(
        "
        INSERT INTO entries (path, hash) VALUES (?1, ?2)
        ON CONFLICT (path) DO UPDATE SET hash = (?2)
        ",
        (
            &page.path.to_str().context("Path should be valid unicode")?,
            &page.source_hash,
        ),
    )?;

    conn.execute(
        "
        INSERT INTO pages ( 
            out_path, permalink, date, updated, content, toc, summary, title, tags, template, slug, draft, requires, entry
        ) VALUES (
            ?1, ?2, datetime(?3), datetime(?4), ?5, json(?6), ?7, ?8, json(?9), ?10, ?11, ?12, ?13, ?14
        ) ON CONFLICT (out_path) DO UPDATE SET permalink = ?2,
            date = datetime(?3),
            updated = datetime(?4),
            content = ?5,
            toc = json(?6),
            summary = ?7,
            title = ?8,
            tags = json(?9),
            template = ?10,
            slug = ?11,
            draft = ?12,
            requires = ?13
    ",
        (
            &page.out_path.to_str().context("Path should be valid unicode.")?,
            &page.permalink.as_str(),
            &page.document.date,
            &page.document.updated,
            &page.document.content,
            &serde_json::to_string(&page.document.toc)?,
            &page.document.summary,
            &page.document.frontmatter.title,
            &serde_json::to_string(&page.document.frontmatter.tags)?,
            &page.document.frontmatter.template,
            &page.document.frontmatter.slug,
            &page.document.frontmatter.draft,
            &serde_json::to_string(&page.document.frontmatter.requires)?,
            &page.path.to_str().context("Path should be valid unicode.")?,
        ),
    )?;

    Ok(())
}

/// Insert an asset into the database. If it already exists, update the existing entry.
pub fn insert_or_update_asset(conn: &Connection, asset: &Asset) -> Result<()> {
    conn.execute(
        "
        INSERT INTO entries (path, hash) VALUES (?1, ?2)
        ON CONFLICT (path) DO UPDATE SET hash = (?2)
        ",
        (
            &asset
                .path
                .to_str()
                .context("Path should be valid unicode")?,
            &asset.source_hash,
        ),
    )?;

    conn.execute(
        "
        INSERT INTO assets (out_path, permalink, content, entry)
        VALUES (?1, ?2, ?3, ?4)
        ON CONFLICT (out_path) DO UPDATE SET permalink = ?2,
            content = ?3
        ",
        (
            &asset
                .out_path
                .to_str()
                .context("Path should be valid unicode")?,
            &asset.permalink.as_str(),
            &asset.content,
            &asset
                .path
                .to_str()
                .context("Path should be valid unicode")?,
        ),
    )?;

    Ok(())
}

/// Insert a static asset into the database. If it already exists, update the existing entry.
pub fn insert_or_update_static_file(conn: &Connection, static_file: &StaticFile) -> Result<()> {
    conn.execute(
        "
        INSERT INTO entries (path, hash) VALUES (?1, ?2)
        ON CONFLICT (path) DO UPDATE SET hash = (?2)
        ",
        (
            &static_file
                .path
                .to_str()
                .context("Path should be valid unicode")?,
            &static_file.source_hash,
        ),
    )?;

    conn.execute(
        "
        INSERT INTO static_files (out_path, permalink, content, entry)
        VALUES (?1, ?2, ?3, ?4)
        ON CONFLICT (out_path) DO UPDATE SET permalink = ?2,
            content = ?3
        ",
        (
            &static_file
                .out_path
                .to_str()
                .context("Path should be valid unicode")?,
            &static_file.permalink.as_str(),
            &static_file.content,
            &static_file
                .path
                .to_str()
                .context("Path should be valid unicode")?,
        ),
    )?;

    Ok(())
}

/// Insert the tag, or if it already exists, do nothing.
pub fn insert_tag(conn: &Connection, tag: &str) -> Result<()> {
    let mut stmt = conn.prepare(
        "
        INSERT INTO tags (name) VALUES (?1)
        ON CONFLICT (name) DO NOTHING
        ",
    )?;

    stmt.execute((tag,))?;

    Ok(())
}

// /// Insert tag map for page at given path.
// pub fn insert_tagmaps(conn: &Connection, path: &Path, tags: &[String]) -> Result<()> {

// }
