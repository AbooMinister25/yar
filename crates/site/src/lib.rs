#![allow(clippy::missing_errors_doc)]
#![allow(clippy::missing_panics_doc)]

pub mod config;

mod asset;
pub mod database;
mod entry;
mod page;
mod static_file;
mod templates;
mod utils;

use std::{collections::HashSet, fs, path::PathBuf, process::Command};

use chrono::Utc;
use color_eyre::{Result, eyre::OptionExt};
use config::Config;
use entry::{Entry, Typ, discover_entries};
use minijinja::{Environment, context};
use rayon::prelude::*;
use redb::Database;
use yar_markdown::MarkdownRenderer;

use crate::{
    asset::Asset,
    database::{get_pages, insert_hash, insert_page},
    page::Page,
    static_file::StaticFile,
    templates::{Template, create_environment, template_page::TemplatePage},
    utils::fs::ensure_directory,
};

struct Library {
    pub pages: Vec<Page>,
    pub assets: Vec<Asset>,
    pub static_files: Vec<StaticFile>,
    pub template_pages: Vec<TemplatePage>,
    pub templates: Vec<Template>,
    pub invalidated_pages: HashSet<PathBuf>,
}

impl Library {
    // Create an empty library.
    pub fn new() -> Self {
        Self {
            pages: vec![],
            assets: vec![],
            static_files: vec![],
            template_pages: vec![],
            templates: vec![],
            invalidated_pages: HashSet::new(),
        }
    }
}

/// A site to be built.
pub struct Site<'a> {
    db: Database,
    config: Config,
    environment: Environment<'a>,
    markdown_renderer: MarkdownRenderer,
    library: Library,
}

/// A helper enum that holds the different outputs `yar` works with.
enum Processed {
    Page(Page),
    Asset(Asset),
    StaticFile(StaticFile),
    TemplatePage(TemplatePage),
    Template(Template),
}

