mod app;
mod audio_metadata;
mod audio_processing;
mod cli;
mod ffmpeg;
mod task;
mod util;

use anyhow::Result;
use clap::Parser;

fn main() -> Result<()> {
    let args = cli::Args::parse();
    app::run(args)
}
