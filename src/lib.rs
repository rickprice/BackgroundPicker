mod app;
pub mod cache;
mod cli;
mod error;
pub mod scanner;
pub mod thumbnail;

pub use app::BackgroundPickerApp;
pub use cli::Args;
pub use error::{BackgroundPickerError, Result};
pub use scanner::{ImageInfo, is_image_file};

/// # Errors
///
/// Returns an error if `command` is empty or contains only whitespace.
pub fn validate_command(command: &str) -> Result<()> {
    if command.trim().is_empty() {
        return Err(BackgroundPickerError::CommandExecution(
            "Empty command".to_owned(),
        ));
    }
    Ok(())
}
