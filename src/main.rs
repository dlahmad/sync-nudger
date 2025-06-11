mod app;
mod cli;
mod ffmpeg;

use anyhow::Result;
use clap::Parser;

fn main() -> Result<()> {
    let args = cli::Args::parse();
    app::run(args)
}
