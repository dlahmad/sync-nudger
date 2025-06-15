use crate::audio_metadata::build_stream_map_args;
use crate::ffmpeg::run_ffmpeg;
use anyhow::Result;
use std::path::Path;
use std::path::PathBuf;

/// Helper to convert a Path to &str, returning an error if not valid UTF-8.
fn path_to_str(path: &Path) -> anyhow::Result<&str> {
    path.to_str()
        .ok_or_else(|| anyhow::anyhow!("Invalid path (not UTF-8)"))
}

/// Extract a specific audio stream from a media file to a FLAC file using ffmpeg.
pub fn extract_audio_stream_to_flac(
    input: &str,
    stream: usize,
    output_path: &std::path::Path,
    debug: bool,
) -> anyhow::Result<()> {
    let output_path_str = path_to_str(output_path)?;
    crate::ffmpeg::run_ffmpeg(
        &[
            "-y",
            "-i",
            input,
            "-map",
            &format!("0:{}", stream),
            "-c:a",
            "flac",
            output_path_str,
        ],
        debug,
    )?;
    Ok(())
}

/// Split and delay audio segments according to split points and delays.
/// Returns a Vec<PathBuf> of the resulting split files.
pub fn split_and_delay_audio(
    flac_path: &Path,
    split_points: &[f64],
    delays: &[f64],
    tmpdir: &Path,
    debug: bool,
) -> Result<Vec<PathBuf>> {
    let n = split_points.len();
    let mut split_files = Vec::new();
    let mut prev = 0.0f64;
    for i in 0..=n {
        let part = tmpdir.join(format!("part_{}.flac", i + 1));
        let (start, duration) = (prev, if i < n { split_points[i] - prev } else { 0.0 });
        let start_str = start.to_string();
        let mut ffmpeg_args = vec!["-y", "-i", path_to_str(flac_path)?, "-ss", &start_str];
        let duration_str;
        if i < n {
            duration_str = duration.to_string();
            ffmpeg_args.push("-t");
            ffmpeg_args.push(&duration_str);
            prev = split_points[i];
        }
        ffmpeg_args.extend_from_slice(&[
            "-af",
            "asetpts=PTS-STARTPTS",
            "-c:a",
            "flac",
            path_to_str(&part)?,
        ]);
        run_ffmpeg(&ffmpeg_args, debug)?;
        let delay = delays[i];
        let target = if delay > 0.0 {
            let delayed = tmpdir.join(format!("part_{}_delayed.flac", i + 1));
            let delay_str = delay.to_string();
            run_ffmpeg(
                &[
                    "-y",
                    "-i",
                    path_to_str(&part)?,
                    "-filter_complex",
                    &format!("adelay={}|{},asetpts=PTS-STARTPTS", delay_str, delay_str),
                    "-c:a",
                    "flac",
                    path_to_str(&delayed)?,
                ],
                debug,
            )?;
            std::fs::remove_file(&part)?;
            delayed
        } else if delay < 0.0 {
            let trimmed = tmpdir.join(format!("part_{}_trimmed.flac", i + 1));
            let trim_s = (-delay as f64) / 1000.0;
            let trim_s_str = trim_s.to_string();
            run_ffmpeg(
                &[
                    "-y",
                    "-i",
                    path_to_str(&part)?,
                    "-ss",
                    &trim_s_str,
                    "-af",
                    "asetpts=PTS-STARTPTS",
                    "-c:a",
                    "flac",
                    path_to_str(&trimmed)?,
                ],
                debug,
            )?;
            std::fs::remove_file(&part)?;
            trimmed
        } else {
            part
        };
        split_files.push(target);
    }
    Ok(split_files)
}

