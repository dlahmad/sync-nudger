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

* **Audio Stream Inspection**: View detailed information about all audio streams in a file before processing (`--inspect`).
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

### Inspecting Audio Streams

Before processing, you can inspect the available audio streams in your file:

```bash
sync-nudger --input input.mkv --inspect
```

This will display a table showing all audio streams with their properties:

```bash
┌───────┬─────────┬──────────┬─────────────┬─────────┬──────────┬─────────────────────┐
│ Index │ Codec   │ Channels │ Sample Rate │ Bitrate │ Language │ Title               │
├───────┼─────────┼──────────┼─────────────┼─────────┼──────────┼─────────────────────┤
│ 1     │ aac     │ 2        │ 48000 Hz    │ 128 kbps│ eng      │ English Audio       │
│ 2     │ ac3     │ 6        │ 48000 Hz    │ 640 kbps│ eng      │ English Surround    │
│ 3     │ dts     │ 8        │ 48000 Hz    │ 153 kbps│ eng      │ DTS-HD Master Audio │
└───────┴─────────┴──────────┴─────────────┴─────────┴──────────┴─────────────────────┘

💡 Use the 'Index' value with --stream to select an audio stream for processing.
```

### Using Split and Delay Arguments

You can use `--split`, `--split-range`, and `--initial-delay` together in any combination to define your split points and delays. For example:

```sh
sync-nudger \
    --input "my_video.mkv" \
    --output "my_video_synced.mkv" \
    --stream 6 \
    --split 177.3:360 \
    --split-range 850.5:855.1:360 \
    --initial-delay -50
```

### Using a Split Map JSON File

You can provide all split points, split ranges, and the initial delay in a single JSON file using the `--split-map` flag. **This is exclusive:** you cannot use `--split-map` together with `--split`, `--split-range`, or `--initial-delay`.

**Example JSON file (`splits.json`):**

```json
{
  "initial_delay": -50,
  "splits": [[177.3, 360]],
  "split_ranges": [
    { "startTime": 850.5, "endTime": 855.1, "delay": 360 }
  ]
}
```

**Usage:**

```sh
sync-nudger \
    --input "my_video.mkv" \
    --output "my_video_synced.mkv" \
    --stream 6 \
    --split-map splits.json
```

### Saving a Split Map for Reproducibility

You can save the resolved split points and delays to a JSON file using the `--write-split-map` option. This allows you to reproduce the same operation later using `--split-map`.

* If you use `--write-split-map` **with a filename**, that file will be used.
* If you use `--write-split-map` **as a flag with no filename**, the output file will be the input file name (without extension) plus `.json`.

**What gets saved:**

* If you provide `--split`, only those split points will be saved in the JSON.
* If you provide `--split-range`, only the original ranges will be saved in the JSON (not the derived split points).
* If you provide both, both will be present in the JSON.
* The split map is a faithful record of your input arguments, not the resolved plan.

**Examples:**

*Only split points:*

```json
{
  "initial_delay": 0,
  "splits": [
    { "time": 100.5, "delay": 200 },
    { "time": 300.25, "delay": 400 }
  ],
  "split_ranges": []
}
```

*Only split ranges:*

```json
{
  "initial_delay": 0,
  "splits": [],
  "split_ranges": [
    { "startTime": 100.75, "endTime": 110.5, "delay": 200 },
    { "startTime": 825.1, "endTime": 832.9, "delay": 400 }
  ]
}
```

*Both:*

```json
{
  "initial_delay": 0,
  "splits": [
    { "time": 100.5, "delay": 200 }
  ],
  "split_ranges": [
    { "startTime": 825.1, "endTime": 832.9, "delay": 400 }
  ]
}
```

Note: Fractions of seconds are supported for all time fields (`time`, `startTime`, `endTime`).

Later, you can reproduce the same operation with:

```