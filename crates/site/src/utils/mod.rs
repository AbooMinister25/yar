use std::path::{Path, PathBuf};

use color_eyre::{Result, eyre::ContextCompat};
use url::Url;

pub mod fs;

/// Build permalink for a site item.
pub fn build_permalink<P: AsRef<Path>, T: AsRef<Path>>(
    path: P,
    out_dir: T,
    url: &Url,
) -> Result<Url> {
    let out = out_dir
        .as_ref()
        .file_name()
        .context("Output directory shouldn't terminate in ..")?;

    let mut components = path
        .as_ref()
        .components()
        .skip_while(|c| c.as_os_str() != out);
    components.next();
    let mut url_ending = components.collect::<PathBuf>();
    if url_ending.ends_with("index.html") {
        url_ending = url_ending
            .parent()
            .context("path doesn't have parent?")?
            .to_path_buf();
    }
    let permalink = url.join(
        url_ending
            .to_str()
            .context("Path should be valid unicode.")?,
    )?;

    Ok(permalink)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_permalink() -> Result<()> {
        let path = Path::new("site/index.html");
        let out_dir = Path::new("site/");
        let url = Url::parse("https://example.com/")?;
        insta::assert_yaml_snapshot!(build_permalink(path, out_dir, &url)?);

        let path = Path::new("site/posts/hello-world/index.html");
        let out_dir = Path::new("site/");
        let url = Url::parse("https://example.com/")?;
        insta::assert_yaml_snapshot!(build_permalink(path, out_dir, &url)?);

        let path = Path::new("site/assets/style.css");
        let out_dir = Path::new("site/");
        let url = Url::parse("https://example.com/")?;
        insta::assert_yaml_snapshot!(build_permalink(path, out_dir, &url)?);

        Ok(())
    }
}
