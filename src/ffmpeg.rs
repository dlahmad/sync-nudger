use anyhow::{Result, bail};
use regex::Regex;
use std::{
    io,
    path::Path,
    process::{Command, Stdio},
};

const EXPECTED_FFMPEG_MAJOR_VERSION: u32 = 7;
const EXPECTED_FFMPEG_MINOR_VERSION: u32 = 1;
const MINIMUM_FFMPEG_MAJOR_VERSION: u32 = 4;

#[derive(Debug)]
pub struct AudioStream {
    pub index: usize,
    pub codec: String,
    pub channels: String,
    pub sample_rate: String,
    pub bitrate: String,
    pub language: String,
    pub title: String,
}

pub fn run_ffmpeg(args: &[&str], debug: bool) -> Result<()> {
    let mut command = Command::new("ffmpeg");
    command.args(args);

    if !debug {
        command.stdout(Stdio::null()).stderr(Stdio::null());
    }

    let status = command.status()?;
    if !status.success() {
        bail!("FFmpeg failed: {:?}", args);
    }
    Ok(())
}

pub fn check_ffmpeg_version(ignore_check: bool) -> Result<()> {
    if ignore_check {
        return Ok(());
    }

    let output = Command::new("ffmpeg").arg("-version").output()?;
    if !output.status.success() {
        bail!("Could not run `ffmpeg -version` to check version.");
    }

    let version_info = String::from_utf8_lossy(&output.stdout);
    let re = Regex::new(r"ffmpeg version (\d+)\.(\d+)")?;

    if let Some(caps) = re.captures(&version_info) {
        let major: u32 = caps.get(1).unwrap().as_str().parse()?;
        let minor: u32 = caps.get(2).unwrap().as_str().parse()?;

        if major == EXPECTED_FFMPEG_MAJOR_VERSION && minor == EXPECTED_FFMPEG_MINOR_VERSION {
            Ok(())
        } else {
            bail!(
                "ffmpeg version mismatch. Expected v{}.{}, but found v{}.{}. Use --ignore-ffmpeg-version to bypass.",
                EXPECTED_FFMPEG_MAJOR_VERSION,
                EXPECTED_FFMPEG_MINOR_VERSION,
                major,
                minor
            )
        }
    } else {
        bail!("Could not parse ffmpeg version from output. Use --ignore-ffmpeg-version to bypass.");
    }
}

