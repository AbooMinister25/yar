#![allow(clippy::missing_errors_doc)]
#![allow(clippy::missing_panics_doc)]

pub mod config;
pub mod sql;

mod asset;
mod entry;
mod extensions;
mod page;
mod static_file;
mod templates;
mod utils;

use std::{collections::HashSet, ffi::OsStr, fs, io, path::PathBuf, process::Command, sync::{Arc, atomic::{AtomicBool, Ordering}}};

use chrono::Utc;
use color_eyre::{Result, eyre::OptionExt};
use config::Config;
use crossbeam::channel::{Receiver, Sender, bounded};
use entry::discover_entries;
use minijinja::{Environment, Value, context};
use rayon::prelude::*;
use rusqlite::Connection;
use smol_str::SmolStr;
use yar_markdown::MarkdownRenderer;

use crate::{
    asset::Asset,
    page::Page,
    sql::{
        get_pages, get_pages_for_template, get_tags, get_template_pages, insert_or_update_asset,
        insert_or_update_page, insert_or_update_static_file, insert_or_update_template,
        insert_or_update_template_page, insert_tag,
    },
    static_file::StaticFile,
    templates::{create_environment, discover_templates, template_page::TemplatePage},
    utils::fs::ensure_directory,
};

/// A site to be built.
pub struct Site<'a> {
    conn: Connection,
    config: Config,
    pages: Vec<Arc<Page>>,
    pages_to_build: HashSet<Arc<Page>>,
    assets: Vec<Asset>,
    static_files: Vec<StaticFile>,
    template_pages: HashSet<TemplatePage>,
    templates: Vec<(PathBuf, String)>,
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
            pages_to_build: HashSet::new(),
            assets: Vec::new(),
            static_files: Vec::new(),
            template_pages: HashSet::new(),
            templates: Vec::new(),
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
    ///
    /// TODO: refactor this into multiple functions so it's easier to follow.
    #[allow(clippy::too_many_lines)]
    pub fn load(&mut self) -> Result<()> {
        self.pages.clear();
        self.assets.clear();
        self.static_files.clear();
        self.template_pages.clear();
        self.tags.clear();

        let entries = discover_entries(&self.config.site.root, &self.conn)?;
        println!("Discovered {} entries to build", entries.len());

        let (page_tx, page_rx): (Sender<Page>, Receiver<Page>) = bounded(100);
        let (asset_tx, asset_rx) = bounded(100);
        let (static_file_tx, static_file_rx) = bounded(100);
        let (template_page_tx, template_page_rx) = bounded(100);

        let page_handle = std::thread::spawn(|| {
            let mut pages = Vec::new();
            let mut pages_to_build = HashSet::new();
            let mut tags: HashSet<SmolStr> = HashSet::new();

            for page in page_rx.into_iter().map(Arc::new) {
                let page_tags = page.document.frontmatter.tags.clone();
                tags.extend(page_tags);

                pages.push(Arc::clone(&page));
                pages_to_build.insert(Arc::clone(&page));
            }

            (pages, pages_to_build, tags)
        });

        let asset_handle = std::thread::spawn(|| {
            let mut assets = Vec::new();

            for asset in asset_rx {
                assets.push(asset);
            }

            assets
        });

        let static_file_handle = std::thread::spawn(|| {
            let mut static_files = Vec::new();

            for static_file in static_file_rx {
                static_files.push(static_file);
            }

            static_files
        });

        let template_page_handle = std::thread::spawn(|| {
            let mut template_pages = HashSet::new();

            for template_page in template_page_rx {
                template_pages.insert(template_page);
            }

            template_pages
        });

        let templates_modified = AtomicBool::new(false);

        entries
            .into_par_iter()
            .map(|entry| {
                let page_tx = page_tx.clone();
                let asset_tx = asset_tx.clone();
                let static_file_tx = static_file_tx.clone();

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
                        page_tx.send(page)?;
                    }
                    Some("css" | "scss" | "js") => {
                        let asset = Asset::new(
                            entry.path,
                            entry.hash,
                            &self.config.site.output_path,
                            &self.config.site.root,
                            &self.config.site.url,
                        )?;
                        asset_tx.send(asset)?;
                    }
                    Some("html") => {
                        if entry
                            .path
                            .parent()
                            .is_some_and(|p| p.file_name().is_some_and(|s| s == "templates"))
                        {
                            templates_modified.store(true, Ordering::Relaxed);
                        } else {
                            let template_page = TemplatePage::new(
                                &String::from_utf8(entry.raw_content)?,
                                entry.hash,
                                entry.path,
                                &self.config.site.output_path,
                                &self.config.site.root,
                                &self.config.site.url,
                            )?;
                            template_page_tx.send(template_page)?;
                        }
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
                        static_file_tx.send(static_file)?;
                    }
                }

                Ok(())
            })
            .collect::<Result<Vec<_>>>()?;

        drop(page_tx);
        drop(asset_tx);
        drop(static_file_tx);
        drop(template_page_tx);

        self.templates = discover_templates(self.config.site.root.join("templates"), &self.conn)?;

        let mut pages_for_template = HashSet::new();
        for template in &self.templates {
            pages_for_template.extend(
                get_pages_for_template(
                    &self.conn,
                    template
                        .0
                        .file_name()
                        .ok_or_eyre("Template doesn't have file name")?
                        .to_str()
                        .ok_or_eyre("Template file name is not valid UTF-8.")?,
                )?
                .into_iter()
                .map(Arc::new),
            );
        }

        if !pages_for_template.is_empty() {
            println!(
                "Templates changed...rebuilding {} pages that depend on changed templates",
                pages_for_template.len()
            );
        }

        // Join the consumer threads.
        let (pages, pages_to_build, tags) = page_handle
            .join()
            .map_err(|e| io::Error::other(format!("Collector thread panicked: {e:?}")))?;
        self.pages = pages;
        self.pages_to_build = pages_to_build;
        self.tags = tags;

        self.pages_to_build.extend(pages_for_template);

        let assets = asset_handle
            .join()
            .map_err(|e| io::Error::other(format!("Collector thread panicked: {e:?}")))?;
        self.assets = assets;

        let static_files = static_file_handle
            .join()
            .map_err(|e| io::Error::other(format!("Collector thread panicked: {e:?}")))?;
        self.static_files = static_files;

        let template_pages = template_page_handle
            .join()
            .map_err(|e| io::Error::other(format!("Collector thread panicked: {e:?}")))?;
        self.template_pages = template_pages;

        // Get remaining pages (those that aren't being processed in this run of the static site generator) from the database.
        let remaining_pages = get_pages(
            &self.conn,
            self.pages_to_build
                .iter()
                .map(|p| p.path.as_path())
                .collect(),
        )?;
        self.pages.extend(remaining_pages.into_iter().map(Arc::new));

        // Same as above, but for tags.
        let tags = get_tags(&self.conn)?;
        let it = tags.iter().map(std::convert::Into::into);
        let hs = it.collect::<HashSet<_>>();

        // Find all the newly created tags and queue pages that depend on them for a rebuild.
        // TODO: replace this when I create a more sophisticated dependency system.
        let mut difference = self.tags.difference(&hs).peekable();
        if difference.peek().is_some() {
            let pag = get_template_pages(&self.conn, "tags")?;
            self.template_pages.extend(pag);
        }

        self.tags.extend(hs);

        let depends_on_pages = get_template_pages(&self.conn, "pages")?;
        self.template_pages.extend(depends_on_pages);

        // TODO: I don't like that this is being added here, but we'll leave it for now. Find
        // TODO: a more elegant fix later.
        self.environment
            .add_global("tags", Value::from_serialize(&self.tags));

        println!("Loaded entries");

        Ok(())
    }

    /// Renders the site to disk.
    pub fn render(&self) -> Result<()> {
        ensure_directory(&self.config.site.output_path)?;

        let pages = &self.pages;
        let environment = &self.environment;
        let dev = self.config.site.development;
        self.pages_to_build
            .par_iter()
            .filter(|p| dev || !p.document.frontmatter.draft)
            .map(|p| p.render(pages, environment))
            .collect::<Result<Vec<_>>>()?;

        self.assets
            .par_iter()
            .map(Asset::render)
            .collect::<Result<Vec<_>>>()?;

        self.static_files
            .par_iter()
            .map(StaticFile::render)
            .collect::<Result<Vec<_>>>()?;

        self.template_pages
            .par_iter()
            .filter(|t| dev || !t.frontmatter.draft)
            .map(|t| t.render(pages, environment))
            .collect::<Result<Vec<_>>>()?;

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

        println!("Wrote site to disk");

        Ok(())
    }

    fn reload_templates(&mut self) -> Result<()> {
        self.environment = create_environment(&self.config)?;
        Ok(())
    }

    /// Run post hooks (hooks that are to be run once the static site generator has finished running).
    pub fn run_post_hooks(&self) -> Result<()> {
        for hook in &self.config.hooks.post {
            println!("Running hook with command {}", hook.cmd);
            let mut split = hook.cmd.split_whitespace();
            let cmd = split
                .next()
                .ok_or_eyre(format!("Post hook command {} not valid.", hook.cmd))?;
            let args = split.collect::<Vec<&str>>();

            let output = Command::new(cmd).args(args).output()?;
            println!("Hook completed with status {}", output.status);
            println!("STDERR: {}", String::from_utf8_lossy(&output.stderr));
            println!("STDOUT: {}", String::from_utf8_lossy(&output.stdout));
        }

        Ok(())
    }

    /// Commit the state of the site to the database.
    pub fn commit_to_db(&mut self) -> Result<()> {
        let tx = self.conn.transaction()?;

        for page in &self.pages {
            insert_or_update_page(&tx, page)?;
        }

        for asset in &self.assets {
            insert_or_update_asset(&tx, asset)?;
        }

        for static_file in &self.static_files {
            insert_or_update_static_file(&tx, static_file)?;
        }

        for template_page in &self.template_pages {
            insert_or_update_template_page(&tx, template_page)?;
        }

        for template in &self.templates {
            insert_or_update_template(&tx, template)?;
        }

        for tag in &self.tags {
            insert_tag(&tx, tag)?;
        }

        tx.commit()?;

        println!("Committed site state to database");

        Ok(())
    }
}
