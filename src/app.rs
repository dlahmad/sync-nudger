use crate::{
    cli::Args,
    ffmpeg::{
        check_dependency, check_ffmpeg_installation, check_ffmpeg_version, find_quietest_point,
        get_stream_bitrate_for_processing, inspect_audio_streams, run_ffmpeg,
    },
};
use anyhow::{Result, bail};
use comfy_table::{Table, presets::UTF8_FULL};
use std::{
    env,
    fs::{self},
    io,
    process::Command,
};

pub fn run(args: Args) -> Result<()> {
    // Handle --check-ffmpeg command
    if args.check_ffmpeg {
        return handle_ffmpeg_check();
    }

    // Handle --inspect command
    if args.inspect {
        let input = args
            .input
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("--input is required for inspection"))?;
        return handle_inspect(input);
    }

    // Validate required arguments for normal operation
    let input = args
        .input
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("--input is required"))?;
    let output = args
        .output
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("--output is required"))?;
    if input == output {
        bail!("Input and output file cannot be the same.");
    }
    let stream = args
        .stream
        .ok_or_else(|| anyhow::anyhow!("--stream is required"))?;

    check_ffmpeg_version(args.ignore_ffmpeg_version)?;
    check_dependency("ffprobe")?;

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
    if audio_stream_idx < 0 {
        bail!("Could not find audio stream {} in mapping", stream);
    }
    if original_codec.is_empty() {
        bail!("Could not determine codec for audio stream {}", stream);
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
            input,
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
            input,
        ])
        .output()?;
    let original_lang = String::from_utf8_lossy(&ffprobe_lang.stdout)
        .trim()
        .to_owned();

    // Determine bitrate
    let bitrate = if let Some(b) = args.bitrate.clone() {
        println!("ℹ️ Using user-provided bitrate: {}", b);
        b
    } else {
        // Use improved bitrate detection
        match get_stream_bitrate_for_processing(input, stream) {
            Ok(detected_bitrate) => {
                println!("ℹ️ Automatically detected bitrate: {}", detected_bitrate);
                detected_bitrate
            }
            Err(e) => {
                bail!("{}", e);
            }
        }
    };

    let flac_path = tmpdir.join("target_audio.flac");

    // 1. Extract target audio to temporary file for analysis
    println!("ℹ️ Extracting target audio track to temporary FLAC file...");
    run_ffmpeg(
        &[
            "-y",
            "-i",
            input,
            "-map",
            &format!("0:{}", stream),
            "-c:a",
            "flac",
            flac_path.to_str().unwrap(),
        ],
        args.debug,
    )?;

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
        for range in &args.split_ranges {
            println!(
                "ℹ️ Finding quietest point in range {:.3}s - {:.3}s",
                range.start, range.end
            );
            let result = find_quietest_point(
                &flac_path,
                range.start,
                range.end,
                args.silence_threshold,
                args.debug,
            )?;

            // Display debug output if available
            if let Some(debug_output) = &result.debug_output {
                eprintln!("{}", debug_output);
            }

            println!(
                "  ✅ Found quietest point at {:.3}s (Loudness: {:.2} LUFS)",
                result.time, result.loudness
            );
            all_splits.push((
                result.time,
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
            .add_row(vec!["Input File", input])
            .add_row(vec!["Output File", output]);

        let stream_name = if !original_title.is_empty() {
            original_title.clone()
        } else if !original_lang.is_empty() {
            original_lang.clone()
        } else {
            "Untitled".to_string()
        };

        info_table
            .add_row(vec!["Initial Delay", &format!("{} ms", args.initial_delay)])
            .add_row(vec!["Stream ID", &format!("#{}", stream)])
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
        run_ffmpeg(&ffmpeg_args, args.debug)?;

        let delay = delays[i];
        let target = if delay > 0 {
            let delayed = tmpdir.join(format!("part_{}_delayed.flac", i + 1));
            let delay_str = delay.to_string();
            run_ffmpeg(
                &[
                    "-y",
                    "-i",
                    part.to_str().unwrap(),
                    "-filter_complex",
                    &format!("adelay={}|{},asetpts=PTS-STARTPTS", delay_str, delay_str),
                    "-c:a",
                    "flac",
                    delayed.to_str().unwrap(),
                ],
                args.debug,
            )?;
            fs::remove_file(&part)?;
            delayed
        } else if delay < 0 {
            let trimmed = tmpdir.join(format!("part_{}_trimmed.flac", i + 1));
            let trim_s = (-delay as f64) / 1000.0;
            let trim_s_str = trim_s.to_string();
            run_ffmpeg(
                &[
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
                ],
                args.debug,
            )?;
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
    run_ffmpeg(&concat_args_slice, args.debug)?;

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
    run_ffmpeg(
        &[
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
        ],
        args.debug,
    )?;

    // 6. Remux audio back in place of the original
    let mut map_args: Vec<String> = Vec::new();
    audio_count = 0;
    for line in streams_info.lines() {
        let parts: Vec<_> = line.split(',').collect();
        if parts.len() == 3 && parts[2] == "audio" {
            if parts[0].parse::<usize>().unwrap() == stream {
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
        input,
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
    ffmpeg_remux.push(output);
    run_ffmpeg(&ffmpeg_remux, args.debug)?;

    // Cleanup
    fs::remove_dir_all(&tmpdir)?;

    println!("✅ Processing complete! Output: {}", output);
    Ok(())
}

fn handle_ffmpeg_check() -> Result<()> {
    println!("🔍 Checking FFmpeg installation...\n");

    let check_result = check_ffmpeg_installation();

    // Display FFmpeg status
    if check_result.ffmpeg_available {
        if let Some(version_info) = &check_result.ffmpeg_version {
            println!("✅ FFmpeg found:");
            println!(
                "   Version: {}.{}.{}",
                version_info.major, version_info.minor, version_info.patch
            );

            if version_info.is_compatible {
                println!("   Status: ✅ Compatible (minimum required: 4.0.0)");
            } else {
                println!("   Status: ❌ Too old (minimum required: 4.0.0)");
            }

            if version_info.is_tested_version {
                println!("   Note: This is the tested version");
            } else {
                println!("   Note: Tested with version 7.1.x");
            }
        } else {
            println!("⚠️  Could not parse FFmpeg version from output");
        }
    } else if let Some(error) = &check_result.error {
        println!("❌ FFmpeg not found in PATH");
        println!("   Please install FFmpeg and ensure it's accessible from the command line");
        bail!("FFmpeg is required but not installed: {}", error);
    }

    println!();

    // Display FFprobe status
    if check_result.ffprobe_available {
        println!("✅ FFprobe found and working");
    } else {
        println!("❌ FFprobe not found in PATH");
        bail!("FFprobe is required but not installed");
    }

    println!();

    // Display filter availability
    if check_result.ebur128_filter_available {
        println!("✅ Required filter 'ebur128' is available");
    } else {
        println!("❌ Required filter 'ebur128' not found");
        println!("   This filter is needed for loudness analysis");
    }

    println!("\n🎉 FFmpeg check complete!");
    Ok(())
}

fn handle_inspect(input: &str) -> Result<()> {
    println!("🔍 Inspecting audio streams in: {}\n", input);

    let streams = inspect_audio_streams(input)?;

    if streams.is_empty() {
        println!("❌ No audio streams found in the input file.");
        return Ok(());
    }

    let mut table = Table::new();
    table.load_preset(UTF8_FULL);
    table.set_header(vec![
        "Index",
        "Codec",
        "Channels",
        "Sample Rate",
        "Bitrate",
        "Language",
        "Title",
    ]);

    for stream in streams {
        table.add_row(vec![
            stream.index.to_string(),
            stream.codec,
            stream.channels,
            stream.sample_rate,
            stream.bitrate,
            stream.language,
            stream.title,
        ]);
    }

    println!("{}", table);
    println!("\n💡 Use the 'Index' value with --stream to select an audio stream for processing.");

    Ok(())
}
