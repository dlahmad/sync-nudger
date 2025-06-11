use anyhow::{Result, bail};
use clap::Parser;
use comfy_table::{Table, presets::UTF8_FULL};
use regex::Regex;
use std::{
    env,
    fs::{self},
    io,
    process::{Command, Stdio},
};

/// Rust version of the multi-split/delay audio tool
#[derive(Parser, Debug)]
#[command(author, version, about)]
struct Args {
    /// Input MKV file
    #[arg(short, long)]
    input: String,

    /// Output MKV file
    #[arg(short, long)]
    output: String,

    /// Audio stream index (e.g. 6)
    #[arg(short, long)]
    stream: usize,

    /// Delay for the first audio segment in ms.
    #[arg(long, default_value_t = 0)]
    initial_delay: i32,

    /// Split points and subsequent delays, in format <seconds>:<delay_ms>.
    /// Can be specified multiple times. E.g. --split 177.3:360 --split 672.3:360
    #[arg(short = 'p', long = "split", value_parser = parse_split, num_args = 1..)]
    splits: Vec<(f64, i32)>,

    /// Split ranges and subsequent delays, in format <start_time>:<end_time>:<delay_ms>.
    /// Can be specified multiple times. E.g. --split-range 177.3:672.3:360
    #[arg(long = "split-range", value_parser = parse_split_range, num_args = 1..)]
    split_ranges: Vec<SplitRange>,

    /// Output bitrate (e.g. 80k). If not provided, it will be detected automatically.
    #[arg(short, long)]
    bitrate: Option<String>,

    /// Loudness threshold (in LUFS) to consider a point as audible.
    /// Used to distinguish quiet audio from pure digital silence.
    /// For 16-bit audio, the theoretical dynamic range is 96dB, so -95 is a good default.
    #[arg(long, default_value_t = -95.0)]
    silence_threshold: f64,
}

