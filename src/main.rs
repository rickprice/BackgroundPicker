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
        Box::new(|cc| {
            BackgroundPickerApp::new(cc, args)
                .map(|app| Box::new(app) as Box<dyn eframe::App>)
                .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)
        }),
    );
    
    match result {
        Ok(_) => Ok(()),
        Err(e) => Err(anyhow::anyhow!("Failed to run application: {:?}", e)),
    }
}