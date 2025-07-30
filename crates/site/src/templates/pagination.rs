use std::{
    fs,
    hash::Hash,
    path::{Path, PathBuf},
    sync::Arc,
};

use color_eyre::{
    Result,
    eyre::{ContextCompat, OptionExt},
};
use minijinja::{Environment, Value, context};
use serde::{Deserialize, Serialize};
use url::Url;

use crate::{
    page::Page,
    templates::PageContext,
    utils::{build_permalink, fs::ensure_directory},
};

#[derive(Debug, PartialEq, Eq)]
pub struct Paginated {
    pub path: PathBuf,
    pub source_hash: String,
    pub out_path: PathBuf,
    pub permalink: Url,
    pub content: String,
    pub frontmatter: MetaFrontmatter,
}

/// The frontmatter parsed from every meta template.
#[derive(Debug, Deserialize, PartialEq, Eq)]
pub struct MetaFrontmatter {
    pub from: String,
    pub every: usize,
    pub name_template: Option<String>,
    #[serde(default)]
    pub additional_dependencies: Vec<String>,
}

/// The pagination context passed to every meta template.
#[derive(Debug, Serialize, Deserialize)]
pub struct PaginationContext {
    items: Vec<String>,
    next: Option<String>,
    previous: Option<String>,
}

impl Paginated {
    /// Create a new `Paginated`.
    pub fn new<P: AsRef<Path>, T: AsRef<Path>, Z: AsRef<Path>>(
        content: &str,
        source_hash: String,
        path: P,
        out_dir: T,
        root: Z,
        url: &Url,
    ) -> Result<Self> {
        let (frontmatter, remaining) = parse_frontmatter(content)?;

        let out_path = out_path(&path, &out_dir, root);
        let permalink = build_permalink(&out_path, out_dir, url)?;

        Ok(Self {
            path: path.as_ref().to_owned(),
            source_hash,
            out_path,
            permalink,
            content: remaining,
            frontmatter,
        })
    }

    /// Render this pagination.
    ///
    /// TODO: currently, only collections of strings can be paginated over. In the future,
    /// TODO: maybe something like `minijinja`s `DynObject` could be used to ease this restriction.
    pub fn render(&self, index: &[Arc<Page>], env: &Environment) -> Result<()> {
        // Get global value that this template paginates on.
        let value = env
            .globals()
            .find(|g| self.frontmatter.from == g.0)
            .ok_or_eyre(format!("Global {} doesn't exist", self.frontmatter.from))?
            .1;
        // Value::downcast_object_ref doesn't seem to work here, and I can't chunk an iterator.
        let items = value
            .try_iter()?
            .map(|v| v.to_string())
            .collect::<Vec<String>>();

        let template = env.template_from_str(&self.content)?;
        let name_expr = self
            .frontmatter
            .name_template
            .as_ref()
            .map(|s| env.compile_expression(s))
            .transpose()?;

        for (idx, chunk) in items.chunks(self.frontmatter.every).enumerate() {
            let pag = PaginationContext {
                items: chunk.into(),
                next: None,
                previous: None,
            };
            let ctx = Value::from_object(PageContext {
                pages: index.to_vec(),
            });

            let rendered = template.render(context! {
                pagination => pag, ..ctx
            })?;

            let name = name_expr
                .as_ref()
                .map(|e| e.eval(context! { pagination => pag }))
                .transpose()?
                .map_or(idx.to_string(), |v| v.to_string());

            let out = self.out_path.join(name).join("index.html");
            ensure_directory(out.parent().context("Path should have a parent")?)?;

            fs::write(out, rendered)?;
        }

        Ok(())
    }
}

impl Hash for Paginated {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.path.hash(state);
    }
}

fn parse_frontmatter(content: &str) -> Result<(MetaFrontmatter, String)> {
    let mut in_frontmatter = false;
    let mut frontmatter_content = String::new();
    let mut remaining = String::new();

    for line in content.lines() {
        if line.trim() == "---" {
            in_frontmatter = !in_frontmatter;
            continue;
        }

        if in_frontmatter {
            frontmatter_content.push_str(line);
            frontmatter_content.push('\n');
        } else {
            remaining.push_str(line);
            remaining.push('\n');
        }
    }

    let frontmatter = toml::from_str(&frontmatter_content)?;
    Ok((frontmatter, remaining))
}

fn out_path<P: AsRef<Path>, T: AsRef<Path>, Z: AsRef<Path>>(
    path: P,
    out_dir: T,
    root: Z,
) -> PathBuf {
    let out_dir = out_dir.as_ref();
    let path = path.as_ref().with_extension("");

    let mut components = path
        .components()
        .filter(|c| !c.as_os_str().to_str().is_some_and(|s| s.starts_with('_')));

    if root.as_ref() != Path::new(".") {
        components.next();
    }

    out_dir.components().chain(components).collect::<PathBuf>()
}
