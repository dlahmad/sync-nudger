# Release Notes

## Features Added

* Find the quietest split point within a given time range (`--split-range`).
* User confirmation prompt with a detailed summary table before processing.
* Configuration for silence detection threshold (`--silence-threshold`).
* `ffmpeg` version check on startup (`--ignore-ffmpeg-version` to bypass).
* Debug flag for verbose `ffmpeg` logs (`--debug`).
* Automated multi-platform builds and releases via GitHub Actions.

**Important:** This tool requires `ffmpeg` and `ffprobe` to be installed and available in your system's PATH.
