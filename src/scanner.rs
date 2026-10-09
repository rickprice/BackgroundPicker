//! Directory walking and image discovery.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use walkdir::WalkDir;

const IMAGE_EXTENSIONS: &[&str] = &["jpg", "jpeg", "png", "gif", "bmp", "webp"];

#[must_use]
pub fn is_image_file(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext_str| {
            IMAGE_EXTENSIONS
                .iter()
                .any(|&valid_ext| valid_ext.eq_ignore_ascii_case(ext_str))
        })
}

#[derive(Clone)]
pub struct ImageInfo {
    pub path: PathBuf,
    pub thumbnail: Option<egui::TextureHandle>,
    pub relative_path: String,
    pub loading: bool,
}

/// Images found under a scanned directory, grouped by folder (sorted alphabetically
/// for stable, predictable UI ordering).
pub struct ScanResult {
    pub images: Vec<ImageInfo>,
    pub folder_tree: BTreeMap<String, Vec<usize>>,
}

/// Recursively walks `base_path`, collecting every image file into a flat list
/// indexed by folder for display.
#[must_use]
pub fn scan(base_path: &Path) -> ScanResult {
    let mut images = Vec::new();
    let mut folder_tree: BTreeMap<String, Vec<usize>> = BTreeMap::new();

    let entries = WalkDir::new(base_path)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|e| e.file_type().is_file() && is_image_file(e.path()));

    for entry in entries {
        let relative_path = entry.path().strip_prefix(base_path).map_or_else(
            |_| entry.path().to_string_lossy().into_owned(),
            |p| p.to_string_lossy().into_owned(),
        );

        let folder = entry
            .path()
            .parent()
            .and_then(|p| p.strip_prefix(base_path).ok())
            .map_or_else(|| ".".to_owned(), |p| p.to_string_lossy().into_owned());

        let image_index = images.len();
        images.push(ImageInfo {
            path: entry.path().to_path_buf(),
            thumbnail: None,
            relative_path,
            loading: false,
        });

        folder_tree.entry(folder).or_default().push(image_index);
    }

    ScanResult {
        images,
        folder_tree,
    }
}
