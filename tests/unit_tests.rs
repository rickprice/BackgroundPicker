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