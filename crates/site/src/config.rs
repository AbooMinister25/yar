use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use url::Url;

/// Configuration values for a site.
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Config {
    /// Site specific configuration.
    pub site: SiteConfig,
    /// Configuration for hooks (commands that are run accompanying some event).
    pub hooks: HooksConfig,
}

/// Site specific configuration.
///
/// All of this information is available to templates under the `site` variable.
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct SiteConfig {
    /// The url to the site.
    pub url: Url,
    /// The authors of the site.
    pub authors: Option<Vec<String>>,
    /// The title of the site.
    pub title: Option<String>,
    /// The description of the site.
    pub description: Option<String>,
    /// An email to accompany the site with.
    pub email: Option<String>,
    /// The path to the root of the site.
    ///
    /// This is where the static site generator will read in and process files from.
    pub root: PathBuf,
    /// The path the static site generator will render the site to.
    pub output_path: PathBuf,
    /// Whether or not a development build is being run.
    pub development: bool,
    /// The syntax highlighting theme.
    pub syntax_theme: String,
    /// A path for discovering syntax highlighting themes.
    pub syntax_theme_path: Option<PathBuf>,
}

/// Configuration for hooks.
///
/// Hooks are commands that are run on files that match a glob patterns. They accompany
/// some event, e.g after processing.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct HooksConfig {
    /// Hooks that are run once the static site generator has finished processing.
    ///
    /// Can be used for various kinds of postprocessing.
    pub post: Vec<Post>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Post {
    /// The command to run.
    pub cmd: String,
    /// A glob pattern specifying what files the command is run on.
    pub pattern: String,
    /// An optional help message.
    pub help: Option<String>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            site: SiteConfig::default(),
            hooks: HooksConfig::default(),
        }
    }
}

impl Default for SiteConfig {
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
            syntax_theme: String::from("base16-ocean.dark"),
            syntax_theme_path: None,
        }
    }
}

impl Default for HooksConfig {
    fn default() -> Self {
        Self { post: Vec::new() }
    }
}
