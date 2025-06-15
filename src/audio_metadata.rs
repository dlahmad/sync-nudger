use anyhow::{Result, bail};
use std::process::Command;

use crate::ffmpeg::FFmpegError;

/// Struct to hold audio stream metadata
pub struct AudioStreamMetadata {
    pub stream_index: usize,
    pub codec: String,
    pub title: String,
    pub language: String,
}

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

pub fn inspect_audio_streams(input_file: &str) -> Result<Vec<AudioStream>, FFmpegError> {
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
        return Err(FFmpegError::CommandFailed(
            "inspect_audio_streams".to_string(),
            String::from_utf8_lossy(&output.stderr).to_string(),
        ));
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

pub fn get_stream_bitrate_for_processing(
    input_file: &str,
    stream_index: usize,
) -> Result<String, FFmpegError> {
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

    Err(FFmpegError::BitrateUndetermined { stream_index })
}

/// Probe the input file for the audio stream index, codec, title, and language.
pub fn probe_audio_stream(input: &str, stream: usize) -> Result<AudioStreamMetadata> {
    // Get stream index and codec
    let ffprobe_streams = Command::new("ffprobe")
        .args(&[
            "-v",
            "error",
            "-show_entries",
            "stream=index,codec_type,codec_name",
            "-of",
            "csv=p=0",
            input,
        ])
        .output()?;
    let streams_info = String::from_utf8_lossy(&ffprobe_streams.stdout);
    let mut audio_count = 0;
    let mut audio_stream_idx = -1isize;
    let mut original_codec = String::new();
    for line in streams_info.lines() {
        let parts: Vec<_> = line.split(',').collect();
        if parts.len() >= 3 && parts[2] == "audio" {
            if let Ok(id) = parts[0].parse::<usize>() {
                if id == stream {
                    audio_stream_idx = audio_count;
                    original_codec = parts[1].to_string();
                    break;
                }
            }
            audio_count += 1;
        }
    }
    if audio_stream_idx == -1 {
        bail!("Could not find audio stream {} in mapping", stream);
    }
    if original_codec.is_empty() {
        bail!("Could not determine codec for audio stream {}", stream);
    }
    // Get title
    let ffprobe_title = Command::new("ffprobe")
        .args(&[
            "-v",
            "error",
            "-select_streams",
            &format!("a:{}", audio_stream_idx),
            "-show_entries",
            "stream_tags=title",
            "-of",
            "default=noprint_wrappers=1:nokey=1",
            input,
        ])
        .output()?;
    let original_title = String::from_utf8_lossy(&ffprobe_title.stdout)
        .trim()
        .to_owned();
    // Get language
    let ffprobe_lang = Command::new("ffprobe")
        .args(&[
            "-v",
            "error",
            "-select_streams",
            &format!("a:{}", audio_stream_idx),
            "-show_entries",
            "stream_tags=language",
            "-of",
            "default=noprint_wrappers=1:nokey=1",
            input,
        ])
        .output()?;
    let original_lang = String::from_utf8_lossy(&ffprobe_lang.stdout)
        .trim()
        .to_owned();
    Ok(AudioStreamMetadata {
        stream_index: audio_stream_idx as usize,
        codec: original_codec,
        title: original_title,
        language: original_lang,
    })
}

/// Get the duration of the audio stream (in seconds)
pub fn get_audio_stream_duration(input_file: &str, stream_index: usize) -> Result<Option<f64>> {
    let output = Command::new("ffprobe")
        .args(&[
            "-v",
            "error",
            "-show_entries",
            "stream=index,duration,codec_type",
            "-show_entries",
            "format=duration",
            "-of",
            "json",
            input_file,
        ])
        .output()?;
    if !output.status.success() {
        bail!(
            "ffprobe failed to get duration: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    let json: serde_json::Value = serde_json::from_slice(&output.stdout)?;
    // Try to find the stream duration
    if let Some(streams) = json["streams"].as_array() {
        for stream in streams {
            if stream["index"].as_u64() == Some(stream_index as u64) {
                if let Some(dur) = stream["duration"].as_str() {
                    if let Ok(val) = dur.parse::<f64>() {
                        return Ok(Some(val));
                    }
                }
            }
        }
    }
    // Fallback: use the container duration
    if let Some(format) = json.get("format") {
        if let Some(dur) = format.get("duration").and_then(|d| d.as_str()) {
            if let Ok(val) = dur.parse::<f64>() {
                return Ok(Some(val));
            }
        }
    }
    Ok(None)
}

/// Build FFmpeg -map arguments to replace a specific audio stream with a new one from input 1.
/// Returns a Vec<String> of -map arguments.
pub fn build_stream_map_args(input: &str, replaced_audio_stream_idx: usize) -> Result<Vec<String>> {
    // Use ffprobe to get all streams and their types
    let ffprobe_streams = std::process::Command::new("ffprobe")
        .args(&[
            "-v",
            "error",
            "-show_entries",
            "stream=index,codec_type",
            "-of",
            "csv=p=0",
            input,
        ])
        .output()?;
    let streams_info = String::from_utf8_lossy(&ffprobe_streams.stdout);
    let mut map_args = Vec::new();
    let mut audio_count = 0;
    for line in streams_info.lines() {
        let parts: Vec<_> = line.split(',').collect();
        if parts.len() == 2 {
            let idx = parts[0];
            let typ = parts[1];
            if typ == "audio" {
                if audio_count == replaced_audio_stream_idx {
                    // Insert the new audio stream from input 1 in place of this one
                    map_args.push("-map".to_string());
                    map_args.push("1:0".to_string());
                } else {
                    map_args.push("-map".to_string());
                    map_args.push(format!("0:{}", idx));
                }
                audio_count += 1;
            } else {
                // Map all non-audio streams as-is
                map_args.push("-map".to_string());
                map_args.push(format!("0:{}", idx));
            }
        }
    }
    Ok(map_args)
}

/// Get the duration (in seconds) of any media file (container duration).
pub fn get_file_duration(path: &str) -> anyhow::Result<f64> {
    let output = std::process::Command::new("ffprobe")
        .args([
            "-v",
            "error",
            "-show_entries",
            "format=duration",
            "-of",
            "default=noprint_wrappers=1:nokey=1",
            path,
        ])
        .output()?;
    let duration: f64 = String::from_utf8_lossy(&output.stdout)
        .trim()
        .parse()
        .unwrap_or(0.0);
    Ok(duration)
}
