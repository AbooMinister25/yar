use std::time::Instant;

use clap::{Parser, Subcommand};
use color_eyre::Result;
use site::{config::Config, sql::setup_sql, Site};

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
    },
}

fn main() -> Result<()> {
    color_eyre::install()?;

    let arguments = Args::parse();

    if let Some(Commands::Build { clean }) = arguments.command {
        let conn = setup_sql()?;
        let now = Instant::now();
        let site = Site::new(conn, Config::default())?;
        site.render()?;
        site.commit_to_db()?;
        let elapsed = now.elapsed();

        println!("Built site in {elapsed:.2?} seconds");
    }

    Ok(())
}
