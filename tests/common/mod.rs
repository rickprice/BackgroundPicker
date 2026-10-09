#![allow(dead_code)] // Not every test binary uses every helper.

use std::path::Path;

use background_picker::{Args, BackgroundPickerApp, ImageInfo};
use image::{DynamicImage, ImageBuffer, RgbImage};

pub fn test_args(directory: &Path) -> Args {
    Args {
        directory: directory.to_path_buf(),
        thumbnail_size: 50,
        command: "echo".to_owned(),
        selected_image_file: directory.join("selected.txt"),
        debug: false,
        pregenerate: false,
    }
}

pub fn app_with_images(directory: &Path, images: Vec<ImageInfo>) -> BackgroundPickerApp {
    BackgroundPickerApp::with_state(test_args(directory), images, directory.join("cache")).unwrap()
}

pub fn image_info(path: &Path) -> ImageInfo {
    ImageInfo {
        path: path.to_path_buf(),
        thumbnail: None,
        relative_path: path.to_string_lossy().into_owned(),
        loading: false,
    }
}

/// Writes a real (decodable) PNG to `path`, for tests that exercise actual image decoding.
pub fn create_real_png(path: &Path, width: u32, height: u32) {
    let img: RgbImage = ImageBuffer::from_fn(width, height, |x, y| {
        image::Rgb([(x % 255) as u8, (y % 255) as u8, 128])
    });
    DynamicImage::ImageRgb8(img).save(path).unwrap();
}

pub fn fake_jpeg_data() -> Vec<u8> {
    vec![0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x10, 0x4A, 0x46, 0x49, 0x46]
}

pub fn fake_png_data() -> Vec<u8> {
    vec![0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A]
}

pub fn fake_bmp_data() -> Vec<u8> {
    vec![0x42, 0x4D]
}
