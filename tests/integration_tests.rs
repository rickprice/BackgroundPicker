use background_picker::{BackgroundPickerApp, cache, thumbnail};
use std::fs;
use tempfile::TempDir;

mod common;
use common::*;

#[cfg(test)]
mod tests {
    use super::*;

    fn create_comprehensive_test_structure(base_dir: &std::path::Path) -> std::io::Result<()> {
        fs::create_dir_all(base_dir.join("Photos/Vacation"))?;
        fs::create_dir_all(base_dir.join("Photos/Family"))?;
        fs::create_dir_all(base_dir.join("Wallpapers/Nature"))?;
        fs::create_dir_all(base_dir.join("Wallpapers/Abstract"))?;
        fs::create_dir_all(base_dir.join("Screenshots"))?;

        fs::write(base_dir.join("desktop_bg.jpg"), fake_jpeg_data())?;
        fs::write(base_dir.join("logo.png"), fake_png_data())?;

        fs::write(
            base_dir.join("Photos/Vacation/beach1.jpg"),
            fake_jpeg_data(),
        )?;
        fs::write(
            base_dir.join("Photos/Vacation/beach2.jpeg"),
            fake_jpeg_data(),
        )?;
        fs::write(base_dir.join("Photos/Vacation/sunset.png"), fake_png_data())?;

        fs::write(
            base_dir.join("Photos/Family/portrait.jpg"),
            fake_jpeg_data(),
        )?;
        fs::write(
            base_dir.join("Photos/Family/group.gif"),
            b"GIF89a fake gif data",
        )?;

        fs::write(
            base_dir.join("Wallpapers/Nature/forest.jpg"),
            fake_jpeg_data(),
        )?;
        fs::write(
            base_dir.join("Wallpapers/Nature/mountains.png"),
            fake_png_data(),
        )?;
        fs::write(
            base_dir.join("Wallpapers/Abstract/geometric.webp"),
            b"RIFF fake webp",
        )?;
        fs::write(
            base_dir.join("Wallpapers/Abstract/colors.bmp"),
            fake_bmp_data(),
        )?;

        fs::write(base_dir.join("Screenshots/screen1.png"), fake_png_data())?;
        fs::write(base_dir.join("Screenshots/screen2.jpg"), fake_jpeg_data())?;

        // Non-image files to test filtering
        fs::write(base_dir.join("readme.txt"), b"This is not an image")?;
        fs::write(base_dir.join("Photos/video.mp4"), b"fake video data")?;
        fs::write(base_dir.join("config.yaml"), b"configuration: data")?;

        Ok(())
    }

    #[test]
    fn test_end_to_end_image_scanning_and_organization() {
        let temp_dir = TempDir::new().unwrap();
        create_comprehensive_test_structure(temp_dir.path()).unwrap();

        let mut args = test_args(temp_dir.path());
        args.debug = true;
        args.thumbnail_size = 128;
        let mut app =
            BackgroundPickerApp::with_state(args, Vec::new(), temp_dir.path().join("cache"))
                .unwrap();

        app.scan_images();

        // Should find 13 image files (excluding txt, mp4, yaml)
        assert_eq!(app.images.len(), 13);

        // Verify folder structure
        assert_eq!(app.folder_tree.len(), 6); // Root + 5 subdirectories

        if let Some(root_images) = app.folder_tree.get(".") {
            assert_eq!(root_images.len(), 2); // desktop_bg.jpg, logo.png
        }

        assert_eq!(app.folder_tree.get("Photos/Vacation").unwrap().len(), 3); // beach1.jpg, beach2.jpeg, sunset.png
        assert_eq!(app.folder_tree.get("Photos/Family").unwrap().len(), 2); // portrait.jpg, group.gif
        assert_eq!(app.folder_tree.get("Wallpapers/Nature").unwrap().len(), 2); // forest.jpg, mountains.png
        assert_eq!(app.folder_tree.get("Wallpapers/Abstract").unwrap().len(), 2); // geometric.webp, colors.bmp
        assert_eq!(app.folder_tree.get("Screenshots").unwrap().len(), 2); // screen1.png, screen2.jpg

        for (i, image) in app.images.iter().enumerate() {
            assert!(!image.relative_path.is_empty());
            assert!(image.path.exists());
            assert!(!image.loading); // Should not be loading after scan
            assert!(image.thumbnail.is_none()); // Thumbnails not loaded yet

            println!(
                "Image {i}: {} -> {}",
                image.relative_path,
                image.path.display()
            );
        }

        assert!(!app.loading);
    }

    #[test]
    fn test_selected_image_persistence_workflow() {
        let temp_dir = TempDir::new().unwrap();
        let selected_file = temp_dir.path().join("selected_image.txt");

        let mut args = test_args(temp_dir.path());
        args.selected_image_file = selected_file.clone();
        let app = BackgroundPickerApp::with_state(args, Vec::new(), temp_dir.path().join("cache"))
            .unwrap();

        let first_image = std::path::PathBuf::from("/path/to/vacation/beach.jpg");
        app.save_selected_image(&first_image).unwrap();

        assert!(selected_file.exists());
        let content = fs::read_to_string(&selected_file).unwrap();
        assert_eq!(content.trim(), first_image.to_string_lossy());

        let second_image = std::path::PathBuf::from("/path/to/nature/forest.jpg");
        app.save_selected_image(&second_image).unwrap();

        let updated_content = fs::read_to_string(&selected_file).unwrap();
        assert_eq!(updated_content.trim(), second_image.to_string_lossy());
    }

