# Background Picker

A Rust GUI application for scanning directories and selecting desktop backgrounds from image thumbnails.

## Features

- Scans configurable directory and all subdirectories for image files
- Displays images as thumbnails in folder hierarchy
- Configurable thumbnail size
- Configurable background setting command (defaults to `feh --bg-scale`)
- Saves state between runs in YAML format
- Supports common image formats: JPG, JPEG, PNG, GIF, BMP, WebP

## Usage

```bash
# Run with default settings (scans current directory)
cargo run

# Specify custom directory and options
cargo run -- -d /path/to/images -t 200 -c "nitrogen --set-scaled" -s my-state.yaml

# Or build and run binary
cargo build --release
./target/release/background-picker --help
```

### Command Line Options

- `-d, --directory <DIR>`: Directory to scan for images (default: current directory)
- `-t, --thumbnail-size <SIZE>`: Thumbnail size in pixels (default: 150)
- `-c, --command <CMD>`: Command to set background (default: "feh --bg-scale")
- `-s, --state-file <FILE>`: State file path (default: "background-picker-state.yaml")

## Controls

- Click on any thumbnail to set it as desktop background
- Folders are collapsible to organize images by directory structure
- Hover over thumbnails to see full file path
- Application remembers window size and last selected image

## Dependencies

Built with:
- `egui` - Immediate mode GUI framework
- `eframe` - Application framework
- `image` - Image processing
- `clap` - Command line argument parsing
- `serde_yaml` - State persistence
- `walkdir` - Directory traversal

## Requirements

- Rust toolchain
- A background setting utility like `feh`, `nitrogen`, or similar
