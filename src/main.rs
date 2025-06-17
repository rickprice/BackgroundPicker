use clap::Parser;
use background_picker::{Args, BackgroundPickerApp};

const DEFAULT_WINDOW_WIDTH: f32 = 800.0;
const DEFAULT_WINDOW_HEIGHT: f32 = 600.0;

fn main() -> anyhow::Result<()> {
    let args = Args::parse();
    
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([DEFAULT_WINDOW_WIDTH, DEFAULT_WINDOW_HEIGHT])
            .with_title("Background Picker"),
        ..Default::default()
    };
    
    let result = eframe::run_native(
        "Background Picker",
        options,
        Box::new(|cc| Ok(Box::new(BackgroundPickerApp::new(cc, args)))),
    );
    
    match result {
        Ok(_) => Ok(()),
        Err(e) => Err(anyhow::anyhow!("Failed to run application: {:?}", e)),
    }
}