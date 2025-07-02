mod asset;
pub mod config;
mod entry;
mod page;
pub mod sql;
mod static_file;
mod utils;

use std::ffi::OsStr;

use color_eyre::Result;
use config::Config;
use entry::discover_entries;
use minijinja::{Environment, path_loader};
use rusqlite::Connection;

use crate::{
    asset::Asset,
    page::Page,
    sql::{get_pages, insert_or_update_asset, insert_or_update_page, insert_or_update_static_file},
    static_file::StaticFile,
};

/// A site to be built.
pub struct Site<'a> {
    conn: Connection,
    config: Config,
    pages: Vec<Page>,
    assets: Vec<Asset>,
    static_files: Vec<StaticFile>,
    index: Vec<Page>,
    environment: Environment<'a>,
}

impl<'a> Site<'a> {
    /// Create a new site.
    pub fn new(conn: Connection, config: Config) -> Result<Self> {
        let entries = discover_entries(&config.root, &conn)?;
        println!("Discovered {} entries to build", entries.len());

        let mut pages = Vec::new();
        let mut assets = Vec::new();
        let mut static_files = Vec::new();

        for entry in entries {
            match entry.path.extension().and_then(OsStr::to_str) {
                Some("md") => {
                    let page = Page::new(
                        entry.path,
                        String::from_utf8(entry.raw_content)?,
                        entry.hash,
                        &config.output_path,
                        &config.root,
                        &config.url,
                    )?;
                    pages.push(page);
                }
                Some("css") | Some("scss") | Some("js") => {
                    let asset = Asset::new(
                        entry.path,
                        entry.hash,
                        &config.output_path,
                        &config.root,
                        &config.url,
                    )?;
                    assets.push(asset);
                }
                _ => {
                    // Copy over any remaining extensions as-is.
                    let static_file = StaticFile::new(
                        entry.path,
                        entry.hash,
                        &config.output_path,
                        &config.root,
                        &config.url,
                    )?;
                    static_files.push(static_file)
                }
            }
        }

        // Get all of the pages in the database save from the ones we are building/rebuilding right now.
        let index = get_pages(&conn, pages.iter().map(|p| p.path.as_path()).collect())?;

        let mut env = Environment::new();
        env.set_loader(path_loader(&config.root.join("templates")));

        Ok(Self {
            conn,
            config,
            pages,
            assets,
            static_files,
            index,
            environment: env,
        })
    }

    /// Renders the site to disk.
    pub fn render(&self) -> Result<()> {
        let combined_index = self.index.iter().chain(&self.pages).collect::<Vec<&Page>>();
        for page in &self.pages {
            page.render(&combined_index, &self.environment)?;
        }

        for asset in &self.assets {
            asset.render()?;
        }

        for static_file in &self.static_files {
            static_file.render()?;
        }

        Ok(())
    }

    /// Commit the state of the site to the database.
    pub fn commit_to_db(&self) -> Result<()> {
        for page in &self.pages {
            insert_or_update_page(&self.conn, page)?;
        }

        for asset in &self.assets {
            insert_or_update_asset(&self.conn, asset)?;
        }

        for static_file in &self.static_files {
            insert_or_update_static_file(&self.conn, static_file)?;
        }

        Ok(())
    }
}
