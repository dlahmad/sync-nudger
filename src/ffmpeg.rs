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
