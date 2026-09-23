use background_picker::{Args, BackgroundPickerApp, is_image_file, validate_command};
use clap::Parser;
use std::path::PathBuf;
use std::sync::{Arc, RwLock};
use std::fs::{self, File};
use tempfile::TempDir;
use serial_test::serial;

#[cfg(test)]
mod cli_args_tests {
    use super::*;

    #[test]
    fn test_args_default_values() {
        let args = Args::try_parse_from(["background-picker"]).unwrap();
        
        assert_eq!(args.directory, PathBuf::from("."));
        assert_eq!(args.thumbnail_size, 150);
        assert_eq!(args.command, "feh --bg-max");
        assert_eq!(args.selected_image_file, PathBuf::from("selected-background.txt"));
        assert!(!args.debug);
        assert!(!args.pregenerate);
    }

    #[test]
    fn test_args_custom_values() {
        let args = Args::try_parse_from([
            "background-picker",
            "--directory", "/home/user/pictures",
            "--thumbnail-size", "200",
            "--command", "gsettings set org.gnome.desktop.background picture-uri",
            "--selected-image-file", "custom-selected.txt",
            "--debug",
            "--pregenerate"
        ]).unwrap();
        
        assert_eq!(args.directory, PathBuf::from("/home/user/pictures"));
        assert_eq!(args.thumbnail_size, 200);
        assert_eq!(args.command, "gsettings set org.gnome.desktop.background picture-uri");
        assert_eq!(args.selected_image_file, PathBuf::from("custom-selected.txt"));
        assert!(args.debug);
        assert!(args.pregenerate);
    }

    #[test]
    fn test_args_short_flags() {
        let args = Args::try_parse_from([
            "background-picker",
            "-d", "/tmp",
            "-t", "100",
            "-c", "echo",
            "-s", "selected.txt"
        ]).unwrap();
        
        assert_eq!(args.directory, PathBuf::from("/tmp"));
        assert_eq!(args.thumbnail_size, 100);
        assert_eq!(args.command, "echo");
        assert_eq!(args.selected_image_file, PathBuf::from("selected.txt"));
    }

    #[test]
    fn test_args_invalid_thumbnail_size() {
        let result = Args::try_parse_from([
            "background-picker",
            "--thumbnail-size", "not_a_number"
        ]);
        
        assert!(result.is_err());
    }
}

#[cfg(test)]
mod selected_image_tests {
    use super::*;
    use std::fs;

    #[test]
    #[serial]
    fn test_save_selected_image_creates_file() {
        let temp_dir = TempDir::new().unwrap();
        let selected_file = temp_dir.path().join("selected.txt");
        
        let args = Args {
            directory: PathBuf::from("."),
            thumbnail_size: 150,
            command: "echo".to_string(),
            selected_image_file: selected_file.clone(),
            debug: false,
            pregenerate: false,
        };
        
        // Create a minimal app for testing
        let app = BackgroundPickerApp {
            args,
            images: Arc::new(RwLock::new(Vec::new())),
            folder_tree: std::collections::HashMap::new(),
            loading: false,
            thumbnail_sender: std::sync::mpsc::channel().0,
            thumbnail_receiver: std::sync::mpsc::channel().1,
            thread_pool: rayon::ThreadPoolBuilder::new().num_threads(1).build().unwrap(),
            cache_dir: temp_dir.path().to_path_buf(),
        };
        
        let test_path = PathBuf::from("/path/to/test/image.jpg");
        let result = app.save_selected_image(&test_path);
        
        assert!(result.is_ok());
        assert!(selected_file.exists());
        
        let content = fs::read_to_string(&selected_file).unwrap();
        assert_eq!(content.trim(), test_path.to_string_lossy());
    }

    #[test]
    #[serial]
    fn test_save_selected_image_creates_directory() {
        let temp_dir = TempDir::new().unwrap();
        let nested_dir = temp_dir.path().join("nested").join("dir");
        let selected_file = nested_dir.join("selected.txt");
        
        let args = Args {
            directory: PathBuf::from("."),
            thumbnail_size: 150,
            command: "echo".to_string(),
            selected_image_file: selected_file.clone(),
            debug: false,
            pregenerate: false,
        };
        
        // Create a minimal app for testing
        let app = BackgroundPickerApp {
            args,
            images: Arc::new(RwLock::new(Vec::new())),
            folder_tree: std::collections::HashMap::new(),
            loading: false,
            thumbnail_sender: std::sync::mpsc::channel().0,
            thumbnail_receiver: std::sync::mpsc::channel().1,
            thread_pool: rayon::ThreadPoolBuilder::new().num_threads(1).build().unwrap(),
            cache_dir: temp_dir.path().to_path_buf(),
        };
        
        let test_path = PathBuf::from("/path/to/image.jpg");
        let result = app.save_selected_image(&test_path);
        
        assert!(result.is_ok());
        assert!(nested_dir.exists());
        assert!(selected_file.exists());
        
        let content = fs::read_to_string(&selected_file).unwrap();
        assert_eq!(content.trim(), test_path.to_string_lossy());
    }
}

#[cfg(test)]
mod utility_tests {
    use super::*;

    #[test]
    fn test_is_image_file_valid_extensions() {
        let test_cases = vec![
            ("image.jpg", true),
            ("image.jpeg", true),
            ("image.png", true),
            ("image.gif", true),
            ("image.bmp", true),
            ("image.webp", true),
            ("IMAGE.JPG", true), // Test case insensitive
            ("IMAGE.PNG", true),
        ];
        
        for (filename, expected) in test_cases {
            let path = PathBuf::from(filename);
            assert_eq!(is_image_file(&path), expected, "Failed for {}", filename);
        }
    }

    #[test]
    fn test_is_image_file_invalid_extensions() {
        let test_cases = vec![
            ("document.txt", false),
            ("video.mp4", false),
            ("audio.mp3", false),
            ("archive.zip", false),
            ("no_extension", false),
            ("image.", false), // Empty extension
        ];
        
        for (filename, expected) in test_cases {
            let path = PathBuf::from(filename);
            assert_eq!(is_image_file(&path), expected, "Failed for {}", filename);
        }
    }

    #[test]
    fn test_validate_command_valid() {
        let valid_commands = vec![
            "feh --bg-max",
            "gsettings set org.gnome.desktop.background picture-uri",
            "nitrogen --set-zoom-fill",
            "single_command",
        ];
        
        for command in valid_commands {
            assert!(validate_command(command).is_ok(), "Command should be valid: {}", command);
        }
    }

