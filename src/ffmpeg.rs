// NOTE: This file requires the `thiserror` crate. If you see unresolved import errors for `thiserror`, run:
//     cargo add thiserror
// to add it to your Cargo.toml.

use regex::Regex;
use std::{
    io,
    process::{Command, Stdio},
};
use thiserror::Error;

const EXPECTED_FFMPEG_MAJOR_VERSION: u32 = 7;
const EXPECTED_FFMPEG_MINOR_VERSION: u32 = 1;
const MINIMUM_FFMPEG_MAJOR_VERSION: u32 = 4;

#[derive(Debug)]
pub struct FFmpegVersionInfo {
    pub major: u32,
    pub minor: u32,
    pub patch: u32,
    pub is_compatible: bool,
    pub is_tested_version: bool,
}

#[derive(Debug)]
pub struct FFmpegCheckResult {
    pub ffmpeg_available: bool,
    pub ffmpeg_version: Option<FFmpegVersionInfo>,
    pub ffprobe_available: bool,
    pub ebur128_filter_available: bool,
    pub error: Option<String>,
}

#[derive(Debug, Error)]
pub enum FFmpegError {
    #[error(
        "FFmpeg version mismatch. Expected v{expected_major}.{expected_minor}, but found v{found_major}.{found_minor}. Use --ignore-ffmpeg-version to bypass."
    )]
    VersionMismatch {
        expected_major: u32,
        expected_minor: u32,
        found_major: u32,
        found_minor: u32,
    },
    #[error("Could not parse ffmpeg version from output. Use --ignore-ffmpeg-version to bypass.")]
    VersionParseError,
    #[error("Could not run `ffmpeg -version` to check version.")]
    FFmpegVersionCheckFailed,
    #[error("`{0}` command not found. Please ensure it is installed and in your PATH.")]
    CommandNotFound(String),
    #[error("Failed to run `{0}`: {1}")]
    CommandFailed(String, String),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Regex(#[from] regex::Error),
    #[error(transparent)]
    ParseInt(#[from] std::num::ParseIntError),
    #[error(transparent)]
    SerdeJson(#[from] serde_json::Error),
    #[error("")]
    BitrateUndetermined { stream_index: usize },
}

pub fn run_ffmpeg(args: &[&str], debug: bool) -> Result<(), FFmpegError> {
    let mut command = Command::new("ffmpeg");
    command.args(args);

    if !debug {
        command.stdout(Stdio::null()).stderr(Stdio::null());
    }

    let status = command.status()?;
    if !status.success() {
        return Err(FFmpegError::CommandFailed(
            args.join(" "),
            "FFmpeg failed".to_string(),
        ));
    }
    Ok(())
}

pub fn check_ffmpeg_version(ignore_check: bool) -> Result<(), FFmpegError> {
    if ignore_check {
        return Ok(());
    }

    let output = Command::new("ffmpeg").arg("-version").output()?;
    if !output.status.success() {
        return Err(FFmpegError::FFmpegVersionCheckFailed);
    }

    let version_info = String::from_utf8_lossy(&output.stdout);
    let re = Regex::new(r"ffmpeg version (\d+)\.(\d+)")?;

    if let Some(caps) = re.captures(&version_info) {
        let major: u32 = caps
            .get(1)
            .ok_or(FFmpegError::VersionParseError)?
            .as_str()
            .parse()?;
        let minor: u32 = caps
            .get(2)
            .ok_or(FFmpegError::VersionParseError)?
            .as_str()
            .parse()?;

        if major == EXPECTED_FFMPEG_MAJOR_VERSION && minor == EXPECTED_FFMPEG_MINOR_VERSION {
            Ok(())
        } else {
            Err(FFmpegError::VersionMismatch {
                expected_major: EXPECTED_FFMPEG_MAJOR_VERSION,
                expected_minor: EXPECTED_FFMPEG_MINOR_VERSION,
                found_major: major,
                found_minor: minor,
            })
        }
    } else {
        Err(FFmpegError::VersionParseError)
    }
}

pub fn check_dependency(cmd: &str) -> Result<(), FFmpegError> {
    match Command::new(cmd)
        .arg("-version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
    {
        Ok(_) => Ok(()),
        Err(e) => {
            if e.kind() == io::ErrorKind::NotFound {
                Err(FFmpegError::CommandNotFound(cmd.to_string()))
            } else {
                Err(FFmpegError::CommandFailed(cmd.to_string(), e.to_string()))
            }
        }
    }
}

pub fn check_ffmpeg_installation() -> FFmpegCheckResult {
    let mut result = FFmpegCheckResult {
        ffmpeg_available: false,
        ffmpeg_version: None,
        ffprobe_available: false,
        ebur128_filter_available: false,
        error: None,
    };

    // Check if ffmpeg is available
    match Command::new("ffmpeg").arg("-version").output() {
        Ok(output) => {
            if output.status.success() {
                result.ffmpeg_available = true;

                let version_info = String::from_utf8_lossy(&output.stdout);
                let re = Regex::new(r"ffmpeg version (\d+)\.(\d+)(?:\.(\d+))?").unwrap();

                if let Some(caps) = re.captures(&version_info) {
                    let major: u32 = caps
                        .get(1)
                        .and_then(|m| m.as_str().parse().ok())
                        .unwrap_or(0);
                    let minor: u32 = caps
                        .get(2)
                        .and_then(|m| m.as_str().parse().ok())
                        .unwrap_or(0);
                    let patch: u32 = caps.get(3).map_or(0, |m| m.as_str().parse().unwrap_or(0));

                    result.ffmpeg_version = Some(FFmpegVersionInfo {
                        major,
                        minor,
                        patch,
                        is_compatible: major >= MINIMUM_FFMPEG_MAJOR_VERSION,
                        is_tested_version: major == EXPECTED_FFMPEG_MAJOR_VERSION
                            && minor == EXPECTED_FFMPEG_MINOR_VERSION,
                    });
                }
            }
        }
        Err(e) => {
            if e.kind() == io::ErrorKind::NotFound {
                result.error = Some("FFmpeg not found in PATH".to_string());
            } else {
                result.error = Some(format!("Failed to check FFmpeg: {}", e));
            }
        }
    }

    // Check if ffprobe is available
    match Command::new("ffprobe").arg("-version").output() {
        Ok(output) => {
            result.ffprobe_available = output.status.success();
        }
        Err(_) => {
            result.ffprobe_available = false;
        }
    }

    // Check for required filter
    match Command::new("ffmpeg")
        .args(&["-hide_banner", "-filters"])
        .output()
    {
        Ok(output) => {
            let filters = String::from_utf8_lossy(&output.stdout);
            result.ebur128_filter_available = filters.contains("ebur128");
        }
        Err(_) => {
            result.ebur128_filter_available = false;
        }
    }

    result
}