impl Site<'_> {
    /// Create a new site.
    pub fn new(db: Database, config: Config) -> Result<Self> {
        let markdown_renderer = MarkdownRenderer::new(
            config.site.syntax_theme_path.as_ref(),
            Some(&config.site.syntax_theme),
        )?;
        let env = create_environment(&config)?;

        Ok(Self {
            db,
            config,
            environment: env,
            markdown_renderer,
            library: Library::new(),
        })
    }

    /// Load all entries and process them.
    pub fn load(&mut self) -> Result<()> {
        let entries = discover_entries(&self.db, &self.config.site.root)?;
        println!("Discovered {} entries to build", entries.len());

        // Process the entries and collect all of the outputs.
        let processed = entries
            .into_par_iter()
            .map(|entry| {
                Ok(match entry.entry_type() {
                    Typ::Markdown => process_page(
                        entry,
                        &self.config,
                        &self.markdown_renderer,
                        &self.environment,
                    )?,
                    Typ::Asset => process_asset(entry, &self.config)?,
                    Typ::StaticFile => process_static_file(entry, &self.config)?,
                    Typ::TemplatePage => process_template_page(entry, &self.config)?,
                    Typ::Template => process_template(entry),
                })
            })
            .collect::<Result<Vec<Processed>>>()?;

        let mut processed_pages = vec![];

        for item in processed {
            match item {
                Processed::Page(p) => processed_pages.push(p),
                Processed::Asset(a) => self.library.assets.push(a),
                Processed::StaticFile(s) => self.library.static_files.push(s),
                Processed::TemplatePage(tp) => self.library.template_pages.push(tp),
                Processed::Template(t) => self.library.templates.push(t),
            }
        }

        // Get the paths of all the pages that were processed in this run, and thus
        // invalidated, and use that to pull all of the cached pages that are still valid.
        let invalidated_pages = processed_pages
            .iter()
            .map(|p| p.path.clone())
            .collect::<HashSet<PathBuf>>();
        let cached_pages = get_pages(&self.db, &invalidated_pages)?;

        self.library.invalidated_pages = invalidated_pages;
        self.library.pages = processed_pages
            .into_iter()
            .chain(cached_pages)
            .collect::<Vec<Page>>();

        println!("Built entries");
        Ok(())
    }

    /// Render the site to disk.
    pub fn render(&mut self) -> Result<()> {
        ensure_directory(&self.config.site.output_path)?;
        println!("Rendering site to disk");

        // If any templates have been modified, reload the environment.
        if !self.library.template_pages.is_empty() {
            self.reload_environment()?;
        }

        self.render_pages()?;
        self.library
            .assets
            .par_iter()
            .map(Asset::render)
            .collect::<Result<Vec<_>>>()?;

        self.library
            .static_files
            .par_iter()
            .map(StaticFile::render)
            .collect::<Result<Vec<_>>>()?;

        println!("Rendered site");
        Ok(())
    }

    /// Save the site to cache.
    pub fn save_to_cache(&mut self) -> Result<()> {
        println!("Caching site");

        let invalididated_pages = self
            .library
            .pages
            .iter()
            .filter(|p| self.library.invalidated_pages.contains(&p.path))
            .collect::<Vec<&Page>>();

        for page in invalididated_pages {
            insert_page(&self.db, page)?;
        }

        for asset in &self.library.assets {
            insert_hash(&self.db, &asset.path, asset.source_hash.as_bytes())?;
        }

        for static_file in &self.library.static_files {
            insert_hash(
                &self.db,
                &static_file.path,
                static_file.source_hash.as_bytes(),
            )?;
        }

        for template_page in &self.library.template_pages {
            insert_hash(
                &self.db,
                &template_page.path,
                template_page.source_hash.as_bytes(),
            )?;
        }

        for template in &self.library.templates {
            insert_hash(&self.db, &template.path, template.source_hash.as_bytes())?;
        }

        Ok(())
    }

    fn reload_environment(&mut self) -> Result<()> {
        self.environment = create_environment(&self.config)?;
        Ok(())
    }

    fn render_pages(&self) -> Result<()> {
        let pages_to_build = self
            .library
            .pages
            .iter()
            .filter(|p| self.library.invalidated_pages.contains(&p.path))
            .collect::<Vec<&Page>>();

        pages_to_build
            .par_iter()
            .filter(|p| self.config.site.development || !p.document.frontmatter.draft)
            .map(|p| p.render(&self.library.pages, &self.environment))
            .collect::<Result<Vec<_>>>()?;

        self.library
            .template_pages
            .par_iter()
            .filter(|t| self.config.site.development || !t.frontmatter.draft)
            .map(|t| t.render(&self.library.pages, &self.environment))
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
            pages => &self.library.pages,
        })?;
        fs::write(out_path, rendered)?;

        // Generate sitemap.
        let out_path = self.config.site.output_path.join("sitemap.xml");
        let template = self.environment.get_template("sitemap.xml")?;
        let rendered = template.render(context! {
            pages => &self.library.pages,
        })?;
        fs::write(out_path, rendered)?;

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
}

fn process_page(
    entry: Entry,
    config: &Config,
    markdown_renderer: &MarkdownRenderer,
    env: &Environment,
) -> Result<Processed> {
    let page = Page::new(
        entry.path,
        String::from_utf8(entry.raw_content)?.as_str(),
        entry.hash,
        &config.site.output_path,
        &config.site.root,
        &config.site.url,
        markdown_renderer,
        env,
    )?;
    Ok(Processed::Page(page))
}

fn process_asset(entry: Entry, config: &Config) -> Result<Processed> {
    let asset = Asset::new(
        entry.path,
        entry.hash,
        &config.site.output_path,
        &config.site.root,
        &config.site.url,
    )?;
    Ok(Processed::Asset(asset))
}

fn process_static_file(entry: Entry, config: &Config) -> Result<Processed> {
    let static_file = StaticFile::new(
        entry.path,
        entry.hash,
        &config.site.output_path,
        &config.site.root,
        &config.site.url,
    )?;
    Ok(Processed::StaticFile(static_file))
}

fn process_template_page(entry: Entry, config: &Config) -> Result<Processed> {
    let template_page = TemplatePage::new(
        &String::from_utf8(entry.raw_content)?,
        entry.hash,
        entry.path,
        &config.site.output_path,
        &config.site.root,
        &config.site.url,
    )?;
    Ok(Processed::TemplatePage(template_page))
}

fn process_template(entry: Entry) -> Processed {
    Processed::Template(Template::new(entry.path, entry.hash))
}
