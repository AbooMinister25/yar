use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// Configuration values for a site.
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Config {
    pub url: String,
    pub author: String,
    pub title: String,
    pub description: String,
    pub root: PathBuf,
    pub output_path: PathBuf,
    pub development: bool,
    pub theme: String,
    pub theme_path: Option<PathBuf>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            url: String::from("http://0.0.0.0:8000/"),
            author: String::from(""),
            title: String::from(""),
            description: String::from(""),
            root: Path::new("site/").to_owned(),
            output_path: Path::new("public/").to_owned(),
            development: false,
            theme: String::from("base16-ocean.dark"),
            theme_path: None,
        }
    }
}
