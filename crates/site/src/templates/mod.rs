pub mod template_page;

mod functions;

use std::{fs, io, path::Path, sync::Arc};

use color_eyre::{Result, eyre::OptionExt};
use crossbeam::channel::bounded;
use ignore::{WalkBuilder, WalkState};
use minijinja::{Environment, Value, context, path_loader, value::Object};
use rayon::prelude::*;
use rusqlite::Connection;

use crate::{
    config::Config, page::Page, sql::get_template_hashes, templates::functions::pages_in_section,
};

const DEFAULT_404: &str = r#"
<!DOCTYPE html>
<h1> Page Not Found</h1>
<a href="{{ site.url | safe }}">Home</a>
"#;

const DEFAULT_ATOM_FEED: &str = r#"
<?xml version="1.0" encoding="UTF-8">
<feed xmlns="http://www.w3.org/2005/Atom">
    <title> {{ site.title | default("Unknown") }} </title>
    <link href="{{ feed_url | safe }}" rel="self" />
    <link href="{{ site.url | safe }}"/>
    <updated> {{ last_updated | datetimeformat(format="iso") }} </updated>
    <id> {{ feed_url | safe }} </id>
    {% for page in pages %}
    {% if page.path is not endingwith "index.md" %}
    <entry>
        <title> {{ page.document.frontmatter.title }} </title>
        <published> {{ page.document.date | datetimeformat(format="iso") }} </published>
        <updated> {{ page.document.updated | datetimeformat(format="iso") }} </updated>
        <id> {{ page.permalink | safe }} </id>
        {% if site.authors %}
            {% for author in site.authors %}
            <author>
                <name> {{ author }} </name>
            </author>
            {% endfor %}
        {% else %}
            <author>
                <name> Unknown </name>
            </author>
        {% endif %}
        <summary> {{ page.document.summary | safe }} </summary>
        <content type="html">
            {{ page.document.content | safe }}
        </content>
    </entry>
    {% endif %}
    {% endfor %}
</feed>
"#;

const DEFAULT_SITEMAP: &str = r#"
<?xml version="1.0" encoding="UTF-8"?>
<urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">
    {%- for page in pages %}
    <url>
        <loc>{{ page.permalink | safe }}</loc>
        <lastmod>{{ page.document.updated }}</lastmod>
    </url>
    {%- endfor %}
</urlset>
"#;

/// The context that is passed to pages when they are rendered.
#[derive(Debug)]
pub struct PageContext {
    pub pages: Vec<Arc<Page>>,
}

impl Object for PageContext {
    fn get_value(self: &Arc<Self>, field: &Value) -> Option<Value> {
        match field.as_str()? {
            "pages" => Some(Value::from_serialize(&self.pages)),
            _ => None,
        }
    }
}

/// Initialize the template environment.
///
/// Loads all templates from the templates directory, some defaults
/// defined in this file, and global variables.
pub fn create_environment(config: &Config) -> Result<Environment<'static>> {
    let mut env = Environment::new();

    env.add_template("404.html", DEFAULT_404)?;
    env.add_template("atom.xml", DEFAULT_ATOM_FEED)?;
    env.add_template("sitemap.xml", DEFAULT_SITEMAP)?;
    env.set_loader(path_loader(&config.site.root.join("templates")));
    env.add_global(
        "site",
        context! {
            url => config.site.url,
            authors => config.site.authors,
            title => config.site.title,
            description => config.site.description,
        },
    );
    env.add_function("pages_in_section", pages_in_section);
    minijinja_contrib::add_to_environment(&mut env);

    Ok(env)
}

/// Discovers templates from the `templates` directory. Returns a collection
/// of templates that have been modified or newly created from the previous run.
pub fn discover_templates<T: AsRef<Path>>(path: T, conn: &Connection) -> Result<Vec<String>> {
    let mut ret = Vec::new();

    let (tx, rx) = bounded(100);
    let handle = std::thread::spawn(|| {
        let mut templates = Vec::new();

        for template in rx {
            templates.push(template);
        }

        templates
    });

    WalkBuilder::new(path).build_parallel().run(|| {
        let tx = tx.clone();

        Box::new(move |path| {
            if let Ok(p) = path {
                if !p.path().is_dir() {
                    let content = fs::read(p.path()).expect("Error reading from file.");
                    tx.send((p.into_path(), content))
                        .expect("Error while sending.");
                }
            }

            WalkState::Continue
        })
    });

    drop(tx);

    let templates = handle
        .join()
        .map_err(|e| io::Error::other(format!("Collector thread panicked: {e:?}")))?;

    let hashes = templates
        .par_iter()
        .map(|(_, s)| format!("{:016x}", seahash::hash(s)))
        .collect::<Vec<String>>();

    for ((path, _), hash) in templates.into_iter().zip(hashes) {
        let hashes = get_template_hashes(conn, &path)?;

        if hashes.is_empty() || hashes[0].1 != hash {
            ret.push(
                path.file_name()
                    .ok_or_eyre("Template doesn't have file name")?
                    .to_str()
                    .ok_or_eyre("Template file name is not valid UTF-8.")?
                    .to_string(),
            );
        }
    }

    Ok(ret)
}

#[cfg(test)]
mod tests {
    use chrono::{TimeZone, Utc};
    use url::Url;
    use yar_markdown::MarkdownRenderer;

    use crate::page::Page;

    use super::*;

    fn make_pages() -> Result<Vec<Page>> {
        let pages = (0..10)
            .collect::<Vec<_>>()
            .iter()
            .map(|n| {
                format!(
                    r#"
---
title = "post-{n}"
tags = ["foo"]
template = "page.html"
date = "2025-01-01T6:00:00"
updated = "2025-03-12T8:00:00"
---

Hello World
        "#
                )
            })
            .enumerate()
            .map(|(n, s)| {
                Page::new(
                    format!("site/_content/series/testing/post-{n}.md"),
                    &s,
                    "hashplaceholder".to_string(),
                    "public/",
                    "site/",
                    &Url::parse("https://example.com")?,
                    &MarkdownRenderer::new::<&str>(None, None)?,
                    &Environment::empty(),
                )
            })
            .collect::<Result<Vec<Page>>>()?;

        Ok(pages)
    }

    #[test]
    fn test_render_default_404_template() -> Result<()> {
        let env = create_environment(&Config::default())?;
        let rendered = env.get_template("404.html")?.render(context! {})?;

        insta::assert_yaml_snapshot!(rendered);

        Ok(())
    }

    #[test]
    fn test_render_default_atom_template() -> Result<()> {
        let cfg = Config::default();
        let feed_url = cfg.site.url.join("atom.xml")?;
        let pages = make_pages()?;
        let dt = Utc.with_ymd_and_hms(2025, 1, 1, 0, 1, 1);

        let env = create_environment(&cfg)?;
        let rendered = env.get_template("atom.xml")?.render(context! {
            last_updated => dt.unwrap(),
            feed_url => feed_url,
            pages => pages
        })?;

        insta::assert_yaml_snapshot!(rendered);

        Ok(())
    }

    #[test]
    fn test_render_default_sitemap_template() -> Result<()> {
        let cfg = Config::default();
        let pages = make_pages()?;

        let env = create_environment(&cfg)?;
        let rendered = env.get_template("sitemap.xml")?.render(context! {
            pages => pages
        })?;

        insta::assert_yaml_snapshot!(rendered);

        Ok(())
    }
}

/// Get all the pages that rely on the given template.
pub fn get_page_for_template(conn: &Connection, template: &str) -> Result<Vec<Page>> {
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
    WHERE template = ?1
    ",
    )?;

    todo!()
}
