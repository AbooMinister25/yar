use std::path::Path;

use color_eyre::{Result, eyre::ContextCompat};

pub mod fs;

/// Build permalink for a site item.
pub fn build_permalink<P: AsRef<Path>>(path: P, url: &str) -> Result<String> {
    let mut components = path.as_ref().components();
    components.next();
    Ok(format!(
        "{url}{}",
        components
            .as_path()
            .to_str()
            .context("Path should be valid unicode")?
    ))
}
