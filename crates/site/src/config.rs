use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use url::Url;

/// Configuration values for a site.
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Config {
    pub url: Url,
    pub authors: Option<Vec<String>>,
    pub title: Option<String>,
    pub description: Option<String>,
    pub email: Option<String>,
    pub root: PathBuf,
    pub output_path: PathBuf,
    pub development: bool,
    pub theme: String,
    pub theme_path: Option<PathBuf>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            url: Url::parse("http://0.0.0.0:8000/").expect("Invalid default URL?"),
            authors: None,
            title: None,
            description: None,
            email: None,
            root: Path::new("site/").to_owned(),
            output_path: Path::new("public/").to_owned(),
            development: false,
            theme: String::from("base16-ocean.dark"),
            theme_path: None,
        }
    }
}
