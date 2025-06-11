# Sync-Nudger: Advanced Audio Splitting Tool

[![Deploy](https://github.com/dlahmad/sync-nudger/actions/workflows/release.yml/badge.svg)](https://github.com/dlahmad/sync-nudger/actions/workflows/release.yml)

Sync-Nudger is a command-line utility designed for precise audio stream manipulation within video files. It allows you to split an audio track at specific timestamps, apply individual delays to each new segment, and then seamlessly remux the modified audio back into the original container.

Its standout feature is the ability to find the quietest point within a given time range, ensuring that your splits are clean and occur during natural pauses in the audio.

## What it does

The tool performs a series of operations to achieve its goal:

1. **Extracts** the target audio stream into a temporary, high-quality FLAC file.
2. **Analyzes** user-specified time ranges to find the quietest moment, using the EBU R 128 loudness standard for perceptual accuracy.
3. **Resolves** all split points, whether provided as exact timestamps or as search ranges.
4. **Asks for Confirmation** by presenting a detailed summary of the proposed changes before proceeding.
5. **Splits** the audio into multiple parts based on the resolved points.
6. **Applies** the specified millisecond delays to each part (or trims them if the delay is negative).
7. **Concatenates** the modified audio parts back into a single stream.
8. **Re-encodes** the audio to its original format and bitrate.
9. **Remuxes** the new audio stream back into the video file, replacing the original while keeping all other video, audio, and subtitle streams intact.

## Features

* **Precise Splitting**: Split audio at exact floating-point timestamps.
* **Quiet Point Detection**: Automatically find the quietest split point within a given time range (`--split-range`).
* **Per-Segment Delay**: Apply a unique delay in milliseconds to each audio segment, including the initial one.
* **User Confirmation**: Displays a detailed summary of the files, streams, and planned splits before executing, preventing accidental changes.
* **Configurable Silence Detection**: Tune the loudness threshold for what the tool considers "audible" vs. silent (`--silence-threshold`).
* **FFmpeg Version Check**: Ensures a compatible version of `ffmpeg` is installed to prevent runtime errors. Can be bypassed (`--ignore-ffmpeg-version`).
* **Debug Logging**: Optional verbose logging from `ffmpeg` for troubleshooting (`--debug`).
* **Automated Releases**: Multi-platform binaries are built automatically via GitHub Actions.

## Installation

### Prerequisites

You must have **`ffmpeg`** and **`ffprobe`** installed and available in your system's `PATH`. These tools are essential for all audio and video processing.

#### FFmpeg Version Requirements

Sync-Nudger requires **FFmpeg version 4.0 or higher** to function properly. The tool specifically relies on:

* The `ebur128` filter for accurate loudness measurement (available since FFmpeg 4.0)
* Advanced audio processing capabilities
* Proper stream handling for complex media files

#### Checking Your FFmpeg Installation

**Quick Check with Sync-Nudger:**
The easiest way to verify your FFmpeg installation is to use the built-in check command:

```bash
sync-nudger --check-ffmpeg
```

This will automatically verify:

* FFmpeg and FFprobe availability
* Version compatibility (4.0+ required)
* Required filter availability (`ebur128`)

**Manual Verification:**
If you prefer to check manually:

1. **Check if FFmpeg is installed and accessible:**

   ```bash
   ffmpeg -version
   ffprobe -version
   ```

2. **Verify the version number:**
   Look for the version information in the output. You should see something like:

   ```bash
   ffmpeg version 6.1.1 Copyright (c) 2000-2023 the FFmpeg developers
   ```

   The version number (e.g., `6.1.1`) should be 4.0 or higher.

3. **Test the required filter:**
   You can verify that the `ebur128` filter is available by running:

   ```bash
   ffmpeg -hide_banner -filters | grep ebur128
   ```

   This should return a line containing `ebur128` if the filter is available.

#### Installing FFmpeg

If you don't have FFmpeg installed or need to upgrade:

* **macOS (using Homebrew):**

  ```bash
  brew install ffmpeg
  ```

* **Ubuntu/Debian:**

  ```bash
  sudo apt update
  sudo apt install ffmpeg
  ```

* **Windows:**
  Download from the [official FFmpeg website](https://ffmpeg.org/download.html)

* **Other distributions:**
  Check your package manager or visit the [FFmpeg download page](https://ffmpeg.org/download.html)

#### Bypassing Version Checks

If you're confident your FFmpeg installation will work despite version warnings, you can bypass the version check using:

```bash
sync-nudger --ignore-ffmpeg-version [other options...]
```

**Note:** Using an incompatible FFmpeg version may result in runtime errors or unexpected behavior.

### From Releases

You can download the latest pre-compiled binary for your operating system (Windows, Linux, macOS) from the [**GitHub Releases**](https://github.com/sahmad/sync-nudger/releases) page.

1. Download the appropriate archive (`.zip` for Windows, `.tar.gz` for Linux/macOS).
2. Extract the `sync-nudger` (or `sync-nudger.exe`) executable.
3. Place it in a directory that is included in your system's `PATH`.

## Usage

Here is an example of a typical command:

```sh
sync-nudger \
    --input "my_video.mkv" \
    --output "my_video_synced.mkv" \
    --stream 6 \
    --initial-delay -50 \
    --split 177.3:360 \
    --split-range 850.5:855.1:360 \
    --bitrate 128k
```

This command will:

* Process the audio stream with index `6` from `my_video.mkv`.
* Trim `50ms` from the beginning of the audio.
* Create a split at exactly `177.3` seconds and apply a `360ms` delay to the following segment.
* Find the quietest point between `850.5s` and `855.1s`, create a split there, and apply a `360ms` delay to the final segment.
* Re-encode the final audio to a bitrate of `128k`.
* Save the result to `my_video_synced.mkv`.

## License

This project is licensed under the MIT License. See the [LICENSE](LICENSE) file for details.
