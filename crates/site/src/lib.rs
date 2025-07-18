#![allow(clippy::missing_errors_doc)]
#![allow(clippy::missing_panics_doc)]

pub mod config;
pub mod sql;

mod asset;
mod entry;
mod page;
mod static_file;
mod templates;
mod utils;

use std::{collections::HashSet, ffi::OsStr, fs, rc::Rc};

use chrono::Utc;
use color_eyre::Result;
use config::Config;
use entry::discover_entries;
use markdown::MarkdownRenderer;
use minijinja::{Environment, Value, context};
use rusqlite::Connection;
use smol_str::SmolStr;

use crate::{
    asset::Asset,
    page::Page,
    sql::{
        get_pages, get_tags, insert_or_update_asset, insert_or_update_page,
        insert_or_update_static_file,
    },
    static_file::StaticFile,
    templates::create_environment,
    utils::fs::ensure_directory,
};

/// A site to be built.
pub struct Site<'a> {
    conn: Connection,
    config: Config,
    pages: Vec<Rc<Page>>,
    pages_to_build: Vec<Rc<Page>>,
    assets: Vec<Asset>,
    static_files: Vec<StaticFile>,
    tags: HashSet<SmolStr>,
    environment: Environment<'a>,
    markdown_renderer: MarkdownRenderer,
}

impl Site<'_> {
    /// Create a new site.
    pub fn new(conn: Connection, config: Config) -> Result<Self> {
        let markdown_renderer = MarkdownRenderer::new(
            config.site.syntax_theme_path.as_ref(),
            Some(&config.site.syntax_theme),
        )?;
        let env = create_environment(&config)?;

        Ok(Self {
            conn,
            config,
            pages: Vec::new(),
            pages_to_build: Vec::new(),
            assets: Vec::new(),
            static_files: Vec::new(),
            tags: HashSet::new(),
            environment: env,
            markdown_renderer,
        })
    }

    /// Loads the site, finding and building changed/new entries.
    ///
    /// Keep in mind that if this is run without the previous iteration
    /// of the site being committed to the database with `Site.commit_to_db`,
    /// everything built in that iteration will be rebuilt.
    pub fn load(&mut self) -> Result<()> {
        self.pages.clear();
        self.assets.clear();
        self.static_files.clear();
        self.tags.clear();

        let entries = discover_entries(&self.config.site.root, &self.conn)?;
        println!("Discovered {} entries to build", entries.len());

        for entry in entries {
            match entry.path.extension().and_then(OsStr::to_str) {
                Some("md") => {
                    let page = Page::new(
                        entry.path,
                        &String::from_utf8(entry.raw_content)?,
                        entry.hash,
                        &self.config.site.output_path,
                        &self.config.site.root,
                        &self.config.site.url,
                        &self.markdown_renderer,
                        &self.environment,
                    )?;
                    let tags = page.document.frontmatter.tags.clone();
                    self.tags.extend(tags);

                    let page_rc = Rc::new(page);
                    self.pages.push(Rc::clone(&page_rc));
                    self.pages_to_build.push(Rc::clone(&page_rc));
                }
                Some("css" | "scss" | "js") => {
                    let asset = Asset::new(
                        entry.path,
                        entry.hash,
                        &self.config.site.output_path,
                        &self.config.site.root,
                        &self.config.site.url,
                    )?;
                    self.assets.push(asset);
                }
                _ => {
                    // Copy over any remaining extensions as-is.
                    let static_file = StaticFile::new(
                        entry.path,
                        entry.hash,
                        &self.config.site.output_path,
                        &self.config.site.root,
                        &self.config.site.url,
                    )?;
                    self.static_files.push(static_file);
                }
            }
        }

        let remaining_pages = get_pages(
            &self.conn,
            self.pages_to_build
                .iter()
                .map(|p| p.path.as_path())
                .collect(),
        )?;
        self.pages.extend(remaining_pages.into_iter().map(Rc::new));

        let tags = get_tags(&self.conn)?;
        self.tags.extend(tags.iter().map(std::convert::Into::into));

        // TODO: I don't like that this is being added here, but we'll leave it for now. Find
        // TODO: a more elegant fix later.
        self.environment
            .add_global("tags", Value::from_serialize(&self.tags));

        Ok(())
    }

    /// Renders the site to disk.
    pub fn render(&self) -> Result<()> {
        ensure_directory(&self.config.site.output_path)?;

        for page in &self.pages {
            page.render(&self.pages, &self.environment)?;
        }

        for asset in &self.assets {
            asset.render()?;
        }

        for static_file in &self.static_files {
            static_file.render()?;
        }

        // Generate 404 page.
        let out_path = self.config.site.output_path.join("404.html");
        let template = self.environment.get_template("404.html")?;
        let rendered = template.render(context! {})?;
        fs::write(out_path, rendered)?;

        // Generate atom feed.
        let out_path = self.config.site.output_path.join("atom.xml");
        let template = self.environment.get_template("atom.xml")?;
        let last_updated = Utc::now();
        let feed_url = self.config.site.url.join("atom.xml")?;

        let rendered = template.render(context! {
            last_updated => last_updated,
            feed_url => feed_url,
            pages => &self.pages,
        })?;
        fs::write(out_path, rendered)?;

        // Generate sitemap.
        let out_path = self.config.site.output_path.join("sitemap.xml");
        let template = self.environment.get_template("sitemap.xml")?;
        let rendered = template.render(context! {
            pages => &self.pages,
        })?;
        fs::write(out_path, rendered)?;

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
