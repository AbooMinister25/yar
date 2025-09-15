#![allow(clippy::missing_errors_doc)]
#![allow(clippy::missing_panics_doc)]

mod new;
mod server;

use std::{
    fs,
    path::Path,
    time::{Duration, Instant},
};

use clap::{Parser, Subcommand};
use color_eyre::Result;
use figment::{
    Figment,
    providers::{Format, Serialized, Toml},
};
use notify_debouncer_mini::{DebounceEventResult, DebouncedEvent, new_debouncer, notify::Error};
use tempfile::Builder;
use tokio::signal::ctrl_c;
use tower_livereload::{LiveReloadLayer, Reloader};
use yar_site::{Site, config::Config, sql::setup_sql};

use crate::{new::create_site_template, server::run_server};

#[derive(Parser)]
#[command(version, about, long_about = None, arg_required_else_help = true)]
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
    /// Create a new site.
    New { path: String },
    /// Build the site and serve it on a development web server.
    /// Hot reloading on file changes.
    Serve {
        #[arg(long)]
        clean: bool,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    color_eyre::install()?;
    ensure_removed("temp/")?;

    let arguments = Args::parse();
    let mut config: Config = Figment::from(Serialized::defaults(Config::default()))
        .merge(Toml::file("Config.toml"))
        .extract()?;

    match arguments.command {
        Some(Commands::Build { clean, dev }) => {
            config.site.development = dev;
            let tmp_dir = Builder::new()
                .prefix("temp")
                .rand_bytes(0)
                .tempdir_in(".")?;

            // Build site in a temporary directory and copy it over once everything is built
            let original_output_path = config.site.output_path;
            config.site.output_path = tmp_dir.path().join("public");

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
            site.run_post_hooks()?;

            let elapsed = now.elapsed();
            println!("Built site in {elapsed:.2?}");
            copy_dir_all(tmp_dir.path().join("public"), original_output_path)?;
        }
        Some(Commands::New { path }) => {
            println!("Creating new site at {path}");
            create_site_template(path)?;
            println!("Created site");
        }
        Some(Commands::Serve { clean }) => {
            config.site.development = true;
            let tmp_dir = Builder::new()
                .prefix("temp")
                .rand_bytes(0)
                .tempdir_in(".")?;
            let serve_path = tmp_dir.path().join("public"); // The path the static file server will serve files from.

            // Build site in a temporary directory
            config.site.output_path = tmp_dir.path().join("public");

            // Clean build
            if clean {
                println!("Clean build, removing existing databases and output file");
                ensure_removed("site.db")?;
            }

            let root = config.site.root.clone();
            let conn = setup_sql()?;
            let mut site = Site::new(conn, config)?;

            let now = Instant::now();
            println!("Building site.");
            site.load()?;
            site.render()?;
            site.commit_to_db()?;
            site.run_post_hooks()?;

            let elapsed = now.elapsed();
            println!("Built site in {elapsed:.2?}");

            let livereload = LiveReloadLayer::new();
            let reloader = livereload.reloader();

            let (tx, rx) = tokio::sync::mpsc::channel(32);

            let mut debouncer = new_debouncer(
                Duration::from_millis(50),
                move |res: DebounceEventResult| {
                    tx.blocking_send(res).expect("Problem with sending message");
                },
            )?;
            debouncer
                .watcher()
                .watch(&root, notify::RecursiveMode::Recursive)?;

            let server_task =
                tokio::spawn(async move { run_server(serve_path, livereload, tmp_dir).await });
            let livereload_task = tokio::spawn(run_livereload(reloader, site, rx));

            livereload_task.await??;
            server_task.await??;
        }
        _ => unreachable!(),
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
fn ensure_removed<T: AsRef<Path>>(path: T) -> Result<()> {
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

async fn run_livereload(
    reloader: Reloader,
    mut site: Site<'_>,
    mut rx: tokio::sync::mpsc::Receiver<Result<Vec<DebouncedEvent>, Error>>,
) -> Result<()> {
    loop {
        tokio::select! {
            Some(Ok(events)) = rx.recv() => {
                for _ in events {
                    let now = Instant::now();
                    println!("Filesystem changes detected...rebuilding site");
                    site.load()?;
                    site.render()?;
                    site.commit_to_db()?;
                    site.run_post_hooks()?;

                    let elapsed = now.elapsed();
                    println!("Built site in {elapsed:.2?}");

                    reloader.reload();
                }
            },
            _ = ctrl_c() => {
                break;
            }
        };
    }

    Ok(())
}