    #[test]
    fn test_validate_command_empty() {
        let invalid_commands = vec![
            "",
            "   ", // Only whitespace
        ];
        
        for command in invalid_commands {
            assert!(validate_command(command).is_err(), "Command should be invalid: '{}'", command);
        }
    }
}

#[cfg(test)]
mod thumbnail_hash_tests {
    use super::*;
    use std::fs::File;
    use std::io::Write;

    #[test]
    #[serial]
    fn test_get_thumbnail_hash() {
        let temp_dir = TempDir::new().unwrap();
        let test_file = temp_dir.path().join("test_image.jpg");
        
        // Create a test file
        let mut file = File::create(&test_file).unwrap();
        file.write_all(b"fake image data").unwrap();
        
        let hash1 = BackgroundPickerApp::get_thumbnail_hash(&test_file);
        let hash2 = BackgroundPickerApp::get_thumbnail_hash(&test_file);
        
        assert!(hash1.is_some());
        assert!(hash2.is_some());
        assert_eq!(hash1, hash2); // Same file should produce same hash
        
        // Hash should be 40 characters (SHA1 hex)
        let hash = hash1.unwrap();
        assert_eq!(hash.len(), 40);
        assert!(hash.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    #[serial]
    fn test_get_thumbnail_hash_different_files() {
        let temp_dir = TempDir::new().unwrap();
        let file1 = temp_dir.path().join("image1.jpg");
        let file2 = temp_dir.path().join("image2.jpg");
        
        // Create different test files
        File::create(&file1).unwrap().write_all(b"image data 1").unwrap();
        File::create(&file2).unwrap().write_all(b"image data 2").unwrap();
        
        let hash1 = BackgroundPickerApp::get_thumbnail_hash(&file1);
        let hash2 = BackgroundPickerApp::get_thumbnail_hash(&file2);
        
        assert!(hash1.is_some());
        assert!(hash2.is_some());
        assert_ne!(hash1, hash2); // Different files should produce different hashes
    }

    #[test]
    fn test_get_thumbnail_hash_nonexistent_file() {
        let nonexistent = PathBuf::from("/nonexistent/path/image.jpg");
        let hash = BackgroundPickerApp::get_thumbnail_hash(&nonexistent);
        
        // Should still return a hash based on the path
        assert!(hash.is_some());
        let hash_str = hash.unwrap();
        assert_eq!(hash_str.len(), 40);
    }
}

#[cfg(test)]
mod cache_validation_tests {
    use super::*;
    use std::fs::File;
    use std::time::Duration;

    #[test]
    #[serial]
    fn test_is_thumbnail_cache_valid_static_cache_newer() {
        let temp_dir = TempDir::new().unwrap();
        let original_file = temp_dir.path().join("original.jpg");
        let cache_file = temp_dir.path().join("cache.png");
        
        // Create original file first
        File::create(&original_file).unwrap();
        
        // Wait a bit to ensure different timestamps
        std::thread::sleep(Duration::from_millis(10));
        
        // Create cache file after original
        File::create(&cache_file).unwrap();
        
        assert!(BackgroundPickerApp::is_thumbnail_cache_valid_static(&original_file, &cache_file));
    }

    #[test]
    #[serial]
    fn test_is_thumbnail_cache_valid_static_cache_older() {
        let temp_dir = TempDir::new().unwrap();
        let original_file = temp_dir.path().join("original.jpg");
        let cache_file = temp_dir.path().join("cache.png");
        
        // Create cache file first
        File::create(&cache_file).unwrap();
        
        // Wait a bit to ensure different timestamps
        std::thread::sleep(Duration::from_millis(10));
        
        // Create/modify original file after cache
        File::create(&original_file).unwrap();
        
        assert!(!BackgroundPickerApp::is_thumbnail_cache_valid_static(&original_file, &cache_file));
    }

    #[test]
    fn test_is_thumbnail_cache_valid_static_cache_not_exists() {
        let temp_dir = TempDir::new().unwrap();
        let original_file = temp_dir.path().join("original.jpg");
        let cache_file = temp_dir.path().join("nonexistent_cache.png");
        
        File::create(&original_file).unwrap();
        
        assert!(!BackgroundPickerApp::is_thumbnail_cache_valid_static(&original_file, &cache_file));
    }
}

#[cfg(test)]
mod thumbnail_cache_path_tests {
    use super::*;

    #[test]
    #[serial]
    fn test_get_cached_thumbnail_path_static() {
        let temp_dir = TempDir::new().unwrap();
        let cache_dir = temp_dir.path().join("cache");
        let test_file = temp_dir.path().join("test.jpg");
        
        // Create the test file so hash generation works
        File::create(&test_file).unwrap();
        
        let cache_path = BackgroundPickerApp::get_cached_thumbnail_path_static(&test_file, &cache_dir);
        
        assert!(cache_path.is_some());
        let path = cache_path.unwrap();
        
        // Should be in the cache directory
        assert!(path.starts_with(&cache_dir));
        
        // Should have .png extension
        assert_eq!(path.extension().unwrap(), "png");
        
        // Filename should be a 40-character hash
        let filename = path.file_stem().unwrap().to_string_lossy();
        assert_eq!(filename.len(), 40);
        assert!(filename.chars().all(|c| c.is_ascii_hexdigit()));
    }
}

#[cfg(test)]
mod image_processing_tests {
    use super::*;
    use image::{DynamicImage, RgbImage};

    #[test]
    fn test_create_thumbnail_fast() {
        // Create a simple test image
        let test_image = DynamicImage::ImageRgb8(RgbImage::new(100, 100));
        let thumbnail_size = 50;
        
        let result = BackgroundPickerApp::create_thumbnail_fast(test_image, thumbnail_size);
        
        assert!(result.is_some());
        let thumbnail = result.unwrap();
        
        // Check dimensions
        assert_eq!(thumbnail.size[0], thumbnail_size as usize);
        assert_eq!(thumbnail.size[1], thumbnail_size as usize);
        
        // Check that we have the right number of pixels
        assert_eq!(thumbnail.pixels.len(), (thumbnail_size * thumbnail_size) as usize);
    }

    #[test]
    fn test_create_thumbnail_fast_different_sizes() {
        let test_sizes = vec![32, 64, 128, 256];
        let test_image = DynamicImage::ImageRgb8(RgbImage::new(200, 200));
        
        for size in test_sizes {
            let result = BackgroundPickerApp::create_thumbnail_fast(test_image.clone(), size);
            assert!(result.is_some());
            
            let thumbnail = result.unwrap();
            assert_eq!(thumbnail.size[0], size as usize);
            assert_eq!(thumbnail.size[1], size as usize);
        }
    }

    #[test]
    #[serial]
    fn test_fast_thumbnail_generation_invalid_file() {
        let temp_dir = TempDir::new().unwrap();
        let invalid_file = temp_dir.path().join("nonexistent.jpg");
        
        let result = BackgroundPickerApp::fast_thumbnail_generation(&invalid_file, 150);
        assert!(result.is_none());
    }

    #[test]
    #[serial]
    fn test_fast_thumbnail_generation_non_image_file() {
        let temp_dir = TempDir::new().unwrap();
        let text_file = temp_dir.path().join("test.txt");
        
        // Create a text file
        std::fs::write(&text_file, "This is not an image").unwrap();
        
        let result = BackgroundPickerApp::fast_thumbnail_generation(&text_file, 150);
        assert!(result.is_none());
    }
}

#[cfg(test)]
mod cache_directory_tests {
    use super::*;

    #[test]
    fn test_get_thumbnail_cache_dir() {
        let cache_dir = BackgroundPickerApp::get_thumbnail_cache_dir();
        
        // Should return a valid path
        let cache_path = cache_dir.unwrap();
        assert!(cache_path.is_absolute());
        
        // Should end with thumbnails/normal
        assert!(cache_path.ends_with("thumbnails/normal"));
    }

    #[test]
    #[serial]
    fn test_find_existing_thumbnail_no_cache() {
        let temp_dir = TempDir::new().unwrap();
        let test_file = temp_dir.path().join("test.jpg");
        
        // Create test file
        File::create(&test_file).unwrap();
        
        // Should return None when no cache exists
        let result = BackgroundPickerApp::find_existing_thumbnail(&test_file);
        assert!(result.is_none());
    }
}

#[cfg(test)]
mod file_scanning_tests {
    use super::*;
    use std::fs;

    fn create_test_image_structure(base_dir: &std::path::Path) -> std::io::Result<()> {
        // Create directory structure
        fs::create_dir_all(base_dir.join("folder1"))?;
        fs::create_dir_all(base_dir.join("folder2"))?;
        fs::create_dir_all(base_dir.join("nested/subfolder"))?;
        
        // Create various file types
        fs::write(base_dir.join("image1.jpg"), b"fake jpg data")?;
        fs::write(base_dir.join("image2.png"), b"fake png data")?;
        fs::write(base_dir.join("image3.JPEG"), b"fake jpeg data")?; // Test case insensitive
        fs::write(base_dir.join("document.txt"), b"not an image")?;
        fs::write(base_dir.join("folder1/photo1.gif"), b"fake gif data")?;
        fs::write(base_dir.join("folder1/photo2.bmp"), b"fake bmp data")?;
        fs::write(base_dir.join("folder2/image.webp"), b"fake webp data")?;
        fs::write(base_dir.join("nested/subfolder/deep.png"), b"fake deep png")?;
        fs::write(base_dir.join("no_extension"), b"file without extension")?;
        
        Ok(())
    }

    #[test]
    #[serial]
    fn test_scan_images_basic() {
        let temp_dir = TempDir::new().unwrap();
        create_test_image_structure(temp_dir.path()).unwrap();
        
        let mut args = Args::try_parse_from(["background-picker"]).unwrap();
        args.directory = temp_dir.path().to_path_buf();
        args.debug = false;
        
        // Mock the creation context - this is tricky without egui
        // For unit tests, we'll test the scan_images method directly by creating a minimal app
        let (sender, _receiver) = std::sync::mpsc::channel();
        let thread_pool = rayon::ThreadPoolBuilder::new()
            .num_threads(1)
            .build()
            .unwrap();
        
        let mut app = BackgroundPickerApp {
            args: args.clone(),
            images: Arc::new(RwLock::new(Vec::new())),
            folder_tree: std::collections::HashMap::new(),
            loading: true,
            thumbnail_sender: sender,
            thumbnail_receiver: _receiver,
            thread_pool,
            cache_dir: temp_dir.path().join("cache"),
        };
        
        let _ = app.scan_images();
        
        let images = app.images.read().unwrap();
        
        // Should find 7 image files (excluding .txt and no_extension)
        assert_eq!(images.len(), 7);
        
        // Check that we have the right folder structure
        // Note: Root folder might be empty if all files are in subdirectories
        if let Some(root_images) = app.folder_tree.get(".") {
            assert_eq!(root_images.len(), 3); // image1.jpg, image2.png, image3.JPEG
        }
        assert!(app.folder_tree.contains_key("folder1"));
        assert!(app.folder_tree.contains_key("folder2"));
        assert!(app.folder_tree.contains_key("nested/subfolder"));
        
        // Check folder1 images
        let folder1_images = app.folder_tree.get("folder1").unwrap();
        assert_eq!(folder1_images.len(), 2); // photo1.gif, photo2.bmp
        
        // Check folder2 images
        let folder2_images = app.folder_tree.get("folder2").unwrap();
        assert_eq!(folder2_images.len(), 1); // image.webp
        
        // Check nested folder images
        let nested_images = app.folder_tree.get("nested/subfolder").unwrap();
        assert_eq!(nested_images.len(), 1); // deep.png
        
        assert!(!app.loading);
    }

    #[test]
    #[serial]
    fn test_scan_images_empty_directory() {
        let temp_dir = TempDir::new().unwrap();
        
        let mut args = Args::try_parse_from(["background-picker"]).unwrap();
        args.directory = temp_dir.path().to_path_buf();
        
        let (sender, _receiver) = std::sync::mpsc::channel();
        let thread_pool = rayon::ThreadPoolBuilder::new()
            .num_threads(1)
            .build()
            .unwrap();
        
        let mut app = BackgroundPickerApp {
            args: args.clone(),
            images: Arc::new(RwLock::new(Vec::new())),
            folder_tree: std::collections::HashMap::new(),
            loading: true,
            thumbnail_sender: sender,
            thumbnail_receiver: _receiver,
            thread_pool,
            cache_dir: temp_dir.path().join("cache"),
        };
        
        let _ = app.scan_images();
        
        let images = app.images.read().unwrap();
        assert_eq!(images.len(), 0);
        assert!(app.folder_tree.is_empty());
        assert!(!app.loading);
    }

    #[test]
    #[serial]
    fn test_scan_images_nonexistent_directory() {
        let nonexistent_dir = PathBuf::from("/nonexistent/directory");
        
        let mut args = Args::try_parse_from(["background-picker"]).unwrap();
        args.directory = nonexistent_dir;
        
        let (sender, _receiver) = std::sync::mpsc::channel();
        let thread_pool = rayon::ThreadPoolBuilder::new()
            .num_threads(1)
            .build()
            .unwrap();
        
        let mut app = BackgroundPickerApp {
            args: args.clone(),
            images: Arc::new(RwLock::new(Vec::new())),
            folder_tree: std::collections::HashMap::new(),
            loading: true,
            thumbnail_sender: sender,
            thumbnail_receiver: _receiver,
            thread_pool,
            cache_dir: PathBuf::from("/tmp/cache"),
        };
        
        let _ = app.scan_images();
        
        let images = app.images.read().unwrap();
        assert_eq!(images.len(), 0);
        assert!(app.folder_tree.is_empty());
        assert!(!app.loading);
    }
}

#[cfg(test)]
mod error_handling_tests {
    use super::*;

    #[test]
    fn test_set_background_empty_command() {
        let temp_dir = TempDir::new().unwrap();
        let test_image = temp_dir.path().join("test.jpg");
        fs::write(&test_image, b"fake image").unwrap();
        
        let mut args = Args::try_parse_from(["background-picker"]).unwrap();
        args.command = "".to_string(); // Empty command
        
        let (sender, _receiver) = std::sync::mpsc::channel();
        let thread_pool = rayon::ThreadPoolBuilder::new()
            .num_threads(1)
            .build()
            .unwrap();
        
        let app = BackgroundPickerApp {
            args,
            images: Arc::new(RwLock::new(Vec::new())),
            folder_tree: std::collections::HashMap::new(),
            loading: false,
            thumbnail_sender: sender,
            thumbnail_receiver: _receiver,
            thread_pool,
            cache_dir: temp_dir.path().join("cache"),
        };
        
        let result = app.set_background(&test_image);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Empty command"));
    }

    #[test]
    #[serial]
    fn test_set_background_invalid_command() {
        let temp_dir = TempDir::new().unwrap();
        let test_image = temp_dir.path().join("test.jpg");
        fs::write(&test_image, b"fake image").unwrap();
        
        let mut args = Args::try_parse_from(["background-picker"]).unwrap();
        args.command = "nonexistent_command_that_should_fail".to_string();
        
        let (sender, _receiver) = std::sync::mpsc::channel();
        let thread_pool = rayon::ThreadPoolBuilder::new()
            .num_threads(1)
            .build()
            .unwrap();
        
        let app = BackgroundPickerApp {
            args,
            images: Arc::new(RwLock::new(Vec::new())),
            folder_tree: std::collections::HashMap::new(),
            loading: false,
            thumbnail_sender: sender,
            thumbnail_receiver: _receiver,
            thread_pool,
            cache_dir: temp_dir.path().join("cache"),
        };
        
        let result = app.set_background(&test_image);
        assert!(result.is_err());
    }

    #[test]
    #[serial]
    fn test_set_background_valid_echo_command() {
        let temp_dir = TempDir::new().unwrap();
        let test_image = temp_dir.path().join("test.jpg");
        fs::write(&test_image, b"fake image").unwrap();
        
        let mut args = Args::try_parse_from(["background-picker"]).unwrap();
        args.command = "echo".to_string(); // Echo should always succeed
        
        let (sender, _receiver) = std::sync::mpsc::channel();
        let thread_pool = rayon::ThreadPoolBuilder::new()
            .num_threads(1)
            .build()
            .unwrap();
        
        let app = BackgroundPickerApp {
            args,
            images: Arc::new(RwLock::new(Vec::new())),
            folder_tree: std::collections::HashMap::new(),
            loading: false,
            thumbnail_sender: sender,
            thumbnail_receiver: _receiver,
            thread_pool,
            cache_dir: temp_dir.path().join("cache"),
        };
        
        let result = app.set_background(&test_image);
        assert!(result.is_ok());
    }

    #[test]
    #[serial]
    fn test_save_selected_image_permission_denied() {
        let temp_dir = TempDir::new().unwrap();
        
        let mut args = Args::try_parse_from(["background-picker"]).unwrap();
        // Try to save to a directory that doesn't exist and can't be created
        args.selected_image_file = PathBuf::from("/root/forbidden/selected.txt");
        
        let (sender, receiver) = std::sync::mpsc::channel();
        let thread_pool = rayon::ThreadPoolBuilder::new()
            .num_threads(1)
            .build()
            .unwrap();
        
        let app = BackgroundPickerApp {
            args,
            images: Arc::new(RwLock::new(Vec::new())),
            folder_tree: std::collections::HashMap::new(),
            loading: false,
            thumbnail_sender: sender,
            thumbnail_receiver: receiver,
            thread_pool,
            cache_dir: temp_dir.path().join("cache"),
        };
        
        let test_path = PathBuf::from("/path/to/image.jpg");
        let result = app.save_selected_image(&test_path);
        assert!(result.is_err());
    }
}

// Helper to create a valid PNG image at the given path
fn create_real_png(path: &std::path::Path, width: u32, height: u32) {
    use image::{DynamicImage, ImageBuffer, RgbImage};
    let img: RgbImage = ImageBuffer::from_fn(width, height, |x, y| {
        image::Rgb([(x % 255) as u8, (y % 255) as u8, 128u8])
    });
    DynamicImage::ImageRgb8(img).save(path).unwrap();
}


#[cfg(test)]
mod real_image_tests {
    use super::*;
    use image::{DynamicImage, ImageBuffer, RgbImage};

    #[test]
    #[serial]
    fn test_fast_thumbnail_generation_real_png() {
        let temp_dir = TempDir::new().unwrap();
        let path = temp_dir.path().join("test.png");
        create_real_png(&path, 200, 200);

        let result = BackgroundPickerApp::fast_thumbnail_generation(&path, 100);
        assert!(result.is_some());
        let thumb = result.unwrap();
        assert_eq!(thumb.size[0], 100);
        assert_eq!(thumb.size[1], 100);
        assert_eq!(thumb.pixels.len(), 100 * 100);
    }

    #[test]
    #[serial]
    fn test_fast_thumbnail_generation_real_jpeg() {
        let temp_dir = TempDir::new().unwrap();
        let path = temp_dir.path().join("test.jpg");
        let img: RgbImage = ImageBuffer::from_fn(200, 200, |x, y| {
            image::Rgb([(x % 255) as u8, (y % 255) as u8, 64u8])
        });
        DynamicImage::ImageRgb8(img).save(&path).unwrap();

        let result = BackgroundPickerApp::fast_thumbnail_generation(&path, 100);
        assert!(result.is_some());
        let thumb = result.unwrap();
        // JPEG may not produce exactly 100x100 due to aspect ratio, but pixels match dimensions
        assert_eq!(thumb.pixels.len(), thumb.size[0] * thumb.size[1]);
    }

    // Scale factor > 4 path: max(width, height) / size > 4 → need image > 400px for size=100
    #[test]
    #[serial]
    fn test_fast_thumbnail_generation_scale_factor_4x() {
        let temp_dir = TempDir::new().unwrap();
        let path = temp_dir.path().join("large.png");
        create_real_png(&path, 500, 500);

        let result = BackgroundPickerApp::fast_thumbnail_generation(&path, 100);
        assert!(result.is_some());
        let thumb = result.unwrap();
        assert_eq!(thumb.size[0], 100);
        assert_eq!(thumb.size[1], 100);
    }

    // Scale factor > 8 path: max(width, height) / size > 8 → need image > 800px for size=100
    #[test]
    #[serial]
    fn test_fast_thumbnail_generation_scale_factor_8x() {
        let temp_dir = TempDir::new().unwrap();
        let path = temp_dir.path().join("verylarge.png");
        create_real_png(&path, 900, 900);

        let result = BackgroundPickerApp::fast_thumbnail_generation(&path, 100);
        assert!(result.is_some());
        let thumb = result.unwrap();
        assert_eq!(thumb.size[0], 100);
        assert_eq!(thumb.size[1], 100);
    }

    // Image already smaller than target size — early return path
    #[test]
    #[serial]
    fn test_fast_thumbnail_generation_small_image_passthrough() {
        let temp_dir = TempDir::new().unwrap();
        let path = temp_dir.path().join("small.png");
        create_real_png(&path, 40, 40);

        let result = BackgroundPickerApp::fast_thumbnail_generation(&path, 100);
        assert!(result.is_some());
        let thumb = result.unwrap();
        // Image was 40x40, smaller than target, so resize still runs (size <= target)
        assert_eq!(thumb.pixels.len(), thumb.size[0] * thumb.size[1]);
    }
}

#[cfg(test)]
mod non_square_image_tests {
    use super::*;
    use image::{DynamicImage, ImageBuffer, RgbImage};

    #[test]
    fn test_create_thumbnail_fast_landscape() {
        // 200 wide, 100 tall — resize to fit within 50x50 preserves aspect ratio → 50x25
        let img: RgbImage = ImageBuffer::new(200, 100);
        let result = BackgroundPickerApp::create_thumbnail_fast(DynamicImage::ImageRgb8(img), 50);
        assert!(result.is_some());
        let thumb = result.unwrap();
        assert_eq!(thumb.size[0], 50); // width limited by size
        assert_eq!(thumb.size[1], 25); // height proportional
        assert_eq!(thumb.pixels.len(), 50 * 25);
    }

    #[test]
    fn test_create_thumbnail_fast_portrait() {
        // 100 wide, 200 tall — resize to fit within 50x50 → 25x50
        let img: RgbImage = ImageBuffer::new(100, 200);
        let result = BackgroundPickerApp::create_thumbnail_fast(DynamicImage::ImageRgb8(img), 50);
        assert!(result.is_some());
        let thumb = result.unwrap();
        assert_eq!(thumb.size[0], 25);
        assert_eq!(thumb.size[1], 50);
        assert_eq!(thumb.pixels.len(), 25 * 50);
    }

    #[test]
    fn test_create_thumbnail_fast_1x1_image() {
        let img: RgbImage = ImageBuffer::new(1, 1);
        let result = BackgroundPickerApp::create_thumbnail_fast(DynamicImage::ImageRgb8(img), 50);
        assert!(result.is_some());
        let thumb = result.unwrap();
        assert_eq!(thumb.pixels.len(), thumb.size[0] * thumb.size[1]);
    }

    #[test]
    fn test_create_thumbnail_fast_pixel_count_matches_dimensions() {
        // Invariant: pixels.len() == size[0] * size[1] for any input
        for &(w, h) in &[(100u32, 100u32), (300, 150), (150, 300), (1, 1), (1000, 500)] {
            let img: RgbImage = ImageBuffer::new(w, h);
            let result = BackgroundPickerApp::create_thumbnail_fast(DynamicImage::ImageRgb8(img), 64);
            assert!(result.is_some(), "Failed for {}x{}", w, h);
            let thumb = result.unwrap();
            assert_eq!(
                thumb.pixels.len(),
                thumb.size[0] * thumb.size[1],
                "Pixel count mismatch for {}x{}", w, h
            );
        }
    }
}

#[cfg(test)]
mod thumbnail_save_load_tests {
    use super::*;

    #[test]
    #[serial]
    fn test_save_and_load_thumbnail_roundtrip() {
        let temp_dir = TempDir::new().unwrap();
        let original = temp_dir.path().join("original.png");
        create_real_png(&original, 200, 200);

        let cache_dir = temp_dir.path().join("cache");
        fs::create_dir_all(&cache_dir).unwrap();
        let cache_path = cache_dir.join("thumb.png");

        // Generate a thumbnail
        let generated = BackgroundPickerApp::fast_thumbnail_generation(&original, 100).unwrap();

        // Save it to cache
        BackgroundPickerApp::save_thumbnail_to_cache(&generated, &cache_path, &original);
        assert!(cache_path.exists(), "Cache file should be created");

        // Load it back
        let loaded = BackgroundPickerApp::load_cached_thumbnail(&cache_path, 100);
        assert!(loaded.is_some(), "Should load from cache");
        let loaded_thumb = loaded.unwrap();
        assert_eq!(loaded_thumb.pixels.len(), loaded_thumb.size[0] * loaded_thumb.size[1]);
    }

    #[test]
    fn test_load_cached_thumbnail_nonexistent_file() {
        let result = BackgroundPickerApp::load_cached_thumbnail(
            &PathBuf::from("/nonexistent/thumbnail.png"),
            100,
        );
        assert!(result.is_none());
    }

    #[test]
    #[serial]
    fn test_load_cached_thumbnail_corrupt_file() {
        let temp_dir = TempDir::new().unwrap();
        let corrupt = temp_dir.path().join("corrupt.png");
        fs::write(&corrupt, b"this is not a valid png file").unwrap();

        let result = BackgroundPickerApp::load_cached_thumbnail(&corrupt, 100);
        assert!(result.is_none());
    }

    #[test]
    #[serial]
    fn test_save_thumbnail_with_metadata_creates_png() {
        let temp_dir = TempDir::new().unwrap();
        let original = temp_dir.path().join("original.png");
        create_real_png(&original, 100, 100);

        let out_path = temp_dir.path().join("output.png");
        let img = image::open(&original).unwrap();
        BackgroundPickerApp::save_thumbnail_with_metadata(&img, &out_path, &original);

        assert!(out_path.exists(), "Output PNG should be created");
        assert!(out_path.metadata().unwrap().len() > 0, "Output file should not be empty");
    }

    #[test]
    #[serial]
    fn test_load_cached_thumbnail_resizes_if_different_size() {
        let temp_dir = TempDir::new().unwrap();
        let original = temp_dir.path().join("original.png");
        create_real_png(&original, 200, 200);

        let cache_dir = temp_dir.path().join("cache");
        fs::create_dir_all(&cache_dir).unwrap();
        let cache_path = cache_dir.join("thumb.png");

        // Save a 100x100 thumbnail
        let generated = BackgroundPickerApp::fast_thumbnail_generation(&original, 100).unwrap();
        BackgroundPickerApp::save_thumbnail_to_cache(&generated, &cache_path, &original);

        // Load it back at a different target size (64)
        let loaded = BackgroundPickerApp::load_cached_thumbnail(&cache_path, 64);
        assert!(loaded.is_some());
        let thumb = loaded.unwrap();
        // Should have been resized to fit within 64x64
        assert!(thumb.size[0] <= 64 && thumb.size[1] <= 64);
    }
}

#[cfg(test)]
mod find_existing_thumbnail_tests {
    use super::*;

    #[test]
    #[serial]
    fn test_find_existing_thumbnail_with_valid_cached_thumbnail() {
        let temp_dir = TempDir::new().unwrap();
        let original = temp_dir.path().join("image.png");
        create_real_png(&original, 200, 200);

        let cache_dir = temp_dir.path().join("thumbnails").join("normal");
        fs::create_dir_all(&cache_dir).unwrap();

        let abs_original = fs::canonicalize(&original).unwrap();
        let hash = BackgroundPickerApp::get_thumbnail_hash(&abs_original).unwrap();
        let thumb_path = cache_dir.join(format!("{}.png", hash));

        // Save a real thumbnail to the expected cache location
        let generated = BackgroundPickerApp::fast_thumbnail_generation(&original, 128).unwrap();
        BackgroundPickerApp::save_thumbnail_to_cache(&generated, &thumb_path, &original);

        // Override dirs::cache_dir is not possible, but we verify the hash logic is consistent
        // The function uses dirs::cache_dir() internally so we can't easily redirect it in tests.
        // Instead, test that get_thumbnail_hash produces the SHA1 of the file:// URI format.
        let uri = format!("file://{}", abs_original.to_string_lossy());
        use sha1::{Digest, Sha1};
        let mut hasher = Sha1::new();
        hasher.update(uri.as_bytes());
        let expected_hash = format!("{:x}", hasher.finalize());
        assert_eq!(hash, expected_hash, "Hash should match SHA1 of file URI");
    }

    #[test]
    #[serial]
    fn test_get_cached_thumbnail_path_static_same_file_same_path() {
        let temp_dir = TempDir::new().unwrap();
        let file = temp_dir.path().join("img.png");
        File::create(&file).unwrap();
        let cache_dir = temp_dir.path().join("cache");

        let path1 = BackgroundPickerApp::get_cached_thumbnail_path_static(&file, &cache_dir);
        let path2 = BackgroundPickerApp::get_cached_thumbnail_path_static(&file, &cache_dir);

        assert_eq!(path1, path2, "Same file must always produce the same cache path");
    }

    #[test]
    #[serial]
    fn test_get_cached_thumbnail_path_static_different_files_different_paths() {
        let temp_dir = TempDir::new().unwrap();
        let file1 = temp_dir.path().join("img1.png");
        let file2 = temp_dir.path().join("img2.png");
        File::create(&file1).unwrap();
        File::create(&file2).unwrap();
        let cache_dir = temp_dir.path().join("cache");

        let path1 = BackgroundPickerApp::get_cached_thumbnail_path_static(&file1, &cache_dir);
        let path2 = BackgroundPickerApp::get_cached_thumbnail_path_static(&file2, &cache_dir);

        assert_ne!(path1, path2, "Different files must produce different cache paths");
    }
}

#[cfg(test)]
mod preload_batch_tests {
    use super::*;
    use background_picker::ImageInfo;

    fn make_app_with_images(temp_dir: &TempDir, image_paths: Vec<PathBuf>) -> BackgroundPickerApp {
        let args = Args {
            directory: temp_dir.path().to_path_buf(),
            thumbnail_size: 50,
            command: "echo".to_string(),
            selected_image_file: temp_dir.path().join("selected.txt"),
            debug: false,
            pregenerate: false,
        };
        let (sender, receiver) = std::sync::mpsc::channel();
        let thread_pool = rayon::ThreadPoolBuilder::new().num_threads(1).build().unwrap();
        let images: Vec<ImageInfo> = image_paths
            .iter()
            .map(|p| ImageInfo {
                path: p.clone(),
                thumbnail: None,
                relative_path: p.to_string_lossy().into_owned(),
                loading: false,
            })
            .collect();
        BackgroundPickerApp {
            args,
            images: Arc::new(RwLock::new(images)),
            folder_tree: std::collections::HashMap::new(),
            loading: false,
            thumbnail_sender: sender,
            thumbnail_receiver: receiver,
            thread_pool,
            cache_dir: temp_dir.path().join("cache"),
        }
    }

    #[test]
    #[serial]
    fn test_preload_batch_empty_indices() {
        let temp_dir = TempDir::new().unwrap();
        let mut app = make_app_with_images(&temp_dir, vec![]);
        // Should not panic with empty input
        app.preload_batch(&[]);
        let images = app.images.read().unwrap();
        assert!(images.is_empty());
    }

    #[test]
    #[serial]
    fn test_preload_batch_out_of_range_indices() {
        let temp_dir = TempDir::new().unwrap();
        let path = temp_dir.path().join("a.png");
        create_real_png(&path, 50, 50);
        let mut app = make_app_with_images(&temp_dir, vec![path]);

        // Indices beyond the image list — should not panic
        app.preload_batch(&[100, 200, 999]);

        let images = app.images.read().unwrap();
        assert_eq!(images.len(), 1);
        assert!(!images[0].loading); // Out-of-range indices don't affect existing images
    }

    #[test]
    #[serial]
    fn test_preload_batch_marks_images_as_loading() {
        let temp_dir = TempDir::new().unwrap();
        let path = temp_dir.path().join("test.png");
        create_real_png(&path, 50, 50);
        let mut app = make_app_with_images(&temp_dir, vec![path]);

        app.preload_batch(&[0]);

        // After preload_batch, image should be marked as loading (spawn is async)
        let images = app.images.read().unwrap();
        assert!(images[0].loading || images[0].thumbnail.is_some(),
            "Image should be loading or already have thumbnail");
    }

    #[test]
    #[serial]
    fn test_preload_batch_skips_already_loading() {
        let temp_dir = TempDir::new().unwrap();
        let path = temp_dir.path().join("test.png");
        create_real_png(&path, 50, 50);
        let mut app = make_app_with_images(&temp_dir, vec![path]);

        // Mark image as already loading
        {
            let mut images = app.images.write().unwrap();
            images[0].loading = true;
        }

        app.preload_batch(&[0]);

        // Loading state should remain true (not reset or double-spawned)
        let images = app.images.read().unwrap();
        assert!(images[0].loading);
    }
}

#[cfg(test)]
mod error_message_tests {
    use super::*;
    use background_picker::BackgroundPickerError;

    #[test]
    fn test_error_command_execution_message() {
        let err = BackgroundPickerError::CommandExecution("feh not found".to_string());
        let msg = err.to_string();
        assert!(msg.contains("Command execution failed"));
        assert!(msg.contains("feh not found"));
    }

    #[test]
    fn test_error_invalid_image_file_message() {
        let err = BackgroundPickerError::InvalidImageFile(PathBuf::from("/some/image.tiff"));
        let msg = err.to_string();
        assert!(msg.contains("Invalid image file"));
        assert!(msg.contains("image.tiff"));
    }

    #[test]
    fn test_error_lock_acquisition_message() {
        let err = BackgroundPickerError::LockAcquisition;
        assert!(err.to_string().contains("Lock acquisition failed"));
    }

    #[test]
    fn test_error_cache_directory_creation_message() {
        let io_err = std::io::Error::new(std::io::ErrorKind::PermissionDenied, "permission denied");
        let err = BackgroundPickerError::CacheDirectoryCreation(io_err);
        assert!(err.to_string().contains("Failed to create thumbnail cache directory"));
    }

    #[test]
    fn test_error_thumbnail_generation_message() {
        let source: Box<dyn std::error::Error + Send + Sync> =
            Box::new(std::io::Error::new(std::io::ErrorKind::NotFound, "file not found"));
        let err = BackgroundPickerError::ThumbnailGeneration {
            path: PathBuf::from("/images/test.png"),
            source,
        };
        let msg = err.to_string();
        assert!(msg.contains("Failed to generate thumbnail"));
        assert!(msg.contains("test.png"));
    }
}

#[cfg(test)]
mod additional_scan_tests {
    use super::*;

    #[test]
    #[serial]
    fn test_scan_images_clears_previous_data() {
        let temp_dir = TempDir::new().unwrap();
        fs::write(temp_dir.path().join("a.png"), b"fake").unwrap();

        let args = Args {
            directory: temp_dir.path().to_path_buf(),
            thumbnail_size: 50,
            command: "echo".to_string(),
            selected_image_file: temp_dir.path().join("sel.txt"),
            debug: false,
            pregenerate: false,
        };
        let (sender, receiver) = std::sync::mpsc::channel();
        let thread_pool = rayon::ThreadPoolBuilder::new().num_threads(1).build().unwrap();

        let mut app = BackgroundPickerApp {
            args,
            images: Arc::new(RwLock::new(Vec::new())),
            folder_tree: std::collections::HashMap::new(),
            loading: true,
            thumbnail_sender: sender,
            thumbnail_receiver: receiver,
            thread_pool,
            cache_dir: temp_dir.path().join("cache"),
        };

        app.scan_images().unwrap();
        assert_eq!(app.images.read().unwrap().len(), 1);

        // Add more files and rescan — previous data should be cleared
        fs::write(temp_dir.path().join("b.jpg"), b"fake").unwrap();
        fs::write(temp_dir.path().join("c.gif"), b"fake").unwrap();

        app.scan_images().unwrap();
        assert_eq!(app.images.read().unwrap().len(), 3);
    }

    #[test]
    #[serial]
    fn test_scan_images_relative_paths_exclude_base() {
        let temp_dir = TempDir::new().unwrap();
        let sub = temp_dir.path().join("sub");
        fs::create_dir_all(&sub).unwrap();
        fs::write(sub.join("photo.png"), b"fake").unwrap();

        let args = Args {
            directory: temp_dir.path().to_path_buf(),
            thumbnail_size: 50,
            command: "echo".to_string(),
            selected_image_file: temp_dir.path().join("sel.txt"),
            debug: false,
            pregenerate: false,
        };
        let (sender, receiver) = std::sync::mpsc::channel();
        let thread_pool = rayon::ThreadPoolBuilder::new().num_threads(1).build().unwrap();

        let mut app = BackgroundPickerApp {
            args,
            images: Arc::new(RwLock::new(Vec::new())),
            folder_tree: std::collections::HashMap::new(),
            loading: true,
            thumbnail_sender: sender,
            thumbnail_receiver: receiver,
            thread_pool,
            cache_dir: temp_dir.path().join("cache"),
        };

        app.scan_images().unwrap();

        let images = app.images.read().unwrap();
        assert_eq!(images.len(), 1);
        // Relative path should not start with the base directory
        let rel = &images[0].relative_path;
        assert!(!rel.starts_with('/'), "Relative path should not be absolute: {}", rel);
        assert!(rel.contains("photo.png"), "Relative path should contain filename: {}", rel);
    }

    #[test]
    #[serial]
    fn test_scan_images_folder_tree_root_key() {
        let temp_dir = TempDir::new().unwrap();
        fs::write(temp_dir.path().join("root.png"), b"fake").unwrap();

        let args = Args {
            directory: temp_dir.path().to_path_buf(),
            thumbnail_size: 50,
            command: "echo".to_string(),
            selected_image_file: temp_dir.path().join("sel.txt"),
            debug: false,
            pregenerate: false,
        };
        let (sender, receiver) = std::sync::mpsc::channel();
        let thread_pool = rayon::ThreadPoolBuilder::new().num_threads(1).build().unwrap();

        let mut app = BackgroundPickerApp {
            args,
            images: Arc::new(RwLock::new(Vec::new())),
            folder_tree: std::collections::HashMap::new(),
            loading: true,
            thumbnail_sender: sender,
            thumbnail_receiver: receiver,
            thread_pool,
            cache_dir: temp_dir.path().join("cache"),
        };

        app.scan_images().unwrap();

        // Files at root level produce an empty string key (strip_prefix of the base dir gives "")
        assert!(
            app.folder_tree.contains_key(""),
            "Root images should be stored under empty string key; keys: {:?}",
            app.folder_tree.keys().collect::<Vec<_>>()
        );
        assert_eq!(app.folder_tree[""].len(), 1);
    }
}

#[cfg(test)]
mod additional_utility_tests {
    use super::*;

    #[test]
    fn test_is_image_file_mixed_case_extensions() {
        let cases = vec![
            ("photo.Jpg", true),
            ("photo.pNg", true),
            ("photo.JPEG", true),
            ("photo.GiF", true),
            ("photo.BMP", true),
            ("photo.WebP", true),
        ];
        for (name, expected) in cases {
            assert_eq!(
                is_image_file(&PathBuf::from(name)),
                expected,
                "Failed for {}",
                name
            );
        }
    }

    #[test]
    fn test_is_image_file_trailing_slash_path() {
        // Rust strips trailing slashes from paths, so "some/dir.jpg/" is treated as "some/dir.jpg"
        // and extension() returns Some("jpg"), making is_image_file return true.
        let path = PathBuf::from("some/dir.jpg/");
        assert!(is_image_file(&path));
    }

    #[test]
    fn test_is_image_file_hidden_file_with_image_extension() {
        assert!(is_image_file(&PathBuf::from(".hidden.png")));
    }

    #[test]
    fn test_is_image_file_multiple_dots_in_name() {
        assert!(is_image_file(&PathBuf::from("my.photo.2024.jpg")));
        assert!(!is_image_file(&PathBuf::from("my.photo.2024.txt")));
    }

    #[test]
    fn test_validate_command_with_path_containing_spaces() {
        // Command with embedded path-like args should still be valid
        assert!(validate_command("/usr/bin/feh --bg-max").is_ok());
        assert!(validate_command("   command   ").is_ok()); // has non-space content
    }
}

#[cfg(test)]
mod additional_cache_validation_tests {
    use super::*;

    #[test]
    #[serial]
    fn test_is_thumbnail_cache_valid_original_missing() {
        let temp_dir = TempDir::new().unwrap();
        let cache_file = temp_dir.path().join("cache.png");
        let nonexistent_original = temp_dir.path().join("missing_original.jpg");

        File::create(&cache_file).unwrap();

        // When original doesn't exist, metadata returns UNIX_EPOCH → cache appears valid
        // (this is the existing behavior: a missing original means the cache is "valid")
        let result = BackgroundPickerApp::is_thumbnail_cache_valid_static(
            &nonexistent_original,
            &cache_file,
        );
        // Cache was just created, original doesn't exist → defaults to UNIX_EPOCH
        // cache_modified >= original_modified (UNIX_EPOCH) → true
        assert!(result, "Cache should appear valid when original is missing");
    }

    #[test]
    #[serial]
    fn test_is_thumbnail_cache_valid_both_files_exist() {
        let temp_dir = TempDir::new().unwrap();
        let original = temp_dir.path().join("original.png");
        let cache = temp_dir.path().join("cache.png");

        File::create(&original).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(10));
        File::create(&cache).unwrap();

        assert!(BackgroundPickerApp::is_thumbnail_cache_valid_static(&original, &cache));
    }
}

#[cfg(test)]
mod args_tests {
    use super::*;

