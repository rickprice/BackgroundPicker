# Background Picker

A fast, efficient Rust GUI application for scanning directories and selecting desktop backgrounds from image thumbnails. Features intelligent caching and seamless integration with file manager thumbnails.

## Features

### 🖼️ **Smart Image Management**
- Recursively scans directories for image files
- Displays images as thumbnails in collapsible folder hierarchy
- Supports common formats: JPG, JPEG, PNG, GIF, BMP, WebP
- Configurable thumbnail sizes

### ⚡ **Performance & Caching**
- **Intelligent thumbnail caching** using freedesktop.org standard
- **Shares cache with pcmanfm, nautilus, and other file managers**
- **Parallel thumbnail generation** for maximum speed
- **Instant loading** for previously cached images
- **Progressive loading** with visual feedback

### 🚀 **Flexible Operation Modes**
- **GUI Mode** (default): Interactive thumbnail browser
- **Pregenerate Mode** (`--pregenerate`): Batch cache generation and exit
- **Debug Mode** (`--debug`): Detailed operation logging

### 🎛️ **Configurable**
- Customizable background setting command (defaults to `feh --bg-max`)
- Persistent state saving (window size, last selection)
- All settings configurable via command line

## Installation

```bash
# Clone and build
git clone <repository-url>
cd BackgroundPicker
cargo build --release

# Or run directly
cargo run -- --help
```

## Usage

### Interactive Mode (Default)
```bash
# Browse current directory
cargo run

# Browse specific directory with custom settings
cargo run -- -d ~/Pictures -t 200 -c "nitrogen --set-scaled"

# Built binary
./target/release/background-picker -d ~/Pictures
```

### Batch Cache Generation
```bash
# Pre-generate thumbnails for faster browsing later
cargo run -- --pregenerate -d ~/Pictures

# With detailed progress output
cargo run -- --pregenerate --debug -d ~/Pictures
```

### Two-Step Workflow (Recommended for Large Collections)
```bash
# Step 1: Generate all thumbnails (runs in background)
background-picker --pregenerate -d ~/Pictures

# Step 2: Browse with instant thumbnails
background-picker -d ~/Pictures
```

## Command Line Options

| Option | Short | Default | Description |
|--------|-------|---------|-------------|
| `--directory` | `-d` | `.` | Directory to scan for images |
| `--thumbnail-size` | `-t` | `150` | Thumbnail size in pixels |
| `--command` | `-c` | `"feh --bg-max"` | Command to set background |
| `--state-file` | `-s` | `background-picker-state.yaml` | State file path |
| `--debug` | | | Enable detailed debug output |
| `--pregenerate` | | | Generate thumbnails and exit (no GUI) |
| `--help` | `-h` | | Show help information |

## Background Setting Commands

The application supports any command-line wallpaper tool:

```bash
# feh (default - maintains aspect ratio)
-c "feh --bg-max"

# feh (fills screen, may crop)
-c "feh --bg-scale"

# nitrogen
-c "nitrogen --set-scaled"

# Custom script
-c "/path/to/set-wallpaper.sh"
```

## User Interface

- **Click thumbnail**: Set as desktop background and exit application
- **Expand folders**: Click folder names to show/hide thumbnails
- **Hover tooltips**: See full file paths
- **Progress indicators**: Visual feedback during thumbnail loading
- **Responsive design**: Handles collections of any size

## Performance Features

### Thumbnail Caching
- Uses standard `~/.cache/thumbnails/` directory
- **Compatible with pcmanfm, nautilus, thunar, and other file managers**
- Automatic cache validation (regenerates if file modified)
- Supports both normal (128x128) and large (256x256) thumbnail sizes

### Parallel Processing
- Multi-threaded thumbnail generation using Rayon
- Efficient batch processing for large collections
- Non-blocking UI updates
- Smart memory management

### Cache Integration
```bash
# Browse images in file manager first (creates thumbnails)
pcmanfm ~/Pictures

# Then use background picker (instant thumbnails!)
background-picker -d ~/Pictures
```

## Dependencies

Built with modern Rust ecosystem:
- **egui/eframe** - Immediate mode GUI framework
- **image** - Fast image processing and format support
- **rayon** - Data parallelism for thumbnail generation
- **clap** - Command line argument parsing
- **serde/serde_yaml** - Configuration and state persistence
- **walkdir** - Efficient directory traversal
- **dirs** - Cross-platform directory locations
- **sha1** - Thumbnail cache key generation

## Requirements

- **Rust toolchain** (2021 edition)
- **Wallpaper utility**: `feh`, `nitrogen`, or custom command
- **Linux/Unix** environment (uses freedesktop.org standards)

## Examples

### Basic Usage
```bash
# Quick browse and select
background-picker

# Browse specific folder
background-picker -d ~/Wallpapers
```

### Large Collections
```bash
# Pre-generate cache for 1000+ images
background-picker --pregenerate -d ~/Pictures

# Later: instant browsing
background-picker -d ~/Pictures
```

### Custom Workflows
```bash
# Use with different wallpaper tools
background-picker -c "nitrogen --set-zoom-fill" -d ~/Backgrounds

# Custom thumbnail size for high-DPI displays
background-picker -t 300 -d ~/Pictures

# Debug thumbnail cache behavior
background-picker --debug -d ~/Pictures
```

### Scripting
```bash
#!/bin/bash
# Batch generate thumbnails for multiple directories
for dir in ~/Pictures/*/; do
    echo "Processing $dir..."
    background-picker --pregenerate -d "$dir"
done
```

The application automatically quits after setting a background, making it perfect for quick wallpaper changes and integration with desktop shortcuts or scripts.