/// Concatenate audio segments using ffmpeg concat filter. Returns the path to the final FLAC file.
pub fn concat_audio_segments(
    split_files: &[PathBuf],
    tmpdir: &Path,
    debug: bool,
) -> Result<PathBuf> {
    let mut concat_args: Vec<String> = vec!["-y".to_string()];
    for s in split_files {
        concat_args.push("-i".to_string());
        concat_args.push(path_to_str(s)?.to_string());
    }
    let filter_complex_str = (0..split_files.len())
        .map(|i| format!("[{}:a]", i))
        .collect::<String>()
        + &format!("concat=n={}:v=0:a=1[a]", split_files.len());
    concat_args.push("-filter_complex".to_string());
    concat_args.push(filter_complex_str);
    concat_args.push("-map".to_string());
    concat_args.push("[a]".to_string());
    let final_flac = tmpdir.join("target_audio_final.flac");
    concat_args.push("-c:a".to_string());
    concat_args.push("flac".to_string());
    concat_args.push(path_to_str(&final_flac)?.to_string());
    let concat_args_slice: Vec<&str> = concat_args.iter().map(|s| s.as_str()).collect();
    run_ffmpeg(&concat_args_slice, debug)?;
    Ok(final_flac)
}

/// Convert FLAC audio to the target codec and bitrate. Returns the output path.
pub fn convert_audio_codec(
    input_flac: &Path,
    codec: &str,
    bitrate: &str,
    output_path: &Path,
    debug: bool,
) -> Result<()> {
    run_ffmpeg(
        &[
            "-y",
            "-i",
            path_to_str(input_flac)?,
            "-af",
            "asetpts=PTS-STARTPTS",
            "-c:a",
            codec,
            "-b:a",
            bitrate,
            path_to_str(output_path)?,
        ],
        debug,
    )?;
    Ok(())
}

/// Trim or pad the audio at input_path to match target_duration (seconds), writing to output_path.
/// If the input is longer, it is trimmed. If shorter, it is padded with silence.
pub fn fit_audio_to_length(
    input_path: &Path,
    output_path: &Path,
    target_duration: f64,
    debug: bool,
) -> Result<()> {
    // Get duration of the input audio
    let output = std::process::Command::new("ffprobe")
        .args([
            "-v",
            "error",
            "-show_entries",
            "format=duration",
            "-of",
            "default=noprint_wrappers=1:nokey=1",
            path_to_str(input_path)?,
        ])
        .output()?;
    let input_duration: f64 = String::from_utf8_lossy(&output.stdout)
        .trim()
        .parse()
        .unwrap_or(0.0);
    if input_duration > target_duration + 0.001 {
        // Trim to target duration
        run_ffmpeg(
            &[
                "-y",
                "-i",
                path_to_str(input_path)?,
                "-af",
                &format!("atrim=0:{:.6}", target_duration),
                "-c:a",
                "flac",
                path_to_str(output_path)?,
            ],
            debug,
        )?;
    } else if input_duration < target_duration - 0.001 {
        // Pad with silence to target duration
        let pad_len = target_duration - input_duration;
        run_ffmpeg(
            &[
                "-y",
                "-i",
                path_to_str(input_path)?,
                "-af",
                &format!("apad=pad_dur={:.6}", pad_len),
                "-t",
                &format!("{:.6}", target_duration),
                "-c:a",
                "flac",
                path_to_str(output_path)?,
            ],
            debug,
        )?;
    } else {
        // Already matches duration, just copy
        std::fs::copy(input_path, output_path)?;
    }
    Ok(())
}

/// Remux the new audio stream in place of the original audio stream in the input file.
pub fn remux_audio_stream(
    input: &str,
    new_audio: &std::path::Path,
    output: &str,
    audio_stream_idx: usize,
    original_title: &str,
    original_lang: &str,
    debug: bool,
) -> anyhow::Result<()> {
    let map_args = build_stream_map_args(input, audio_stream_idx)?;
    let metadata_spec = format!("-metadata:s:a:{}", audio_stream_idx);
    let title_value = format!("title={}", original_title);
    let lang_value = format!("language={}", original_lang);
    let mut ffmpeg_remux = vec!["-y", "-i", input, "-i", path_to_str(new_audio)?];
    ffmpeg_remux.extend(map_args.iter().map(|s| s.as_str()));
    ffmpeg_remux.push("-c");
    ffmpeg_remux.push("copy");
    if !original_lang.is_empty() {
        ffmpeg_remux.push(&metadata_spec);
        ffmpeg_remux.push(&lang_value);
    }
    if !original_title.is_empty() {
        ffmpeg_remux.push(&metadata_spec);
        ffmpeg_remux.push(&title_value);
    }
    ffmpeg_remux.push(output);
    crate::ffmpeg::run_ffmpeg(&ffmpeg_remux, debug)?;
    Ok(())
}
