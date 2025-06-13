use clap::Parser;
use serde;

/// Rust version of the multi-split/delay audio tool
#[derive(Parser, Debug)]
#[command(author, version, about)]
pub struct Args {
    /// Input MKV file
    #[arg(short, long)]
    pub input: Option<String>,

    /// Output MKV file
    #[arg(short, long)]
    pub output: Option<String>,

    /// Audio stream index (e.g. 6)
    #[arg(short, long)]
    pub stream: Option<usize>,

    /// Path to a JSON file containing split points and delays (conflicts with --split, --split-range, --initial-delay)
    #[arg(
        long = "split-map",
        conflicts_with = "initial_delay",
        conflicts_with = "splits",
        conflicts_with = "split_ranges"
    )]
    pub split_map: Option<Option<String>>,

    /// Delay for the first audio segment in ms. (conflicts with --split-map)
    #[arg(long, default_value_t = 0, conflicts_with = "split_map")]
    pub initial_delay: i32,

    /// Split points and subsequent delays, in format <seconds>:<delay_ms>. (conflicts with --split-map)
    #[arg(short = 'p', long = "split", value_parser = parse_split, num_args = 1.., conflicts_with = "split_map")]
    pub splits: Vec<SplitPoint>,

    /// Split ranges and subsequent delays, in format <start_time>:<end_time>:<delay_ms>. (conflicts with --split-map)
    #[arg(long = "split-range", value_parser = parse_split_range, num_args = 1.., conflicts_with = "split_map")]
    pub split_ranges: Vec<SplitRange>,

    /// Output bitrate (e.g. 80k). If not provided, it will be detected automatically.
    #[arg(short, long)]
    pub bitrate: Option<String>,

    /// Loudness threshold (in LUFS) to consider a point as audible.
    /// Used to distinguish quiet audio from pure digital silence.
    /// For 16-bit audio, the theoretical dynamic range is 96dB, so -95 is a good default.
    #[arg(long, default_value_t = -95.0)]
    pub silence_threshold: f64,

    /// Show ffmpeg logs.
    #[arg(long)]
    pub debug: bool,

    /// Ignore ffmpeg version check.
    #[arg(long)]
    pub ignore_ffmpeg_version: bool,

    /// Check FFmpeg installation and version compatibility.
    #[arg(long)]
    pub check_ffmpeg: bool,

    /// Inspect input file and show all audio streams in a table
    #[arg(long)]
    pub inspect: bool,

    /// Write the resolved split map (after all split points and delays are determined) to this file as JSON. If no file is provided, the input file name (without extension) will be used with .json.
    #[arg(long = "write-split-map", num_args = 0..=1, value_name = "FILE")]
    pub write_split_map: Option<Option<String>>,
}

#[derive(Debug, Clone, Copy, serde::Deserialize, serde::Serialize)]
pub struct SplitPoint {
    pub time: f64,
    pub delay: i32,
}

#[derive(Debug, Clone, Copy, serde::Deserialize, serde::Serialize)]
pub struct SplitRange {
    #[serde(rename = "startTime")]
    pub start: f64,
    #[serde(rename = "endTime")]
    pub end: f64,
    pub delay: i32,
}

#[derive(Debug, serde::Deserialize, serde::Serialize, Default)]
pub struct SplitMap {
    #[serde(default)]
    pub initial_delay: Option<i32>,
    #[serde(default)]
    pub splits: Vec<SplitPoint>,
    #[serde(default)]
    pub split_ranges: Vec<SplitRange>,
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

impl Args {
    pub fn load_split_map(&self) -> anyhow::Result<Option<SplitMap>> {
        match &self.split_map {
            Some(Some(path)) => {
                let contents = std::fs::read_to_string(path)?;
                let split_map: SplitMap = serde_json::from_str(&contents)?;
                Ok(Some(split_map))
            }
            Some(None) | None => Ok(None),
        }
    }
}
