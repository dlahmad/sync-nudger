use anyhow::{Result, bail};
use std::process::Command;

/// Struct to hold audio stream metadata
pub struct AudioStreamMetadata {
    pub stream_index: usize,
    pub codec: String,
    pub title: String,
    pub language: String,
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
            if parts[0].parse::<usize>().unwrap() == stream {
                audio_stream_idx = audio_count;
                original_codec = parts[1].to_string();
                break;
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
