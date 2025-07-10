use std::{fs, path::Path, time::Instant};

use clap::{Parser, Subcommand};
use color_eyre::Result;
use figment::{
    Figment,
    providers::{Format, Serialized, Toml},
};
use site::{Site, config::Config, sql::setup_sql};
use tempfile::Builder;

#[derive(Parser)]
#[command(version, about, long_about = None)]
struct Args {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// Build the site.
    Build {
        /// Run a clean build. Deletes database.
        #[arg(long)]
        clean: bool,
        /// Run a development build. In development builds, drafts are rendered.
        #[arg(long)]
        dev: bool,
    },
}

fn main() -> Result<()> {
    color_eyre::install()?;

    let arguments = Args::parse();

    if let Some(Commands::Build { clean, dev }) = arguments.command {
        let tmp_dir = Builder::new()
            .prefix("temp")
            .rand_bytes(0)
            .tempdir_in(".")?;

        let mut config: Config = Figment::from(Serialized::defaults(Config::default()))
            .merge(Toml::file("Config.toml"))
            .join(("development", dev))
            .extract()?;

        // Build site in a temporary directory and copy it over once everything is built
        let original_output_path = config.output_path;
        config.output_path = tmp_dir.path().join("public");

        // Clean build
        if clean {
            println!("Clean build, removing existing databases and output file");
            ensure_removed("site.db")?;
            ensure_removed(&original_output_path)?;
        }

        let conn = setup_sql()?;
        let now = Instant::now();

        let mut site = Site::new(conn, config)?;
        site.load()?;
        site.render()?;
        site.commit_to_db()?;

        let elapsed = now.elapsed();
        println!("Built site in {elapsed:.2?}");
        copy_dir_all(tmp_dir.path().join("public"), original_output_path)?;
    }

    Ok(())
}

fn copy_dir_all<T: AsRef<Path>, Z: AsRef<Path>>(src: T, out: Z) -> Result<()> {
    fs::create_dir_all(&out)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        if entry.file_type()?.is_file() {
            fs::copy(entry.path(), out.as_ref().join(entry.file_name()))?;
        } else {
            copy_dir_all(entry.path(), out.as_ref().join(entry.file_name()))?;
        }
    }
    Ok(())
}

// If the given file exists, delete it.
pub fn ensure_removed<T: AsRef<Path>>(path: T) -> Result<()> {
    let path = path.as_ref();

    if path.exists() {
        if path.is_dir() {
            fs::remove_dir_all(path)?;
        } else {
            fs::remove_file(path)?;
        }
    }

    Ok(())
}
