use clap::Parser;
use serde;

/// Rust version of the multi-split/delay audio tool
#[derive(Parser, Debug)]
#[command(author, version, about)]
pub struct Args {
    /// Input media file (video or audio, any FFmpeg-supported format)
    #[arg(short = 'i', long)]
    pub input: Option<String>,

    /// Output media file (any FFmpeg-supported format)
    #[arg(short = 'o', long)]
    pub output: Option<String>,

    /// Audio stream index (e.g. 6)
    #[arg(short = 's', long)]
    pub stream: Option<usize>,

    /// Path to a JSON file describing the full task (input, output, stream, splits, delays, etc). CLI arguments override values in the task file.
    #[arg(short = 't', long = "task")]
    pub task: Option<Option<String>>,

    /// Delay for the first audio segment in milliseconds (can be fractional, e.g., 200.5). (conflicts with --split-map)
    #[arg(short = 'd', long, default_value_t = 0.0, conflicts_with = "split_map")]
    pub initial_delay: f64,

    /// Split points and subsequent delays, in format <seconds>:<delay_ms>. (conflicts with --split-map)
    #[arg(short = 'p', long = "split", value_parser = parse_split, num_args = 1.., conflicts_with = "split_map")]
    pub splits: Vec<SplitPoint>,

    /// Split ranges and subsequent delays, in format <start_time>:<end_time>:<delay_ms>. (conflicts with --split-map)
    #[arg(short = 'r', long = "split-range", value_parser = parse_split_range, num_args = 1.., conflicts_with = "split_map")]
    pub split_ranges: Vec<SplitRange>,

    /// Output bitrate (e.g. 80k). If not provided, it will be detected automatically.
    #[arg(short = 'b', long)]
    pub bitrate: Option<String>,

    /// Loudness threshold (in LUFS) to consider a point as audible.
    /// Used to distinguish quiet audio from pure digital silence.
    /// For 16-bit audio, the theoretical dynamic range is 96dB, so -95 is a good default.
    #[arg(short = 'T', long, default_value_t = -95.0)]
    pub silence_threshold: f64,

    /// Show ffmpeg logs.
    #[arg(short = 'g', long)]
    pub debug: bool,

    /// Ignore ffmpeg version check.
    #[arg(long)]
    pub ignore_ffmpeg_version: bool,

    /// Check FFmpeg installation and version compatibility.
    #[arg(short = 'c', long)]
    pub check_ffmpeg: bool,

    /// Inspect input file and show all audio streams in a table
    #[arg(short = 'I', long)]
    pub inspect: bool,

    /// Write the resolved task (after all split points and delays are determined) to this file as JSON. If no file is provided, the input file name (without extension) will be used with .json.
    #[arg(short = 'w', long = "write-task-file", num_args = 0..=1, value_name = "FILE")]
    pub write_task_file: Option<Option<String>>,

    /// Automatically confirm the splitting plan and proceed without prompting
    #[arg(short = 'y', long = "yes")]
    pub yes: bool,

    /// Fit the edited audio stream to the original length (trim or pad with silence at the end of the stream as needed)
    #[arg(short = 'F', long = "fit-length")]
    pub fit_length: bool,
}

#[derive(Debug, Clone, Copy, serde::Deserialize, serde::Serialize)]
pub struct SplitPoint {
    pub time: f64,
    /// Delay in milliseconds (can be fractional, e.g., 200.5)
    pub delay: f64,
}

#[derive(Debug, Clone, Copy, serde::Deserialize, serde::Serialize)]
pub struct SplitRange {
    #[serde(rename = "startTime")]
    pub start: f64,
    #[serde(rename = "endTime")]
    pub end: f64,
    /// Delay in milliseconds (can be fractional, e.g., 200.5)
    pub delay: f64,
}

fn parse_split(s: &str) -> Result<SplitPoint, String> {
    let pos = s
        .rfind(':')
        .ok_or_else(|| format!("invalid format: '{}', expected <time>:<delay>", s))?;
    let time = s[..pos]
        .parse()
        .map_err(|e| format!("invalid time in '{}': {}", s, e))?;
    let delay = s[pos + 1..]
        .parse()
        .map_err(|e| format!("invalid delay in '{}': {}", s, e))?;
    Ok(SplitPoint { time, delay })
}

fn parse_split_range(s: &str) -> Result<SplitRange, String> {
    let parts: Vec<&str> = s.split(':').collect();
    if parts.len() != 3 {
        return Err(format!(
            "invalid format: '{}', expected <start_time>:<end_time>:<delay>",
            s
        ));
    }
    let start = parts[0]
        .parse()
        .map_err(|e| format!("invalid start time in '{}': {}", s, e))?;
    let end = parts[1]
        .parse()
        .map_err(|e| format!("invalid end time in '{}': {}", s, e))?;
    let delay = parts[2]
        .parse()
        .map_err(|e| format!("invalid delay in '{}': {}", s, e))?;
    if start >= end {
        return Err(format!("start time must be less than end time in '{}'", s));
    }
    Ok(SplitRange { start, end, delay })
}