    #[test]
    fn test_args_clone() {
        let args = Args {
            directory: PathBuf::from("/images"),
            thumbnail_size: 200,
            command: "feh --bg-fill".to_string(),
            selected_image_file: PathBuf::from("sel.txt"),
            debug: true,
            pregenerate: false,
        };
        let cloned = args.clone();
        assert_eq!(cloned.directory, args.directory);
        assert_eq!(cloned.thumbnail_size, args.thumbnail_size);
        assert_eq!(cloned.command, args.command);
        assert_eq!(cloned.selected_image_file, args.selected_image_file);
        assert_eq!(cloned.debug, args.debug);
        assert_eq!(cloned.pregenerate, args.pregenerate);
    }

    #[test]
    fn test_args_pregenerate_flag_default_false() {
        let args = Args::try_parse_from(["background-picker"]).unwrap();
        assert!(!args.pregenerate);
    }

    #[test]
    fn test_args_debug_flag_default_false() {
        let args = Args::try_parse_from(["background-picker"]).unwrap();
        assert!(!args.debug);
    }

    #[test]
    fn test_args_thumbnail_size_zero() {
        // thumbnail_size is u32, so 0 is technically valid from CLI
        let args = Args::try_parse_from(["background-picker", "--thumbnail-size", "0"]).unwrap();
        assert_eq!(args.thumbnail_size, 0);
    }

    #[test]
    fn test_args_command_with_multiple_spaces() {
        let args = Args::try_parse_from([
            "background-picker",
            "--command",
            "nitrogen --set-zoom-fill --head=0",
        ])
        .unwrap();
        assert_eq!(args.command, "nitrogen --set-zoom-fill --head=0");
    }
}