use std::path::{Path, PathBuf};

use color_eyre::{Result, eyre::ContextCompat};
use markdown::{Document, Frontmatter, SeriesInfo};
use rusqlite::Connection;

use crate::{asset::Asset, page::Page, static_file::StaticFile};

/// Set up SQLite database.
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
            series_part INTEGER,
            slug TEXT,
            draft BOOLEAN NOT NULL,
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
        series_part, 
        slug, 
        draft, 
        entry
    FROM pages
    ",
    )?;

    let pages_iter = stmt.query_map([], |row| {
        let out_path: String = row.get(0)?;
        let series_part: Option<i32> = row.get(10)?;
        let tags: String = row.get(8)?;
        let parsed_tags = serde_json::from_str(&tags).expect("JSON should be valid.");
        let toc: String = row.get(5)?;
        let parsed_toc = serde_json::from_str(&toc).expect("JSON should be valid.");

        let frontmatter = Frontmatter {
            title: row.get(7)?,
            tags: parsed_tags,
            template: row.get(9)?,
            date: Some(row.get(2)?),
            updated: Some(row.get(3)?),
            series: series_part.map(|n| SeriesInfo { part: n }),
            slug: row.get(11)?,
            draft: row.get(12)?,
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
            permalink: row.get(1)?,
            document,
        })
    })?;

    let mut pages = Vec::new();
    for page in pages_iter {
        let p = page?;
        if !exclusions.contains(&p.path.as_path()) {
            pages.push(p)
        }
    }

    Ok(pages)
}

/// Insert a page into the database. If it already exists, update the existing entry.
pub fn insert_or_update_page(conn: &Connection, page: &Page) -> Result<()> {
    let series_part = page.document.frontmatter.series.as_ref().map(|si| si.part);

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
            out_path, permalink, date, updated, content, toc, summary, title, tags, template, series_part, slug, draft, entry
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
            series_part = ?11,
            slug = ?12,
            draft = ?13
    ",
        (
            &page.out_path.to_str().context("Path should be valid unicode.")?,
            &page.permalink,
            &page.document.date,
            &page.document.updated,
            &page.document.content,
            &serde_json::to_string(&page.document.toc)?,
            &page.document.summary,
            &page.document.frontmatter.title,
            &serde_json::to_string(&page.document.frontmatter.tags)?,
            &page.document.frontmatter.template,
            &series_part,
            &page.document.frontmatter.slug,
            &page.document.frontmatter.draft,
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
            &asset.permalink,
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
            &static_file.permalink,
            &static_file.content,
            &static_file
                .path
                .to_str()
                .context("Path should be valid unicode")?,
        ),
    )?;

    Ok(())
}
