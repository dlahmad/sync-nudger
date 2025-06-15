# Sync-Nudger: Advanced Audio Splitting Tool

[![Deploy](https://github.com/dlahmad/sync-nudger/actions/workflows/release.yml/badge.svg)](https://github.com/dlahmad/sync-nudger/actions/workflows/release.yml)

Sync-Nudger is a command-line utility designed for precise audio stream manipulation within video files. It allows you to split an audio track at specific timestamps, apply individual delays to each new segment (including fractional milliseconds), and then seamlessly remux the modified audio back into the original container.

Its standout feature is the ability to find the quietest point within a given time range, ensuring that your splits are clean and occur during natural pauses in the audio.

## What it does

The tool performs a series of operations to achieve its goal:

1. **Extracts** the target audio stream into a temporary, high-quality FLAC file.
2. **Analyzes** user-specified time ranges to find the quietest moment, using the EBU R 128 loudness standard for perceptual accuracy.
3. **Resolves** all split points, whether provided as exact timestamps or as search ranges.
4. **Asks for Confirmation** by presenting a detailed summary of the proposed changes before proceeding (can be auto-confirmed with `--yes`).
5. **Splits** the audio into multiple parts based on the resolved points.
6. **Applies** the specified millisecond delays (including fractional milliseconds) to each part (or trims them if the delay is negative).
7. **Concatenates** the modified audio parts back into a single stream.
8. **Re-encodes** the audio to its original format and bitrate.
9. **Remuxes** the new audio stream back into the video file, replacing the original while keeping all other video, audio, and subtitle streams intact.

## Features

* **Audio Stream Inspection**: View detailed information about all audio streams in a file before processing (`--inspect`).
* **Precise Splitting**: Split audio at exact floating-point timestamps.
* **Quiet Point Detection**: Automatically find the quietest split point within a given time range (`--split-range`).
* **Per-Segment Delay**: Apply a unique delay in milliseconds (including fractional) to each audio segment, including the initial one.
* **User Confirmation**: Displays a detailed summary of the files, streams, and planned splits before executing, preventing accidental changes. Use `--yes` to auto-confirm.
* **Configurable Silence Detection**: Tune the loudness threshold for what the tool considers "audible" vs. silent (`--silence-threshold`).
* **FFmpeg Version Check**: Ensures a compatible version of `ffmpeg` is installed to prevent runtime errors. Can be bypassed (`--ignore-ffmpeg-version`).
* **Debug Logging**: Optional verbose logging from `ffmpeg` for troubleshooting (`--debug`).
* **Automated Releases**: Multi-platform binaries are built automatically via GitHub Actions.
* **Split Map Support**: Use a JSON file to specify all splits, split ranges, and delays, or save your configuration for reproducibility (`--split-map`, `--write-split-map`).
* **Fit Length**: Fit the edited audio stream to the original length (trim or pad with silence at the end as needed) (`--fit-length`).

## Installation

### Quick Install (Linux/macOS)

You can install the latest release of Sync-Nudger with a single command:

```sh
curl -fsSL https://raw.githubusercontent.com/dlahmad/sync-nudger/refs/heads/master/install.sh | bash
```

This script automatically detects your OS and architecture, downloads the latest release from GitHub, and installs the binary to `/usr/local/bin`.

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

### Processing Audio

Here is an example of a typical command:

```sh
sync-nudger \
    --input "my_video.mkv" \
    --output "my_video_synced.mkv" \
    --stream 6 \
    --initial-delay -50 \
    --split 177.3:360.5 \
    --split-range 850.5:855.1:360.25 \
    --bitrate 128k \
    --yes
```

#### Full CLI Options

| Short | Long                | Description                                                                                 |
|-------|---------------------|---------------------------------------------------------------------------------------------|
| -i    | --input             | Input MKV file                                                                              |
| -o    | --output            | Output MKV file                                                                             |
| -s    | --stream            | Audio stream index (e.g. 6)                                                                  |
| -t    | --task              | Path to a JSON file describing the full task (input, output, stream, splits, delays, etc).   |
| -d    | --initial-delay     | Delay for the first audio segment in milliseconds (can be fractional, e.g., 200.5)           |
| -p    | --split             | Split points and subsequent delays, in format <seconds>:<delay_ms>                           |
| -r    | --split-range       | Split ranges and subsequent delays, in format <start_time>:<end_time>:<delay_ms>             |
| -b    | --bitrate           | Output bitrate (e.g. 80k). If not provided, it will be detected automatically.               |
| -T    | --silence-threshold | Loudness threshold (in LUFS) to consider a point as audible. Default: -95.0                  |
| -g    | --debug             | Show ffmpeg logs                                                                             |
|       | --ignore-ffmpeg-version | Ignore ffmpeg version check                                                             |
| -c    | --check-ffmpeg      | Check FFmpeg installation and version compatibility                                          |
| -I    | --inspect           | Inspect input file and show all audio streams in a table                                     |
| -w    | --write-split-map   | Write the resolved split map to this file as JSON                                            |
| -y    | --yes               | Automatically confirm the splitting plan and proceed without prompting                       |
| -F    | --fit-length        | Fit the edited audio stream to the original length (trim or pad with silence at the end as needed) |

### Using a Task JSON File

You can provide all split points, split ranges, and the initial delay in a single JSON file using the `--task` flag. CLI arguments override values in the task file.

**Note:** Task files do **not** need to contain all parameters. You can include only the fields you want to specify; any missing fields will use their default values or can be provided/overridden via CLI arguments. This allows for minimal or partial task files.

**Example JSON file (`task.json`):**

```json
{
  "input": "my_video.mkv",
  "output": "my_video_synced.mkv",
  "stream": 6,
  "initial_delay": -50.0,
  "splits": [
    { "time": 177.3, "delay": 360.5 }
  ],
  "split_ranges": [
    { "startTime": 850.5, "endTime": 855.1, "delay": 360.25 }
  ],
  "bitrate": "128k",
  "silence_threshold": -95.0,
  "fit_length": true
}
```

You can run:

```sh
sync-nudger -t task.json -y
```
