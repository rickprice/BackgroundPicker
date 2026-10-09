//! Freedesktop.org thumbnail cache directory, hashing, and validity rules.
//! See <https://specifications.freedesktop.org/thumbnail-spec/>.

use std::fmt::Write;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use sha1::{Digest, Sha1};

use crate::error::{BackgroundPickerError, Result};

/// Returns (creating it if necessary) the freedesktop "normal" thumbnail cache directory.
///
/// # Errors
///
/// Returns an error if the directory cannot be created.
pub fn thumbnail_cache_dir() -> Result<PathBuf> {
    let cache_home = dirs::cache_dir()
        .or_else(|| dirs::home_dir().map(|h| h.join(".cache")))
        .unwrap_or_else(|| PathBuf::from(".cache"));

    let normal_dir = cache_home.join("thumbnails").join("normal");
    fs::create_dir_all(&normal_dir).map_err(BackgroundPickerError::CacheDirectoryCreation)?;

    Ok(normal_dir)
}

/// Computes the SHA1 hash of the file's `file://` URI, matching the hash used by
/// pcmanfm and other freedesktop-compliant file managers for thumbnail cache keys.
#[must_use]
pub fn thumbnail_hash(file_path: &Path) -> String {
    let canonicalized = fs::canonicalize(file_path).unwrap_or_else(|_| file_path.to_path_buf());
    let file_uri = format!("file://{}", canonicalized.to_string_lossy());

    let mut hasher = Sha1::new();
    hasher.update(file_uri.as_bytes());

    hasher
        .finalize()
        .iter()
        .fold(String::with_capacity(40), |mut hex, byte| {
            let _ = write!(hex, "{byte:02x}");
            hex
        })
}

/// The path this app would use to cache a thumbnail for `file_path` inside `cache_dir`.
#[must_use]
pub fn cached_thumbnail_path(file_path: &Path, cache_dir: &Path) -> PathBuf {
    cache_dir.join(format!("{}.png", thumbnail_hash(file_path)))
}

/// A cached thumbnail is valid as long as it exists and is not older than the source file.
#[must_use]
pub fn is_cache_valid(original_path: &Path, cache_path: &Path) -> bool {
    if !cache_path.exists() {
        return false;
    }

    let original_modified = fs::metadata(original_path)
        .and_then(|m| m.modified())
        .unwrap_or(SystemTime::UNIX_EPOCH);

    let cache_modified = fs::metadata(cache_path)
        .and_then(|m| m.modified())
        .unwrap_or(SystemTime::UNIX_EPOCH);

    cache_modified >= original_modified
}

/// Looks for a thumbnail already created by another application (file manager, etc.),
/// checking freedesktop-standard cache sizes in order of preference.
#[must_use]
pub fn find_existing_thumbnail(file_path: &Path) -> Option<PathBuf> {
    let thumbnails_dir = dirs::cache_dir()?.join("thumbnails");
    let thumbnail_name = format!("{}.png", thumbnail_hash(file_path));

    ["normal", "large"].into_iter().find_map(|size_dir| {
        let thumbnail_path = thumbnails_dir.join(size_dir).join(&thumbnail_name);
        (thumbnail_path.exists() && is_cache_valid(file_path, &thumbnail_path))
            .then_some(thumbnail_path)
    })
}
