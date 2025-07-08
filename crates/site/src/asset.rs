use std::{
    ffi::OsStr,
    fs,
    path::{Path, PathBuf},
};

use color_eyre::{Result, eyre::ContextCompat};
use url::Url;

use crate::utils::{build_permalink, fs::ensure_directory};

/// Represents a resource that is passed through an asset pipeline.
/// This can include things like images, stylesheets, and javascript.
#[derive(Debug)]
pub struct Asset {
    pub path: PathBuf,
    pub source_hash: String,
    pub out_path: PathBuf,
    pub permalink: Url,
    pub content: String,
}

impl Asset {
    pub fn new<P: AsRef<Path>, T: AsRef<Path>, Z: AsRef<Path>>(
        path: P,
        source_hash: String,
        out_dir: T,
        root: Z,
        url: &Url,
    ) -> Result<Self> {
        let out_path = out_path(&path, &out_dir, root);
        let (content, out_path) = process_asset(&path, out_path)?;
        let permalink = build_permalink(&out_path, out_dir, url)?;

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
        fs::write(&self.out_path, &self.content)?;
        Ok(())
    }
}

fn process_asset<P: AsRef<Path>, T: AsRef<Path>>(path: P, out_dir: T) -> Result<(String, PathBuf)> {
    let mut op = out_dir.as_ref().to_owned();
    let options = grass::Options::default().style(grass::OutputStyle::Compressed);

    Ok((
        match path.as_ref().extension().and_then(OsStr::to_str) {
            Some("scss") => {
                op.set_extension("css");
                grass::from_path(path, &options)?
            }
            Some(ext) => {
                op.set_extension(ext);
                fs::read_to_string(path)?
            }
            None => fs::read_to_string(path)?,
        },
        op,
    ))
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
        .filter(|c| !c.as_os_str().to_str().is_some_and(|s| s.starts_with("_")));

    if root.as_ref() != Path::new(".") {
        components.next();
    }

    out_dir.components().chain(components).collect::<PathBuf>()
}