pub fn check_dependency(cmd: &str) -> Result<()> {
    match Command::new(cmd)
        .arg("-version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
    {
        Ok(_) => Ok(()),
        Err(e) => {
            if e.kind() == io::ErrorKind::NotFound {
                bail!(
                    "`{}` command not found. Please ensure it is installed and in your PATH.",
                    cmd
                );
            }
            Err(anyhow::anyhow!("Failed to run `{}`: {}", cmd, e))
        }
    }
}

pub fn find_quietest_point(
    audio_path: &Path,
    start: f64,
    end: f64,
    silence_threshold: f64,
    debug: bool,
) -> Result<f64> {
    println!(
        "ℹ️ Finding quietest point in range {:.3}s - {:.3}s",
        start, end
    );
    let duration = end - start;
    let output = Command::new("ffmpeg")
        .args(&[
            "-i",
            audio_path.to_str().unwrap(),
            "-ss",
            &start.to_string(),
            "-t",
            &duration.to_string(),
            "-af",
            "ebur128=peak=true",
            "-f",
            "null",
            "-",
        ])
        .output()?;

    let stderr = String::from_utf8_lossy(&output.stderr);
    if debug {
        eprintln!(
            "\n--- FFMPEG STDERR for quietest point ---\n{}\n--- END FFMPEG STDERR ---",
            stderr
        );
    }
    let re =
        Regex::new(r"\[Parsed_ebur128_0 @ [^\]]+\] t:\s*([\d.]+)\s*TARGET:.*M:\s*([-\d.]+)\s*S:")
            .unwrap();

    let mut loudness_points: Vec<(f64, f64)> = Vec::new();
    for cap in re.captures_iter(&stderr) {
        if let (Some(time_str), Some(loudness_str)) = (cap.get(1), cap.get(2)) {
            if let (Ok(time), Ok(loudness)) = (
                time_str.as_str().parse::<f64>(),
                loudness_str.as_str().parse::<f64>(),
            ) {
                // The ebur128 `t:` timestamp is relative to the start of the segment.
                // We only care about points above the silence threshold.
                if time >= start && time <= end && loudness > silence_threshold {
                    loudness_points.push((time, loudness));
                }
            }
        }
    }

    if loudness_points.is_empty() {
        bail!(
            "Could not find any audible point in range {:.3}s - {:.3}s above the threshold of {:.2} LUFS. Try adjusting --silence-threshold.",
            start,
            end,
            silence_threshold
        );
    }

    // From the candidates, find the one with the lowest loudness.
    let (quietest_time, min_loudness) = loudness_points
        .iter()
        .min_by(|a, b| a.1.partial_cmp(&b.1).unwrap())
        .map(|(t, l)| (*t, *l))
        .unwrap(); // Safe to unwrap because loudness_points is not empty

    println!(
        "  ✅ Found quietest point at {:.3}s (Loudness: {:.2} LUFS)",
        quietest_time, min_loudness
    );
    Ok(quietest_time)
}

pub fn check_and_display_ffmpeg() -> Result<()> {
    println!("🔍 Checking FFmpeg installation...\n");

    // Check if ffmpeg is available
    match Command::new("ffmpeg").arg("-version").output() {
        Ok(output) => {
            if !output.status.success() {
                bail!("❌ FFmpeg command failed to execute properly.");
            }

            let version_info = String::from_utf8_lossy(&output.stdout);
            let re = Regex::new(r"ffmpeg version (\d+)\.(\d+)(?:\.(\d+))?")?;

            if let Some(caps) = re.captures(&version_info) {
                let major: u32 = caps.get(1).unwrap().as_str().parse()?;
                let minor: u32 = caps.get(2).unwrap().as_str().parse()?;
                let patch: u32 = caps.get(3).map_or(0, |m| m.as_str().parse().unwrap_or(0));

                println!("✅ FFmpeg found:");
                println!("   Version: {}.{}.{}", major, minor, patch);

                if major >= MINIMUM_FFMPEG_MAJOR_VERSION {
                    println!(
                        "   Status: ✅ Compatible (minimum required: {}.0.0)",
                        MINIMUM_FFMPEG_MAJOR_VERSION
                    );
                } else {
                    println!(
                        "   Status: ❌ Too old (minimum required: {}.0.0)",
                        MINIMUM_FFMPEG_MAJOR_VERSION
                    );
                }

                if major == EXPECTED_FFMPEG_MAJOR_VERSION && minor == EXPECTED_FFMPEG_MINOR_VERSION
                {
                    println!("   Note: This is the tested version");
                } else {
                    println!(
                        "   Note: Tested with version {}.{}.x",
                        EXPECTED_FFMPEG_MAJOR_VERSION, EXPECTED_FFMPEG_MINOR_VERSION
                    );
                }
            } else {
                println!("⚠️  Could not parse FFmpeg version from output");
                println!(
                    "   Raw output: {}",
                    version_info.lines().next().unwrap_or("")
                );
            }
        }
        Err(e) => {
            if e.kind() == io::ErrorKind::NotFound {
                println!("❌ FFmpeg not found in PATH");
                println!(
                    "   Please install FFmpeg and ensure it's accessible from the command line"
                );
                bail!("FFmpeg is required but not installed");
            } else {
                bail!("Failed to check FFmpeg: {}", e);
            }
        }
    }

    println!();

    // Check if ffprobe is available
    match Command::new("ffprobe").arg("-version").output() {
        Ok(output) => {
            if output.status.success() {
                println!("✅ FFprobe found and working");
            } else {
                println!("❌ FFprobe command failed");
            }
        }
        Err(e) => {
            if e.kind() == io::ErrorKind::NotFound {
                println!("❌ FFprobe not found in PATH");
                bail!("FFprobe is required but not installed");
            } else {
                println!("⚠️  Failed to check FFprobe: {}", e);
            }
        }
    }

    println!();

    // Check for required filter
    match Command::new("ffmpeg")
        .args(&["-hide_banner", "-filters"])
        .output()
    {
        Ok(output) => {
            let filters = String::from_utf8_lossy(&output.stdout);
            if filters.contains("ebur128") {
                println!("✅ Required filter 'ebur128' is available");
            } else {
                println!("❌ Required filter 'ebur128' not found");
                println!("   This filter is needed for loudness analysis");
            }
        }
        Err(_) => {
            println!("⚠️  Could not check available filters");
        }
    }

    println!("\n🎉 FFmpeg check complete!");
    Ok(())
}

pub fn inspect_audio_streams(input_file: &str) -> Result<Vec<AudioStream>> {
    let output = Command::new("ffprobe")
        .args([
            "-v",
            "quiet",
            "-print_format",
            "json",
            "-show_streams",
            "-show_format",
            "-select_streams",
            "a",
            input_file,
        ])
        .output()?;

    if !output.status.success() {
        bail!(
            "Failed to probe audio streams: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    let json_output = String::from_utf8_lossy(&output.stdout);
    let parsed: serde_json::Value = serde_json::from_str(&json_output)?;

    let mut streams = Vec::new();

    // Get file duration for bitrate calculation
    let file_duration = parsed["format"]["duration"]
        .as_str()
        .and_then(|d| d.parse::<f64>().ok());

    if let Some(stream_array) = parsed["streams"].as_array() {
        for stream in stream_array {
            let index = stream["index"].as_u64().unwrap_or(0) as usize;
            let codec = stream["codec_name"]
                .as_str()
                .unwrap_or("unknown")
                .to_string();

            let channels = if let Some(ch) = stream["channels"].as_u64() {
                ch.to_string()
            } else if let Some(layout) = stream["channel_layout"].as_str() {
                layout.to_string()
            } else {
                "unknown".to_string()
            };

            let sample_rate = if let Some(sr) = stream["sample_rate"].as_str() {
                format!("{} Hz", sr)
            } else {
                "unknown".to_string()
            };

            let bitrate = get_stream_bitrate(&stream, file_duration);

            let language = if let Some(tags) = stream["tags"].as_object() {
                tags.get("language")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown")
                    .to_string()
            } else {
                "unknown".to_string()
            };

            let title = if let Some(tags) = stream["tags"].as_object() {
                tags.get("title")
                    .and_then(|v| v.as_str())
                    .unwrap_or("-")
                    .to_string()
            } else {
                "-".to_string()
            };

            streams.push(AudioStream {
                index,
                codec,
                channels,
                sample_rate,
                bitrate,
                language,
                title,
            });
        }
    }

    Ok(streams)
}

fn get_stream_bitrate(stream: &serde_json::Value, file_duration: Option<f64>) -> String {
    // Try direct bit_rate field first
    if let Some(br) = stream["bit_rate"].as_str() {
        if let Ok(br_num) = br.parse::<u64>() {
            if br_num > 0 {
                return format!("{} kbps", br_num / 1000);
            }
        }
    }

    // Try tags for bitrate information
    if let Some(tags) = stream["tags"].as_object() {
        // Check various bitrate tag formats
        let bitrate_tags = ["BPS", "BPS-eng", "ENCODER_OPTIONS"];
        for tag in &bitrate_tags {
            if let Some(br_tag) = tags.get(*tag).and_then(|v| v.as_str()) {
                if let Ok(br_num) = br_tag.parse::<u64>() {
                    if br_num > 0 {
                        return format!("{} kbps", br_num / 1000);
                    }
                }
            }
        }
    }

    // Try to estimate from stream size and duration
    if let (Some(duration), Some(size)) =
        (file_duration, stream["tags"]["NUMBER_OF_BYTES"].as_str())
    {
        if let Ok(size_bytes) = size.parse::<u64>() {
            if duration > 0.0 && size_bytes > 0 {
                let bitrate_bps = (size_bytes * 8) as f64 / duration;
                return format!("~{} kbps", (bitrate_bps / 1000.0) as u64);
            }
        }
    }

    // For common codecs, provide typical ranges when unknown
    if let Some(codec) = stream["codec_name"].as_str() {
        match codec {
            "aac" => "~128 kbps".to_string(),
            "mp3" => "~192 kbps".to_string(),
            "flac" => "~1000 kbps".to_string(),
            "ac3" => "~640 kbps".to_string(),
            "dts" => "~1536 kbps".to_string(),
            "eac3" => "~768 kbps".to_string(),
            _ => "unknown".to_string(),
        }
    } else {
        "unknown".to_string()
    }
}

pub fn get_stream_bitrate_for_processing(input_file: &str, stream_index: usize) -> Result<String> {
    let streams = inspect_audio_streams(input_file)?;

    for stream in streams {
        if stream.index == stream_index {
            let bitrate = stream.bitrate;

            // Convert from display format to FFmpeg format
            if bitrate.ends_with(" kbps") {
                // Remove " kbps" and add "k"
                let number_part = &bitrate[..bitrate.len() - 5];
                return Ok(format!("{}k", number_part));
            } else if bitrate.starts_with('~') && bitrate.ends_with(" kbps") {
                // Handle estimated bitrates like "~128 kbps"
                let number_part = &bitrate[1..bitrate.len() - 5];
                return Ok(format!("{}k", number_part));
            } else if bitrate != "unknown" {
                // If it's already in the right format, return as-is
                return Ok(bitrate);
            }
            break;
        }
    }

    bail!(
        "Could not determine bitrate for stream {}. Please provide it with --bitrate <bitrate> (e.g. 128k)",
        stream_index
    );
}
