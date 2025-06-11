use clap::Parser;

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

    /// Delay for the first audio segment in ms.
    #[arg(long, default_value_t = 0)]
    pub initial_delay: i32,

    /// Split points and subsequent delays, in format <seconds>:<delay_ms>.
    /// Can be specified multiple times. E.g. --split 177.3:360 --split 672.3:360
    #[arg(short = 'p', long = "split", value_parser = parse_split, num_args = 1..)]
    pub splits: Vec<(f64, i32)>,

    /// Split ranges and subsequent delays, in format <start_time>:<end_time>:<delay_ms>.
    /// Can be specified multiple times. E.g. --split-range 177.3:672.3:360
    #[arg(long = "split-range", value_parser = parse_split_range, num_args = 1..)]
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
}

#[derive(Debug, Clone, Copy)]
pub struct SplitRange {
    pub start: f64,
    pub end: f64,
    pub delay: i32,
}

fn parse_split(s: &str) -> Result<(f64, i32), String> {
    let pos = s
        .rfind(':')
        .ok_or_else(|| format!("invalid format: '{}', expected <time>:<delay>", s))?;
    let time = s[..pos]
        .parse()
        .map_err(|e| format!("invalid time in '{}': {}", s, e))?;
    let delay = s[pos + 1..]
        .parse()
        .map_err(|e| format!("invalid delay in '{}': {}", s, e))?;
    Ok((time, delay))
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