#[derive(Debug, Clone, Copy)]
struct SplitRange {
    start: f64,
    end: f64,
    delay: i32,
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

fn run_ffmpeg(args: &[&str]) -> Result<()> {
    let status = Command::new("ffmpeg").args(args).status()?;
    if !status.success() {
        bail!("FFmpeg failed: {:?}", args);
    }
    Ok(())
}

fn check_dependency(cmd: &str) -> Result<()> {
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

fn find_quietest_point(
    audio_path: &std::path::Path,
    start: f64,
    end: f64,
    silence_threshold: f64,
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

fn main() -> Result<()> {
    check_dependency("ffmpeg")?;
    check_dependency("ffprobe")?;

    let args = Args::parse();

    // Make temp dir for files
    let tmpdir = env::temp_dir().join(format!("split_audio_{}", std::process::id()));
    fs::create_dir_all(&tmpdir)?;

    // Get audio stream info to find audio track index and bitrate
    let ffprobe_streams = Command::new("ffprobe")
        .args(&[
            "-v",
            "error",
            "-show_entries",
            "stream=index,codec_type,codec_name",
            "-of",
            "csv=p=0",
            &args.input,
        ])
        .output()?;
    let streams_info = String::from_utf8_lossy(&ffprobe_streams.stdout);

    let mut audio_count = 0;
    let mut audio_stream_idx = -1isize;
    let mut original_codec = String::new();

    for line in streams_info.lines() {
        let parts: Vec<_> = line.split(',').collect();
        if parts.len() >= 3 && parts[2] == "audio" {
            if parts[0].parse::<usize>().unwrap() == args.stream {
                audio_stream_idx = audio_count;
                original_codec = parts[1].to_string();
                break;
            }
            audio_count += 1;
        }
    }
    if audio_stream_idx < 0 {
        bail!("Could not find audio stream {} in mapping", args.stream);
    }
    if original_codec.is_empty() {
        bail!("Could not determine codec for audio stream {}", args.stream);
    }
    println!("ℹ️ Original audio codec: {}", original_codec);

    // Get original audio title
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
            &args.input,
        ])
        .output()?;
    let original_title = String::from_utf8_lossy(&ffprobe_title.stdout)
        .trim()
        .to_owned();

    // Get original audio language
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
            &args.input,
        ])
        .output()?;
    let original_lang = String::from_utf8_lossy(&ffprobe_lang.stdout)
        .trim()
        .to_owned();

    // Determine bitrate
    let bitrate = if let Some(b) = args.bitrate {
        println!("ℹ️ Using user-provided bitrate: {}", b);
        b
    } else {
        // Get bitrate from input stream
        let ffprobe_bitrate = Command::new("ffprobe")
            .args(&[
                "-v",
                "error",
                "-select_streams",
                &format!("a:{}", audio_stream_idx),
                "-show_entries",
                "stream=bit_rate",
                "-of",
                "default=noprint_wrappers=1:nokey=1",
                &args.input,
            ])
            .output()?;
        let bitrate_str = String::from_utf8_lossy(&ffprobe_bitrate.stdout)
            .trim()
            .to_owned();

        if bitrate_str == "N/A" || bitrate_str.is_empty() {
            bail!(
                "Could not determine bitrate automatically. Please provide it with --bitrate <bitrate> (e.g. 80k)"
            );
        } else {
            let bitrate_bps: u32 = bitrate_str.parse()?;
            let b = format!("{}k", bitrate_bps / 1000);
            println!("ℹ️ Automatically detected bitrate: {}", b);
            b
        }
    };

    let flac_path = tmpdir.join("target_audio.flac");

    // 1. Extract target audio to temporary file for analysis
    println!("ℹ️ Extracting target audio track to temporary FLAC file...");
    run_ffmpeg(&[
        "-y",
        "-i",
        &args.input,
        "-map",
        &format!("0:{}", args.stream),
        "-c:a",
        "flac",
        flac_path.to_str().unwrap(),
    ])?;

    // 2. Resolve split points
    println!("ℹ️ Resolving split points...");
    // The String will hold the 'source' of the split for the summary table
    let mut all_splits: Vec<(f64, i32, String)> = Vec::new();

    if !args.splits.is_empty() {
        for (point, delay) in &args.splits {
            all_splits.push((*point, *delay, format!("{:.3}", point)));
        }
    }

    if !args.split_ranges.is_empty() {
        for range in args.split_ranges {
            let point =
                find_quietest_point(&flac_path, range.start, range.end, args.silence_threshold)?;
            all_splits.push((
                point,
                range.delay,
                format!("{:.3}-{:.3}", range.start, range.end),
            ));
        }
    }
    all_splits.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());

    // --- User Confirmation ---
    if !all_splits.is_empty() {
        let mut table = Table::new();
        table
            .set_header(vec!["Source", "Resolved Split (s)", "Delay (ms)"])
            .load_preset(UTF8_FULL);

        for (point, delay, source) in &all_splits {
            table.add_row(vec![
                source.clone(),
                format!("{:.3}", point),
                format!("{}", delay),
            ]);
        }

        println!("\n▶️ Proposed Splitting Plan:");
        println!("{table}");

        let mut info_table = Table::new();
        info_table
            .load_preset(UTF8_FULL)
            .set_header(vec!["Parameter", "Value"]);

        info_table
            .add_row(vec!["Input File", &args.input])
            .add_row(vec!["Output File", &args.output]);

        let stream_name = if !original_title.is_empty() {
            original_title.clone()
        } else if !original_lang.is_empty() {
            original_lang.clone()
        } else {
            "Untitled".to_string()
        };

        info_table
            .add_row(vec!["Initial Delay", &format!("{} ms", args.initial_delay)])
            .add_row(vec!["Stream ID", &format!("#{}", args.stream)])
            .add_row(vec!["Stream Name", &stream_name])
            .add_row(vec!["Codec", &original_codec])
            .add_row(vec!["Bitrate", &bitrate])
            .add_row(vec![
                "Silence Threshold",
                &format!("{:.1} LUFS", args.silence_threshold),
            ]);

        println!("\n▶️ Job Details:");
        println!("{info_table}");

        println!("\nProceed with this plan? [y/N]");

        let mut input = String::new();
        io::stdin().read_line(&mut input)?;
        if !input.trim().eq_ignore_ascii_case("y") {
            println!("Aborting operation.");
            fs::remove_dir_all(&tmpdir)?;
            return Ok(());
        }
    }

    let mut split_points: Vec<f64> = Vec::new();
    let mut delays: Vec<i32> = vec![args.initial_delay];
    for (point, delay, _) in &all_splits {
        split_points.push(*point);
        delays.push(*delay);
    }

    let n = split_points.len();
    if delays.len() != n + 1 {
        bail!("Delays must have one more element than split points.");
    }

    // 3. Split and delay
    let mut split_files = Vec::new();
    let mut prev = 0.0f64;
    println!("ℹ️ Splitting audio into parts...");
    for i in 0..=n {
        let part = tmpdir.join(format!("part_{}.flac", i + 1));
        let (start, duration) = (prev, if i < n { split_points[i] - prev } else { 0.0 });

        if i < n {
            println!(
                "  - Part {}: Splitting at {:.3}s (segment duration: {:.3}s)",
                i + 1,
                split_points[i],
                duration
            );
        } else {
            println!(
                "  - Part {}: Final segment starting at {:.3}s",
                i + 1,
                start
            );
        }

        let start_str = start.to_string();
        let mut ffmpeg_args = vec!["-y", "-i", flac_path.to_str().unwrap(), "-ss", &start_str];
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
            part.to_str().unwrap(),
        ]);
        run_ffmpeg(&ffmpeg_args)?;

        let delay = delays[i];
        let target = if delay > 0 {
            let delayed = tmpdir.join(format!("part_{}_delayed.flac", i + 1));
            let delay_str = delay.to_string();
            run_ffmpeg(&[
                "-y",
                "-i",
                part.to_str().unwrap(),
                "-filter_complex",
                &format!("adelay={}|{},asetpts=PTS-STARTPTS", delay_str, delay_str),
                "-c:a",
                "flac",
                delayed.to_str().unwrap(),
            ])?;
            fs::remove_file(&part)?;
            delayed
        } else if delay < 0 {
            let trimmed = tmpdir.join(format!("part_{}_trimmed.flac", i + 1));
            let trim_s = (-delay as f64) / 1000.0;
            let trim_s_str = trim_s.to_string();
            run_ffmpeg(&[
                "-y",
                "-i",
                part.to_str().unwrap(),
                "-ss",
                &trim_s_str,
                "-af",
                "asetpts=PTS-STARTPTS",
                "-c:a",
                "flac",
                trimmed.to_str().unwrap(),
            ])?;
            fs::remove_file(&part)?;
            trimmed
        } else {
            part
        };
        split_files.push(target);
    }

    // 4. Concat list
    // Use the concat filter for robustness, as the concat demuxer can fail with timestamp issues.
    let mut concat_args: Vec<String> = vec!["-y".to_string()];
    for s in &split_files {
        concat_args.push("-i".to_string());
        concat_args.push(s.to_str().unwrap().to_string());
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
    concat_args.push(final_flac.to_str().unwrap().to_string());

    let concat_args_slice: Vec<&str> = concat_args.iter().map(|s| s.as_str()).collect();
    run_ffmpeg(&concat_args_slice)?;

    // 5. Convert final audio back to original codec
    let final_extension = match original_codec.as_str() {
        "aac" => "aac",
        "ac3" => "ac3",
        "dts" => "dts",
        "mp3" => "mp3",
        "opus" => "opus",
        _ => "mka", // Matroska audio as a safe fallback container
    };
    let final_audio_for_remux = tmpdir.join(format!("final_for_remux.{}", final_extension));
    run_ffmpeg(&[
        "-y",
        "-i",
        final_flac.to_str().unwrap(),
        "-af",
        "asetpts=PTS-STARTPTS",
        "-c:a",
        &original_codec,
        "-b:a",
        &bitrate,
        final_audio_for_remux.to_str().unwrap(),
    ])?;

    // 6. Remux audio back in place of the original
    let mut map_args: Vec<String> = Vec::new();
    audio_count = 0;
    for line in streams_info.lines() {
        let parts: Vec<_> = line.split(',').collect();
        if parts.len() == 3 && parts[2] == "audio" {
            if parts[0].parse::<usize>().unwrap() == args.stream {
                map_args.push("-map".to_string());
                map_args.push("1:a:0".to_string());
            } else {
                map_args.push("-map".to_string());
                map_args.push(format!("0:a:{}", audio_count));
            }
            audio_count += 1;
        } else if parts.len() == 3 {
            map_args.push("-map".to_string());
            map_args.push(format!("0:{}", parts[0]));
        }
    }

    // Remux
    let metadata_spec = format!("-metadata:s:a:{}", audio_stream_idx);
    let title_value = format!("title={}", original_title);
    let lang_value = format!("language={}", original_lang);

    let mut ffmpeg_remux = vec![
        "-y",
        "-i",
        &args.input,
        "-i",
        final_audio_for_remux.to_str().unwrap(),
    ];
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
    ffmpeg_remux.push(&args.output);
    run_ffmpeg(&ffmpeg_remux)?;

    // Cleanup
    fs::remove_dir_all(&tmpdir)?;

    println!("✅ Processing complete! Output: {}", args.output);
    Ok(())
}
