use std::{
    fs,
    path::{Path, PathBuf},
};

use color_eyre::{Result, eyre::ContextCompat};
use url::Url;

use crate::utils::{build_permalink, fs::ensure_directory};

/// Represents a static asset. These are copied over to the resulting
/// site as-is.
#[derive(Debug)]
pub struct StaticFile {
    pub path: PathBuf,
    pub source_hash: String,
    pub out_path: PathBuf,
    pub permalink: Url,
    pub content: Vec<u8>,
}

impl StaticFile {
    pub fn new<P: AsRef<Path>, T: AsRef<Path>, Z: AsRef<Path>>(
        path: P,
        source_hash: String,
        out_dir: T,
        root: Z,
        url: &Url,
    ) -> Result<Self> {
        let out_path = out_path(&path, &out_dir, root);
        let permalink = build_permalink(&out_path, out_dir, url)?;
        let content = fs::read(&path)?;

        Ok(Self {
            path: path.as_ref().to_owned(),
            source_hash,
            out_path,
            permalink,
            content,
        })
    }

    pub fn render(&self) -> Result<()> {
        ensure_directory(
            self.out_path
                .parent()
                .context("Path should have a parent")?,
        )?;
        fs::copy(&self.path, &self.out_path)?;
        Ok(())
    }
}

fn out_path<P: AsRef<Path>, T: AsRef<Path>, Z: AsRef<Path>>(
    path: P,
    out_dir: T,
    root: Z,
) -> PathBuf {
    let out_dir = out_dir.as_ref();
    let mut components = path
        .as_ref()
        .components()
        .filter(|c| !c.as_os_str().to_str().is_some_and(|s| s.starts_with('_')));

    if root.as_ref() != Path::new(".") {
        components.next();
    }

    out_dir.components().chain(components).collect::<PathBuf>()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_out_path() {
        let path = out_path("site/static/image.png", "public", "site");
        insta::assert_yaml_snapshot!(path);

        let path = out_path("site/_static/image.png", "public", "site");
        insta::assert_yaml_snapshot!(path);

        let path = out_path("static/image.png", "public", ".");
        insta::assert_yaml_snapshot!(path);

        let path = out_path("image.png", "public", ".");
        insta::assert_yaml_snapshot!(path);
    }
}
