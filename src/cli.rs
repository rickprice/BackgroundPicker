use std::path::PathBuf;

use clap::Parser;

#[derive(Parser, Clone)]
#[command(name = "background-picker")]
#[command(version)]
#[command(about = "A GUI tool for selecting desktop backgrounds")]
pub struct Args {
    #[arg(short, long, default_value = ".")]
    pub directory: PathBuf,

    #[arg(short, long, default_value = "150")]
    pub thumbnail_size: u32,

    #[arg(short, long, default_value = "feh --bg-max")]
    pub command: String,

    #[arg(short, long, default_value = "selected-background.txt")]
    pub selected_image_file: PathBuf,

    #[arg(long, help = "Enable debug output")]
    pub debug: bool,

    #[arg(long, help = "Pre-generate all thumbnails and exit (don't show GUI)")]
    pub pregenerate: bool,
}
