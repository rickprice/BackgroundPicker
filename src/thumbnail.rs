//! Thumbnail generation, decoding, and freedesktop-compliant cache encoding.

use std::fs::{self, File};
use std::io::BufWriter;
use std::path::Path;
use std::time::SystemTime;

use egui::ColorImage;
use image::DynamicImage;
use image::imageops::FilterType;

use crate::cache;

/// Generates a thumbnail for the image at `path`, choosing a multi-step downscale
/// for large source images to balance quality against decode/resize cost.
#[must_use]
pub fn generate(path: &Path, size: u32) -> Option<ColorImage> {
    let img = image::ImageReader::open(path)
        .ok()?
        .with_guessed_format()
        .ok()?
        .decode()
        .ok()?;
    let (width, height) = (img.width(), img.height());

    if width <= size && height <= size {
        return resize_to_color_image(&img, size);
    }

    #[allow(
        clippy::cast_precision_loss,
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss
    )]
    let scale_factor = (width.max(height) as f32 / size as f32).max(1.0);

    if scale_factor > 8.0 {
        // Very large images: coarse nearest-neighbor pass first, then a quality pass,
        // avoids running the (slower) triangle filter over the full source resolution.
        #[allow(
            clippy::cast_precision_loss,
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss
        )]
        let first_step = (size as f32 * 4.0) as u32;
        #[allow(
            clippy::cast_precision_loss,
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss
        )]
        let second_step = (size as f32 * 2.0) as u32;

        let step1 = img.resize(first_step, first_step, FilterType::Nearest);
        let step2 = step1.resize(second_step, second_step, FilterType::Triangle);
        resize_to_color_image(&step2, size)
    } else if scale_factor > 4.0 {
        let intermediate_size = size * 2;
        let intermediate = img.resize(intermediate_size, intermediate_size, FilterType::Nearest);
        resize_to_color_image(&intermediate, size)
    } else {
        resize_to_color_image(&img, size)
    }
}

/// Resizes `img` to fit within `size`x`size` and converts it to an egui texture source.
#[must_use]
pub fn resize_to_color_image(img: &DynamicImage, size: u32) -> Option<ColorImage> {
    let thumbnail = img.resize(size, size, FilterType::Nearest);
    let rgba = thumbnail.to_rgba8();
    let (width, height) = (thumbnail.width() as usize, thumbnail.height() as usize);

    let raw_pixels = rgba.as_raw();
    if raw_pixels.len() != width * height * 4 {
        return None;
    }

    Some(ColorImage::from_rgba_unmultiplied(
        [width, height],
        raw_pixels,
    ))
}

/// Loads a previously cached thumbnail PNG, resizing it if the stored size doesn't match.
#[must_use]
pub fn load_cached(cache_path: &Path, target_size: u32) -> Option<ColorImage> {
    let reader = image::ImageReader::open(cache_path).ok()?;
    let img = reader.with_guessed_format().ok()?.decode().ok()?;

    let resized = if img.width() == target_size && img.height() == target_size {
        img
    } else {
        img.resize(target_size, target_size, FilterType::Nearest)
    };
    resize_to_color_image(&resized, target_size)
}

/// Encodes `color_image` as a freedesktop-spec-compliant PNG and writes it to `cache_path`.
#[allow(clippy::cast_possible_truncation)] // thumbnail dimensions never approach u32::MAX
pub fn save_to_cache(color_image: &ColorImage, cache_path: &Path, original_path: &Path) {
    let [width, height] = color_image.size;
    let pixels: Vec<u8> = color_image
        .pixels
        .iter()
        .flat_map(egui::Color32::to_array)
        .collect();

    let Some(img_buffer) = image::RgbaImage::from_raw(width as u32, height as u32, pixels) else {
        return;
    };

    if let Some(parent) = cache_path.parent() {
        if let Err(e) = fs::create_dir_all(parent) {
            eprintln!(
                "Failed to create thumbnail cache directory {}: {e}",
                parent.display()
            );
            return;
        }
    }

    save_with_metadata(
        &DynamicImage::ImageRgba8(img_buffer),
        cache_path,
        original_path,
    );
}

/// Writes `img` to `cache_path` as a PNG carrying the freedesktop.org thumbnail
/// `Thumb::URI` and `Thumb::MTime` tEXt chunks, so other spec-compliant tools
/// (file managers, etc.) recognize and reuse it.
pub fn save_with_metadata(img: &DynamicImage, cache_path: &Path, original_path: &Path) {
    let canonical_original =
        fs::canonicalize(original_path).unwrap_or_else(|_| original_path.to_path_buf());
    let file_uri = format!("file://{}", canonical_original.to_string_lossy());

    let mtime_secs = fs::metadata(original_path)
        .and_then(|m| m.modified())
        .map_or(0, |t| {
            t.duration_since(SystemTime::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs()
        });

    let Ok(file) = File::create(cache_path) else {
        eprintln!(
            "Failed to create thumbnail cache file: {}",
            cache_path.display()
        );
        return;
    };

    let rgb = img.to_rgb8();
    let mut encoder = png::Encoder::new(BufWriter::new(file), img.width(), img.height());
    encoder.set_color(png::ColorType::Rgb);
    encoder.set_depth(png::BitDepth::Eight);
    let _ = encoder.add_text_chunk("Thumb::URI".to_owned(), file_uri);
    let _ = encoder.add_text_chunk("Thumb::MTime".to_owned(), mtime_secs.to_string());

    let result = encoder
        .write_header()
        .and_then(|mut writer| writer.write_image_data(rgb.as_raw()));

    if let Err(e) = result {
        eprintln!(
            "Failed to save thumbnail for {}: {e}",
            original_path.display()
        );
    }
}

/// Loads a thumbnail for `path`, preferring an existing freedesktop-cached thumbnail
/// (ours or another application's), falling back to generating and caching a new one.
#[must_use]
pub fn load_or_generate(
    path: &Path,
    size: u32,
    cache_dir: &Path,
    debug: bool,
) -> Option<ColorImage> {
    let abs_path = fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());

    if let Some(existing) = cache::find_existing_thumbnail(&abs_path) {
        if let Some(cached_image) = load_cached(&existing, size) {
            if debug {
                println!(
                    "Loaded existing system thumbnail for {:?}",
                    path.file_name().unwrap_or_default()
                );
            }
            return Some(cached_image);
        }
    }

    let cache_path = cache::cached_thumbnail_path(&abs_path, cache_dir);
    if cache::is_cache_valid(&abs_path, &cache_path) {
        if let Some(cached_image) = load_cached(&cache_path, size) {
            if debug {
                println!(
                    "Loaded our cached thumbnail for {:?}",
                    path.file_name().unwrap_or_default()
                );
            }
            return Some(cached_image);
        }
    }

    if debug {
        println!(
            "Generating new thumbnail for {:?}",
            path.file_name().unwrap_or_default()
        );
    }
    let color_image = generate(&abs_path, size)?;
    save_to_cache(&color_image, &cache_path, &abs_path);
    Some(color_image)
}