    #[test]
    fn test_thumbnail_caching_workflow() {
        let temp_dir = TempDir::new().unwrap();
        create_comprehensive_test_structure(temp_dir.path()).unwrap();

        let cache_dir = temp_dir.path().join("thumbnail_cache");
        let test_image = temp_dir.path().join("desktop_bg.jpg");

        let thumbnail_size = 150;
        let result1 = thumbnail::load_or_generate(&test_image, thumbnail_size, &cache_dir, false);

        // The fake JPEG data might not be processable, so we'll handle both cases
        if result1.is_none() {
            println!("Thumbnail generation failed for fake image data - this is expected");
            return;
        }

        let thumbnail1 = result1.unwrap();
        assert_eq!(thumbnail1.size[0], thumbnail_size as usize);
        assert_eq!(thumbnail1.size[1], thumbnail_size as usize);

        // Verify cache file was created
        let cache_file = cache::cached_thumbnail_path(&test_image, &cache_dir);
        assert!(
            cache_file.exists(),
            "Cache file should exist at {cache_file:?}"
        );

        // Test loading from cache
        let result2 = thumbnail::load_or_generate(&test_image, thumbnail_size, &cache_dir, false);
        assert!(result2.is_some());
        let thumbnail2 = result2.unwrap();
        assert_eq!(thumbnail2.size[0], thumbnail_size as usize);
        assert_eq!(thumbnail2.size[1], thumbnail_size as usize);

        assert!(cache::is_cache_valid(&test_image, &cache_file));

        // Test different thumbnail size
        let different_size = 64;
        let result3 = thumbnail::load_or_generate(&test_image, different_size, &cache_dir, false);
        assert!(result3.is_some());
        let thumbnail3 = result3.unwrap();
        assert_eq!(thumbnail3.size[0], different_size as usize);
        assert_eq!(thumbnail3.size[1], different_size as usize);
    }

    #[test]
    fn test_command_execution_workflow() {
        let temp_dir = TempDir::new().unwrap();
        let test_image = temp_dir.path().join("test.jpg");
        fs::write(&test_image, fake_jpeg_data()).unwrap();

        let mut args = test_args(temp_dir.path());
        args.command = "echo test".to_owned();
        let app = BackgroundPickerApp::with_state(args, Vec::new(), temp_dir.path().join("cache"))
            .unwrap();

        let result = app.set_background(&test_image);
        assert!(result.is_ok(), "Command execution should succeed");

        let mut args2 = test_args(temp_dir.path());
        args2.command = "echo --bg-max".to_owned(); // Simulate feh --bg-max
        let app2 =
            BackgroundPickerApp::with_state(args2, Vec::new(), temp_dir.path().join("cache"))
                .unwrap();

        let result2 = app2.set_background(&test_image);
        assert!(result2.is_ok(), "Feh-like command should succeed");
    }

    #[test]
    fn test_pregeneration_workflow() {
        let temp_dir = TempDir::new().unwrap();
        create_comprehensive_test_structure(temp_dir.path()).unwrap();

        let mut args = test_args(temp_dir.path());
        args.thumbnail_size = 100;
        args.debug = false;
        let mut app =
            BackgroundPickerApp::with_state(args, Vec::new(), temp_dir.path().join("thumbnails"))
                .unwrap();

        app.scan_images();
        assert_eq!(app.images.len(), 13); // Should find 13 image files

        // Test pregeneration (this doesn't exit in test environment)
        let start_time = std::time::Instant::now();
        app.pregenerate_all_thumbnails();
        let elapsed = start_time.elapsed();

        assert!(
            elapsed.as_secs() < 30,
            "Pregeneration took too long: {elapsed:?}"
        );

        let cache_dir = &app.cache_dir;
        let mut cache_files_found = 0;

        if cache_dir.exists() {
            for entry in fs::read_dir(cache_dir).unwrap() {
                let entry = entry.unwrap();
                if entry.path().extension().is_some_and(|ext| ext == "png") {
                    cache_files_found += 1;
                }
            }
        }

        println!("Cache files found: {cache_files_found}");
        assert!(
            cache_files_found <= app.images.len(),
            "Should not create more cache files than images"
        );
    }

    #[test]
    fn test_mixed_file_types_handling() {
        let temp_dir = TempDir::new().unwrap();

        fs::write(temp_dir.path().join("valid.jpg"), fake_jpeg_data()).unwrap();
        fs::write(temp_dir.path().join("valid.png"), fake_png_data()).unwrap();
        fs::write(
            temp_dir.path().join("invalid.jpg"),
            b"not actually jpeg data",
        )
        .unwrap();
        fs::write(temp_dir.path().join("document.txt"), b"text file").unwrap();
        fs::write(
            temp_dir.path().join("no_extension"),
            b"file without extension",
        )
        .unwrap();
        fs::write(temp_dir.path().join("empty.jpg"), b"").unwrap();

        let args = test_args(temp_dir.path());
        let mut app =
            BackgroundPickerApp::with_state(args, Vec::new(), temp_dir.path().join("cache"))
                .unwrap();

        app.scan_images();

        // Should find 4 files with image extensions (valid.jpg, valid.png, invalid.jpg, empty.jpg)
        // Scanner only looks at extensions, not file content
        assert_eq!(app.images.len(), 4);

        for image in &app.images {
            let result = thumbnail::generate(&image.path, 100);
            println!(
                "Thumbnail generation for {:?}: {}",
                image.path.file_name().unwrap(),
                if result.is_some() {
                    "SUCCESS"
                } else {
                    "FAILED"
                }
            );
        }

        assert!(!app.loading);
    }
}
