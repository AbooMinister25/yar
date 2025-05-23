use clap::{Parser, Subcommand};
use color_eyre::Result;

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

    Ok(())
}
