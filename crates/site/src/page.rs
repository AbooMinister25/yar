use std::fmt::Debug;
use std::fs;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use color_eyre::Result;
use color_eyre::eyre::ContextCompat;
use markdown::{Document, MarkdownRenderer};
use minify_html::{Cfg, minify};
use minijinja::{Environment, context};
use serde::{Deserialize, Serialize};
use url::Url;

use crate::utils::build_permalink;
use crate::utils::fs::ensure_directory;

/// A single page in the site.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Page {
    pub path: PathBuf,
    pub source_hash: String,
    pub out_path: PathBuf,
    pub permalink: Url,
    pub document: Document,
}

impl Page {
    #[allow(clippy::too_many_arguments)]
    pub fn new<P: AsRef<Path>, T: AsRef<Path>, Z: AsRef<Path>>(
        path: P,
        content: &str,
        source_hash: String,
        out_dir: T,
        root: Z,
        url: &Url,
        markdown_renderer: &MarkdownRenderer,
        env: &Environment,
    ) -> Result<Self> {
        let document = markdown_renderer.parse_from_string(content, env)?;
        let out_path = out_path(
            &path,
            &out_dir,
            root,
            &document.frontmatter.title,
            document.frontmatter.slug.as_deref(),
        );
        let permalink = build_permalink(&out_path, out_dir, url)?;

        Ok(Self {
            path: path.as_ref().into(),
            out_path,
            source_hash,
            permalink,
            document,
        })
    }

    pub fn render(&self, index: &[Rc<Self>], env: &Environment) -> Result<()> {
        ensure_directory(
            self.out_path
                .parent()
                .context("Path should have a parent")?,
        )?;

        let frontmatter = &self.document.frontmatter;
        let template = frontmatter.template.as_ref().map_or("post.html", |v| v);
        let template = env.get_template(template)?;

        let rendered_html =
            template.render(context! { document => self.document, pages => index})?;
        let cfg = Cfg::new();
        let minified = minify(rendered_html.as_bytes(), &cfg);

        fs::write(&self.out_path, minified)?;

        Ok(())
    }
}

fn out_path<P: AsRef<Path>, T: AsRef<Path>, Z: AsRef<Path>>(
    path: P,
    out_dir: T,
    root: Z,
    title: &str,
    slug: Option<&str>,
) -> PathBuf {
    let out_dir = out_dir.as_ref();

    let ending = if path.as_ref().ends_with("index.md") {
        PathBuf::from("index.html")
    } else {
        PathBuf::from(slug.map_or_else(|| title.replace(' ', "-"), ToOwned::to_owned))
            .join("index.html")
    };

    let mut components = path
        .as_ref()
        .parent()
        .unwrap_or_else(|| path.as_ref())
        .components()
        .filter(|c| !c.as_os_str().to_str().is_some_and(|s| s.starts_with('_')));

    if root.as_ref() != Path::new(".") {
        components.next();
    }

    out_dir
        .components()
        .chain(components)
        .collect::<PathBuf>()
        .join(ending)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_out_path() {
        let path = out_path(
            "site/_content/posts/hello-world.md",
            "public",
            "site",
            "hello world",
            None,
        );
        insta::assert_yaml_snapshot!(path);

        let path = out_path(
            "site/_content/posts/hello-world.md",
            "public",
            "site",
            "hello world",
            Some("thisisaslug"),
        );
        insta::assert_yaml_snapshot!(path);

        let path = out_path(
            "_content/posts/hello-world.md",
            "public",
            ".",
            "hello world",
            None,
        );
        insta::assert_yaml_snapshot!(path);

        let path = out_path("hello-world.md", "public", ".", "hello world", None);
        insta::assert_yaml_snapshot!(path);

        let path = out_path(
            "site/_content/series/hello-world/index.md",
            "public",
            "site",
            "this is a series",
            None,
        );
        insta::assert_yaml_snapshot!(path);

        let path = out_path(
            "site/_content/series/hello-world/part-1.md",
            "public",
            "site",
            "Part One",
            None,
        );
        insta::assert_yaml_snapshot!(path);

        let path = out_path("site/_content/index.md", "public", "site", "", None);
        insta::assert_yaml_snapshot!(path);
    }
}
