use crate::audio_metadata::{
    get_audio_stream_duration, get_file_duration, get_stream_bitrate_for_processing,
    inspect_audio_streams, probe_audio_stream,
};
use crate::audio_processing::{
    concat_audio_segments, convert_audio_codec, extract_audio_stream_to_flac, find_quietest_point,
    fit_audio_to_length, remux_audio_stream, split_and_delay_audio,
};
use crate::util::path_to_str;
use crate::{
    cli::Args,
    ffmpeg::{check_dependency, check_ffmpeg_installation, check_ffmpeg_version},
    task::Task,
};
use anyhow::{Result, bail};
use comfy_table::{Table, presets::UTF8_FULL};
use serde_json;
use std::{
    env,
    fs::{self},
    io,
    io::Write,
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

    // Load task file if provided and merge with CLI args
    let task = load_task_from_args(&args)?;
    let input = args
        .input
        .as_ref()
        .or_else(|| task.as_ref().and_then(|t| t.input.as_ref()))
        .ok_or_else(|| anyhow::anyhow!("--input is required"))?;
    let output = args
        .output
        .as_ref()
        .or_else(|| task.as_ref().and_then(|t| t.output.as_ref()))
        .ok_or_else(|| anyhow::anyhow!("--output is required"))?;
    if input == output {
        bail!("Input and output file cannot be the same.");
    }
    let stream = args
        .stream
        .or_else(|| task.as_ref().and_then(|t| t.stream))
        .ok_or_else(|| anyhow::anyhow!("--stream is required"))?;
    let initial_delay = if args.initial_delay != 0.0 {
        args.initial_delay
    } else {
        task.as_ref().and_then(|t| t.initial_delay).unwrap_or(0.0)
    };
    let bitrate = args
        .bitrate
        .clone()
        .or_else(|| task.as_ref().and_then(|t| t.bitrate.clone()));
    let silence_threshold = if args.silence_threshold != -95.0 {
        args.silence_threshold
    } else {
        task.as_ref()
            .and_then(|t| t.silence_threshold)
            .unwrap_or(-95.0)
    };
    let splits = if !args.splits.is_empty() {
        args.splits.clone()
    } else {
        task.as_ref().map(|t| t.splits.clone()).unwrap_or_default()
    };
    let split_ranges = if !args.split_ranges.is_empty() {
        args.split_ranges.clone()
    } else {
        task.as_ref()
            .map(|t| t.split_ranges.clone())
            .unwrap_or_default()
    };
    let fit_length = if args.fit_length {
        true
    } else {
        task.as_ref().and_then(|t| t.fit_length).unwrap_or(false)
    };

    check_ffmpeg_version(args.ignore_ffmpeg_version)?;
    check_dependency("ffprobe")?;

    // Make temp dir for files
    let tmpdir = env::temp_dir().join(format!("split_audio_{}", std::process::id()));
    fs::create_dir_all(&tmpdir)?;

    // Get audio stream metadata
    let audio_meta = probe_audio_stream(input, stream)?;
    println!("ℹ️ Original audio codec: {}", audio_meta.codec);

    // Determine bitrate
    let bitrate = if let Some(b) = bitrate {
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
    let original_codec = audio_meta.codec.clone();
    let original_title = audio_meta.title.clone();
    let original_lang = audio_meta.language.clone();
    let audio_stream_idx = audio_meta.stream_index;

    let flac_path = tmpdir.join("target_audio.flac");

    // 1. Extract target audio to temporary file for analysis
    println!("ℹ️ Extracting target audio track to temporary FLAC file...");
    extract_audio_stream_to_flac(input, stream, flac_path.as_path(), args.debug)?;

    // 2. Resolve split points
    println!("ℹ️ Resolving split points...");
    let mut all_splits: Vec<(f64, f64, String)> = Vec::new();
    if !splits.is_empty() {
        for split in &splits {
            all_splits.push((split.time, split.delay, format!("{:.3}", split.time)));
        }
    }
    if !split_ranges.is_empty() {
        for range in &split_ranges {
            println!(
                "ℹ️ Finding quietest point in range {:.3}s - {:.3}s",
                range.start, range.end
            );
            let result = find_quietest_point(
                &flac_path,
                range.start,
                range.end,
                silence_threshold,
                args.debug,
            )?;
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

    all_splits.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));

    // --- User Confirmation ---
    if !all_splits.is_empty() {
        // Get audio duration for the selected stream
        let audio_duration = match get_audio_stream_duration(input, stream) {
            Ok(Some(dur)) => format!("{:.3} s", dur),
            Ok(None) => "unknown".to_string(),
            Err(_) => "unknown".to_string(),
        };

        let mut table = Table::new();
        table
            .set_header(vec!["Source", "Resolved Split (s)", "Delay (ms)"])
            .load_preset(UTF8_FULL);

        for (point, delay, source) in &all_splits {
            table.add_row(vec![
                source.clone(),
                format!("{:.3}", point),
                format!("{:.3}", delay),
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
            .add_row(vec!["Output File", output])
            .add_row(vec!["Audio Duration", &audio_duration]);

        let stream_name = if !original_title.is_empty() {
            original_title.clone()
        } else if !original_lang.is_empty() {
            original_lang.clone()
        } else {
            "Untitled".to_string()
        };

        info_table
            .add_row(vec!["Initial Delay", &format!("{:.3} ms", initial_delay)])
            .add_row(vec!["Stream ID", &format!("#{}", stream)])
            .add_row(vec!["Stream Name", &stream_name])
            .add_row(vec!["Codec", &original_codec])
            .add_row(vec!["Bitrate", &bitrate])
            .add_row(vec![
                "Silence Threshold",
                &format!("{:.1} LUFS", silence_threshold),
            ]);

        println!("\n▶️ Job Details:");
        println!("{info_table}");

        if args.yes {
            println!("\n--yes flag provided, proceeding without confirmation.");
        } else {
            println!("\nProceed with this plan? [y/N]");
            let mut input = String::new();
            io::stdin().read_line(&mut input)?;
            if !input.trim().eq_ignore_ascii_case("y") {
                println!("Aborting operation.");
                fs::remove_dir_all(&tmpdir)?;
                return Ok(());
            }
        }
    }

    // Optionally write the task to a file (after confirmation)
    if let Some(write_task_file) = &args.write_task_file {
        let out_path = if let Some(path) = write_task_file {
            path.clone().to_string()
        } else {
            // Use input file path with extension replaced by .json
            let input_path = std::path::Path::new(input);
            let mut out = input_path.to_path_buf();
            out.set_extension("json");
            out.to_string_lossy().to_string()
        };
        let task = Task {
            input: Some(input.to_string()),
            output: Some(output.to_string()),
            stream: Some(stream),
            initial_delay: Some(initial_delay),
            splits: splits.clone(),
            split_ranges: split_ranges.clone(),
            bitrate: Some(bitrate.clone()),
            silence_threshold: Some(silence_threshold),
            fit_length: Some(fit_length),
        };
        let json = serde_json::to_string_pretty(&task)?;
        let mut file = fs::File::create(&out_path)?;
        file.write_all(json.as_bytes())?;
        println!("✅ Wrote task to {}", out_path);
    }

    let mut split_points: Vec<f64> = Vec::new();
    let mut delays: Vec<f64> = vec![initial_delay];
    for (point, delay, _) in &all_splits {
        split_points.push(*point);
        delays.push(*delay);
    }

    let n = split_points.len();
    if delays.len() != n + 1 {
        bail!("Delays must have one more element than split points.");
    }

    // 3. Split and delay
    println!("ℹ️ Splitting audio into parts...");
    let split_files = split_and_delay_audio(
        flac_path.as_path(),
        &split_points,
        &delays,
        tmpdir.as_path(),
        args.debug,
    )?;

    // 4. Concat list
    let final_flac = concat_audio_segments(&split_files, tmpdir.as_path(), args.debug)?;

    // --- Fit to original length if requested ---
    println!("\n▶️ Adjusting Audio Lengths...");

    let mut fitted_flac = final_flac.clone();
    let mut orig_duration_val = None;
    let mut processed_duration_val = None;
    let mut adjusted_duration_val = None;
    if fit_length {
        if let Ok(Some(orig_duration)) = get_audio_stream_duration(input, stream) {
            orig_duration_val = Some(orig_duration);
            // Get duration of the processed audio
            let processed_duration = get_file_duration(path_to_str(final_flac.as_path())?)?;
            processed_duration_val = Some(processed_duration);
            let fitted_path = tmpdir.join("target_audio_final_fitted.flac");
            fit_audio_to_length(
                final_flac.as_path(),
                fitted_path.as_path(),
                orig_duration,
                args.debug,
            )?;
            fitted_flac = fitted_path;
            // Get duration of the adjusted audio
            let adjusted_duration = get_file_duration(path_to_str(fitted_flac.as_path())?)?;
            adjusted_duration_val = Some(adjusted_duration);
        }
    }

    // Show duration table if fit_length was used
    if fit_length {
        use comfy_table::Table;
        let mut dur_table = Table::new();
        dur_table.set_header(vec!["Type", "Duration (s)"]);
        let orig_str = orig_duration_val
            .map(|v| format!("{:.3}", v))
            .unwrap_or_else(|| "unknown".to_string());
        let new_str = processed_duration_val
            .map(|v| format!("{:.3}", v))
            .unwrap_or_else(|| "unknown".to_string());
        let adj_str = adjusted_duration_val
            .map(|v| format!("{:.3}", v))
            .unwrap_or_else(|| "unknown".to_string());
        dur_table.add_row(vec!["Original", orig_str.as_str()]);
        dur_table.add_row(vec!["New (pre-adjustment)", new_str.as_str()]);
        dur_table.add_row(vec!["Adjusted (post-fit)", adj_str.as_str()]);
        println!("{}", dur_table);
    }

    // 5. Convert final audio back to original codec
    println!("\n▶️ Converting Audio Back to Original Codec...");
    let final_extension = match original_codec.as_str() {
        "aac" => "aac",
        "ac3" => "ac3",
        "dts" => "dts",
        "mp3" => "mp3",
        "opus" => "opus",
        _ => "mka", // Matroska audio as a safe fallback container
    };
    let final_audio_for_remux = tmpdir.join(format!("final_for_remux.{}", final_extension));
    convert_audio_codec(
        fitted_flac.as_path(),
        &original_codec,
        &bitrate,
        final_audio_for_remux.as_path(),
        args.debug,
    )?;

    // 6. Remux audio back in place of the original
    println!("\n▶️ Remux Audio Back in Place of the Original..");
    remux_audio_stream(
        input,
        final_audio_for_remux.as_path(),
        output,
        audio_stream_idx,
        &original_title,
        &original_lang,
        args.debug,
    )?;

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

fn load_task_from_args(args: &Args) -> anyhow::Result<Option<Task>> {
    match &args.task {
        Some(Some(path)) => Task::load(Some(path.as_str())),
        Some(None) | None => Ok(None),
    }
}
