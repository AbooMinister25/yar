pub mod template_page;

mod functions;

use std::{path::PathBuf, sync::Arc};

use blake3::Hash;
use color_eyre::Result;
use minijinja::{Environment, Value, context, path_loader, value::Object};
use serde::Serialize;

use crate::{config::Config, page::Page, templates::functions::pages_in_section};

const DEFAULT_404: &str = r#"<!DOCTYPE html>
<h1> Page Not Found</h1>
<a href="{{ site.url | safe }}">Home</a>
"#;

const DEFAULT_ATOM_FEED: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<feed xmlns="http://www.w3.org/2005/Atom">
    <title>{{ site.title | default("Unknown") }}</title>
    <updated>{{ last_updated | datetimeformat(format="iso") }}</updated>
    <id>{{ feed_url | safe }}</id>
    <link href="{{ feed_url | safe }}" rel="self" />
    <link href="{{ site.url | safe }}"/>
    {% for page in pages %}
    {% if page.path is not endingwith "index.md" %}
    <entry>
        <title>{{ page.document.frontmatter.title }}</title>
        <published>{{ page.document.date | datetimeformat(format="iso") }}</published>
        <updated>{{ page.document.updated | datetimeformat(format="iso") }}</updated>
        <id>{{ page.permalink | safe }}</id>
        <link rel="alternate" href="{{page.permalink}}" />
        {% if site.authors %}
            {% for author in site.authors %}
            <author>
                <name>{{ author }}</name>
            </author>
            {% endfor %}
        {% else %}
            <author>
                <name>Unknown</name>
            </author>
        {% endif %}
        <summary type="html">{{ page.document.summary | safe }}</summary>
        <content type="html">
            {{ page.document.content | safe }}
        </content>
    </entry>
    {% endif %}
    {% endfor %}
</feed>
"#;

const DEFAULT_SITEMAP: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">
    {%- for page in pages %}
    <url>
        <loc>{{ page.permalink | safe }}</loc>
        <lastmod>{{ page.document.updated }}</lastmod>
    </url>
    {%- endfor %}
</urlset>
"#;

/// A template, used for caching.
#[derive(Debug, Serialize)]
pub struct Template {
    pub path: PathBuf,
    pub source_hash: Hash,
}

impl Template {
    pub const fn new(path: PathBuf, source_hash: Hash) -> Self {
        Self { path, source_hash }
    }
}

/// The context that is passed to pages when they are rendered.
#[derive(Debug)]
pub struct PageContext {
    pub pages: Vec<Page>,
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
                    blake3::hash(b"hashplaceholder"),
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
