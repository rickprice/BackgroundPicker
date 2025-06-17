use clap::Parser;
use eframe::egui;
use image::imageops::FilterType;
use image::ImageEncoder;
use rayon::prelude::*;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, RwLock};
use std::fs;
use std::io::{self, Write};
use std::time::SystemTime;
use walkdir::WalkDir;

const IMAGE_EXTENSIONS: &[&str] = &["jpg", "jpeg", "png", "gif", "bmp", "webp"];
const DEFAULT_PRELOAD_COUNT: usize = 8;
const CHUNK_SIZE: usize = 100;
const MIN_THREAD_COUNT: usize = 4;
const INTERMEDIATE_SIZE_MULTIPLIER: u32 = 4;
const SMALL_IMAGE_THRESHOLD: u32 = 2;
const PROGRESS_THRESHOLD: usize = 50;

#[derive(Parser, Clone)]
#[command(name = "background-picker")]
#[command(about = "A GUI tool for selecting desktop backgrounds")]
pub struct Args {
    #[arg(short, long, default_value = ".")]
    pub directory: PathBuf,
    
    #[arg(short, long, default_value = "150")]
    pub thumbnail_size: u32,
    
    #[arg(short, long, default_value = "feh --bg-max")]
    pub command: String,
    
    #[arg(short, long, default_value = "selected-background.txt")]
    pub selected_image_file: PathBuf,
    
    #[arg(long, help = "Enable debug output")]
    pub debug: bool,
    
    #[arg(long, help = "Pre-generate all thumbnails and exit (don't show GUI)")]
    pub pregenerate: bool,
}


#[derive(Clone)]
pub struct ImageInfo {
    pub path: PathBuf,
    pub thumbnail: Option<egui::TextureHandle>,
    pub relative_path: String,
    pub loading: bool,
}

pub struct BackgroundPickerApp {
    pub args: Args,
    pub images: Arc<RwLock<Vec<ImageInfo>>>,
    pub folder_tree: BTreeMap<String, Vec<usize>>,
    pub loading: bool,
    pub thumbnail_sender: std::sync::mpsc::Sender<(usize, egui::ColorImage)>,
    pub thumbnail_receiver: std::sync::mpsc::Receiver<(usize, egui::ColorImage)>,
    pub thread_pool: rayon::ThreadPool,
    pub cache_dir: PathBuf,
}

impl BackgroundPickerApp {
    pub fn new(_cc: &eframe::CreationContext<'_>, args: Args) -> Self {
        let (thumbnail_sender, thumbnail_receiver) = std::sync::mpsc::channel();
        
        // Create thread pool with optimal number of threads
        let thread_pool = rayon::ThreadPoolBuilder::new()
            .num_threads(num_cpus::get().max(MIN_THREAD_COUNT))
            .build()
            .expect("Failed to create thread pool");
        
        // Set up thumbnail cache directory (freedesktop.org spec)
        let cache_dir = Self::get_thumbnail_cache_dir();
        if args.debug {
            println!("Using thumbnail cache directory: {:?}", cache_dir);
        }
        
        let mut app = Self {
            args,
            images: Arc::new(RwLock::new(Vec::new())),
            folder_tree: BTreeMap::new(),
            loading: true,
            thumbnail_sender,
            thumbnail_receiver,
            thread_pool,
            cache_dir,
        };
        
        app.scan_images();
        
        if app.args.pregenerate {
            app.pregenerate_all_thumbnails();
            // Exit after pregeneration, don't show GUI
            std::process::exit(0);
        }
        
        app
    }
    
    pub fn get_thumbnail_cache_dir() -> PathBuf {
        // Use freedesktop.org thumbnail specification
        if let Some(cache_home) = dirs::cache_dir() {
            let thumbnails_dir = cache_home.join("thumbnails");
            
            // Try to create the directory structure if it doesn't exist
            let normal_dir = thumbnails_dir.join("normal");
            if fs::create_dir_all(&normal_dir).is_err() {
                eprintln!("Warning: Could not create thumbnail cache directory");
            }
            
            normal_dir
        } else {
            // Fallback to home directory
            dirs::home_dir()
                .unwrap_or_else(|| PathBuf::from("."))
                .join(".cache")
                .join("thumbnails")
                .join("normal")
        }
    }
    
    pub fn find_existing_thumbnail(file_path: &Path) -> Option<PathBuf> {
        // Look for existing thumbnails in multiple sizes
        let cache_home = dirs::cache_dir()?;
        let thumbnails_dir = cache_home.join("thumbnails");
        
        let hash = Self::get_thumbnail_hash(file_path)?;
        let thumbnail_name = format!("{}.png", hash);
        
        // Check in order of preference: normal (128x128), large (256x256), then fail
        for size_dir in &["normal", "large"] {
            let thumbnail_path = thumbnails_dir.join(size_dir).join(&thumbnail_name);
            if thumbnail_path.exists() && Self::is_thumbnail_cache_valid_static(file_path, &thumbnail_path) {
                return Some(thumbnail_path);
            }
        }
        
        None
    }
    
    pub fn get_thumbnail_hash(file_path: &Path) -> Option<String> {
        // Generate SHA1 hash of file URI as per freedesktop.org thumbnail spec
        // This matches exactly what pcmanfm and other file managers use
        let canonicalized = fs::canonicalize(file_path).unwrap_or_else(|_| file_path.to_path_buf());
        let file_uri = format!("file://{}", canonicalized.to_string_lossy());
        
        use sha1::{Digest, Sha1};
        let mut hasher = Sha1::new();
        hasher.update(file_uri.as_bytes());
        let result = hasher.finalize();
        Some(format!("{:x}", result))
    }
    
    
    
    pub fn scan_images(&mut self) {
        let base_path = &self.args.directory;
        
        {
            if let Ok(mut images) = self.images.write() {
                images.clear();
            }
        }
        self.folder_tree.clear();
        
        if self.args.debug {
            println!("Scanning directory: {:?}", base_path);
        }
        
        for entry in WalkDir::new(&self.args.directory)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_type().is_file())
        {
            if let Some(ext) = entry.path().extension() {
                if IMAGE_EXTENSIONS.contains(&ext.to_string_lossy().to_lowercase().as_str()) {
                    let relative_path = entry.path()
                        .strip_prefix(base_path)
                        .unwrap_or(entry.path())
                        .to_string_lossy()
                        .to_string();
                    
                    let folder = entry.path()
                        .parent()
                        .and_then(|p| p.strip_prefix(base_path).ok())
                        .map(|p| p.to_string_lossy().to_string())
                        .unwrap_or_else(|| ".".to_string());
                    
                    let image_index = {
                        if let Ok(mut images) = self.images.write() {
                            let index = images.len();
                            images.push(ImageInfo {
                                path: entry.path().to_path_buf(),
                                thumbnail: None,
                                relative_path: relative_path.clone(),
                                loading: false,
                            });
                            index
                        } else {
                            continue;
                        }
                    };
                    
                    if self.args.debug {
                        println!("Found image: {:?} in folder: {}", relative_path, folder);
                    }
                    
                    self.folder_tree
                        .entry(folder)
                        .or_default()
                        .push(image_index);
                }
            }
        }
        
        let image_count = self.images.read().map(|images| images.len()).unwrap_or(0);
        if self.args.debug {
            println!("Found {} images in {} folders", image_count, self.folder_tree.len());
        }
        
        self.loading = false;
    }
    
    pub fn pregenerate_all_thumbnails(&mut self) {
        let total_images = self.images.read().map(|images| images.len()).unwrap_or(0);
        
        if total_images == 0 {
            if self.args.debug {
                println!("No images found to pregenerate thumbnails for");
            }
            return;
        }
        
        if self.args.debug {
            println!("Pre-generating thumbnails for {} images...", total_images);
        } else {
            println!("Generating thumbnails for {} images...", total_images);
        }
        
        let start_time = std::time::Instant::now();
        let mut generated_count = 0;
        let mut cached_count = 0;
        
        // Use rayon to process all images in parallel
        let cache_dir = self.cache_dir.clone();
        let size = self.args.thumbnail_size;
        let debug = self.args.debug;
        let images = self.images.clone();
        
        let results: Vec<(bool, bool)> = (0..total_images)
            .collect::<Vec<_>>()
            .chunks(CHUNK_SIZE) // Process in chunks for progress reporting
            .enumerate()
            .flat_map(|(chunk_idx, chunk)| {
                let chunk_results: Vec<(bool, bool)> = chunk.par_iter().map(|&index| {
                    let path = {
                        if let Ok(images_guard) = images.read() {
                            if index >= images_guard.len() {
                                return (false, false); // (was_cached, was_generated)
                            }
                            images_guard[index].path.clone()
                        } else {
                            return (false, false);
                        }
                    };
                    
                    let abs_path = std::fs::canonicalize(&path).unwrap_or_else(|_| path.to_path_buf());
                    
                    // Check if thumbnail already exists
                    if let Some(existing_thumbnail) = Self::find_existing_thumbnail(&abs_path) {
                        if Self::load_cached_thumbnail(&existing_thumbnail, size).is_some() {
                            if debug {
                                println!("  [{}] Found existing thumbnail: {:?}", 
                                    index + 1, path.file_name().unwrap_or_default());
                            }
                            return (true, false); // was cached
                        }
                    }
                    
                    if let Some(cache_path) = Self::get_cached_thumbnail_path_static(&abs_path, &cache_dir) {
                        if Self::is_thumbnail_cache_valid_static(&abs_path, &cache_path) && Self::load_cached_thumbnail(&cache_path, size).is_some() {
                            if debug {
                                println!("  [{}] Found cached thumbnail: {:?}", 
                                    index + 1, path.file_name().unwrap_or_default());
                            }
                            return (true, false); // was cached
                        }
                    }
                    
                    // Generate new thumbnail
                    if let Some(color_image) = Self::fast_thumbnail_generation(&abs_path, size) {
                        // Save to cache
                        if let Some(cache_path) = Self::get_cached_thumbnail_path_static(&abs_path, &cache_dir) {
                            Self::save_thumbnail_to_cache(&color_image, &cache_path, &abs_path);
                        }
                        
                        if debug {
                            println!("  [{}] Generated thumbnail: {:?}", 
                                index + 1, path.file_name().unwrap_or_default());
                        }
                        (false, true) // was generated
                    } else {
                        if debug {
                            println!("  [{}] Failed to generate thumbnail: {:?}", 
                                index + 1, path.file_name().unwrap_or_default());
                        }
                        (false, false)
                    }
                }).collect();
                
                // Show progress for large collections
                if !debug && total_images > PROGRESS_THRESHOLD {
                    let completed = (chunk_idx + 1) * CHUNK_SIZE.min(total_images);
                    print!("\rProgress: {}/{} images processed", completed, total_images);
                    io::stdout().flush().ok();
                }
                
                chunk_results
            }).collect();
        
        // Count results
        for (was_cached, was_generated) in results {
            if was_cached {
                cached_count += 1;
            } else if was_generated {
                generated_count += 1;
            }
        }
        
        let elapsed = start_time.elapsed();
        
        if !self.args.debug && total_images > PROGRESS_THRESHOLD {
            println!(); // New line after progress indicator
        }
        
        if self.args.debug {
            println!("Thumbnail pregeneration complete:");
            println!("  - {} thumbnails found in cache", cached_count);
            println!("  - {} thumbnails generated", generated_count);
            println!("  - {} thumbnails failed", total_images - cached_count - generated_count);
            println!("  - Time elapsed: {:.2}s", elapsed.as_secs_f64());
        } else {
            println!("Thumbnail generation complete: {} cached, {} generated ({:.1}s)", 
                cached_count, generated_count, elapsed.as_secs_f64());
        }
    }
    
    pub fn load_thumbnail(&mut self, _ctx: &egui::Context, index: usize) {
        let images_len = self.images.read().map(|images| images.len()).unwrap_or(0);
        if index >= images_len {
            return;
        }
        
        let (should_load, path) = {
            if let Ok(mut images) = self.images.write() {
                if images[index].thumbnail.is_some() || images[index].loading {
                    return;
                }
                images[index].loading = true;
                (true, images[index].path.clone())
            } else {
                return;
            }
        };
        
        if should_load {
            let sender = self.thumbnail_sender.clone();
            let size = self.args.thumbnail_size;
            let cache_dir = self.cache_dir.clone();
            let debug = self.args.debug;
            
            self.thread_pool.spawn(move || {
                if let Some(color_image) = Self::load_or_generate_thumbnail(&path, size, &cache_dir, debug) {
                    let _ = sender.send((index, color_image));
                }
            });
        }
    }
    
    pub fn load_or_generate_thumbnail(path: &Path, size: u32, cache_dir: &Path, debug: bool) -> Option<egui::ColorImage> {
        // Get absolute path for cache key generation
        let abs_path = fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
        
        // First, look for existing thumbnails created by other applications (pcmanfm, etc.)
        if let Some(existing_thumbnail) = Self::find_existing_thumbnail(&abs_path) {
            if let Some(cached_image) = Self::load_cached_thumbnail(&existing_thumbnail, size) {
                if debug {
                    println!("Loaded existing system thumbnail for {:?}", path.file_name().unwrap_or_default());
                }
                return Some(cached_image);
            }
        }
        
        // Try to load from our own cache
        if let Some(cache_path) = Self::get_cached_thumbnail_path_static(&abs_path, cache_dir) {
            if Self::is_thumbnail_cache_valid_static(&abs_path, &cache_path) {
                if let Some(cached_image) = Self::load_cached_thumbnail(&cache_path, size) {
                    if debug {
                        println!("Loaded our cached thumbnail for {:?}", path.file_name().unwrap_or_default());
                    }
                    return Some(cached_image);
                }
            }
        }
        
        // Generate new thumbnail and cache it
        if debug {
            println!("Generating new thumbnail for {:?}", path.file_name().unwrap_or_default());
        }
        let color_image = Self::fast_thumbnail_generation(&abs_path, size)?;
        
        // Save to cache for future use
        if let Some(cache_path) = Self::get_cached_thumbnail_path_static(&abs_path, cache_dir) {
            Self::save_thumbnail_to_cache(&color_image, &cache_path, &abs_path);
        }
        
        Some(color_image)
    }
    
    pub fn get_cached_thumbnail_path_static(file_path: &Path, cache_dir: &Path) -> Option<PathBuf> {
        let hash = Self::get_thumbnail_hash(file_path)?;
        Some(cache_dir.join(format!("{}.png", hash)))
    }
    
    pub fn is_thumbnail_cache_valid_static(original_path: &Path, cache_path: &Path) -> bool {
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
    
    pub fn load_cached_thumbnail(cache_path: &Path, target_size: u32) -> Option<egui::ColorImage> {
        match image::io::Reader::open(cache_path) {
            Ok(reader) => {
                if let Ok(img) = reader.with_guessed_format().ok()?.decode() {
                    // Resize cached thumbnail to target size if needed
                    let resized = if img.width() != target_size || img.height() != target_size {
                        img.resize(target_size, target_size, FilterType::Nearest)
                    } else {
                        img
                    };
                    Self::create_thumbnail_fast(resized, target_size)
                } else {
                    None
                }
            }
            Err(_) => None,
        }
    }
    
    pub fn save_thumbnail_to_cache(color_image: &egui::ColorImage, cache_path: &Path, original_path: &Path) {
        // Convert egui::ColorImage back to image format for caching
        let [width, height] = color_image.size;
        
        let pixels: Vec<u8> = color_image.pixels.iter()
            .flat_map(|p| [p.r(), p.g(), p.b(), p.a()])
            .collect();
        
        if let Some(img_buffer) = image::RgbaImage::from_raw(
            width as u32, 
            height as u32, 
            pixels
        ) {
            let dynamic_img = image::DynamicImage::ImageRgba8(img_buffer);
            
            // Create parent directory if it doesn't exist
            if let Some(parent) = cache_path.parent() {
                let _ = fs::create_dir_all(parent);
            }
            
            // Save with freedesktop.org thumbnail metadata
            Self::save_thumbnail_with_metadata(&dynamic_img, cache_path, original_path);
        }
    }
    
    pub fn save_thumbnail_with_metadata(img: &image::DynamicImage, cache_path: &Path, original_path: &Path) {
        // Get file metadata for thumbnail spec compliance (currently unused but could be added later)
        let _file_uri = format!("file://{}", 
            fs::canonicalize(original_path)
                .unwrap_or_else(|_| original_path.to_path_buf())
                .to_string_lossy()
        );
        
        let _file_size = fs::metadata(original_path)
            .map(|m| m.len())
            .unwrap_or(0);
            
        let _mtime = fs::metadata(original_path)
            .and_then(|m| m.modified())
            .map(|t| t.duration_since(SystemTime::UNIX_EPOCH).unwrap_or_default().as_secs())
            .unwrap_or(0);
        
        // Create PNG encoder with metadata
        use std::io::BufWriter;
        use std::fs::File;
        
        if let Ok(file) = File::create(cache_path) {
            let writer = BufWriter::new(file);
            let encoder = image::codecs::png::PngEncoder::new(writer);
            
            // Convert to RGB for PNG encoding
            let rgb_img = img.to_rgb8();
            
            if let Err(e) = encoder.write_image(
                rgb_img.as_raw(),
                img.width(),
                img.height(),
                image::ColorType::Rgb8,
            ) {
                eprintln!("Failed to save thumbnail for {:?}: {}", original_path, e);
            }
        }
        
        // Add metadata using external PNG tools would be ideal, but for now this basic save works
        // The important part is using the correct hash and cache location
    }
    
    pub fn fast_thumbnail_generation(path: &Path, size: u32) -> Option<egui::ColorImage> {
        // Use image reader with auto format detection
        match image::io::Reader::open(path) {
            Ok(reader) => {
                let reader = reader.with_guessed_format().ok()?;
                
                // Check if we can get dimensions without full decode
                if let Ok(img) = reader.decode() {
                    // Quickly check if image is much larger than needed
                    let (width, height) = (img.width(), img.height());
                    
                    // Skip resizing if image is already small
                    if width <= size * SMALL_IMAGE_THRESHOLD && height <= size * SMALL_IMAGE_THRESHOLD {
                        return Self::create_thumbnail_fast(img, size);
                    }
                    
                    // For large images, use a two-step resize for better performance
                    let intermediate_size = size * INTERMEDIATE_SIZE_MULTIPLIER;
                    if width > intermediate_size || height > intermediate_size {
                        let intermediate = img.resize(intermediate_size, intermediate_size, FilterType::Nearest);
                        Self::create_thumbnail_fast(intermediate, size)
                    } else {
                        Self::create_thumbnail_fast(img, size)
                    }
                } else {
                    None
                }
            }
            Err(e) => {
                eprintln!("Failed to load image {:?}: {}", path, e);
                None
            }
        }
    }
    
    pub fn create_thumbnail_fast(img: image::DynamicImage, size: u32) -> Option<egui::ColorImage> {
        // Use fastest resize algorithm for thumbnails
        let thumbnail = img.resize(size, size, FilterType::Nearest);
        let rgba = thumbnail.to_rgba8();
        let (width, height) = (thumbnail.width() as usize, thumbnail.height() as usize);
        
        Some(egui::ColorImage::from_rgba_unmultiplied(
            [width, height],
            rgba.as_raw(),
        ))
    }
    
    pub fn process_thumbnail_results(&mut self, ctx: &egui::Context) {
        while let Ok((index, color_image)) = self.thumbnail_receiver.try_recv() {
            let texture = ctx.load_texture(
                format!("thumbnail_{}", index),
                color_image,
                egui::TextureOptions::default(),
            );
            
            if let Ok(mut images) = self.images.write() {
                if index < images.len() {
                    images[index].thumbnail = Some(texture);
                    images[index].loading = false;
                }
            }
        }
    }
    
    pub fn preload_batch(&mut self, indices: &[usize]) {
        // Preload first few thumbnails when folder opens
        for &index in indices.iter().take(DEFAULT_PRELOAD_COUNT) {
            let images_len = self.images.read().map(|images| images.len()).unwrap_or(0);
            if index >= images_len {
                continue;
            }
            
            let (should_load, path) = {
                if let Ok(mut images) = self.images.write() {
                    if images[index].thumbnail.is_some() || images[index].loading {
                        continue;
                    }
                    images[index].loading = true;
                    (true, images[index].path.clone())
                } else {
                    continue;
                }
            };
            
            if should_load {
                let sender = self.thumbnail_sender.clone();
                let size = self.args.thumbnail_size;
                let cache_dir = self.cache_dir.clone();
                let debug = self.args.debug;
                
                self.thread_pool.spawn(move || {
                    if let Some(color_image) = Self::load_or_generate_thumbnail(&path, size, &cache_dir, debug) {
                        let _ = sender.send((index, color_image));
                    }
                });
            }
        }
    }
    
    pub fn set_background(&self, path: &Path) -> anyhow::Result<()> {
        let command_parts: Vec<&str> = self.args.command.split_whitespace().collect();
        if command_parts.is_empty() {
            return Err(anyhow::anyhow!("Empty command"));
        }
        
        let mut cmd = Command::new(command_parts[0]);
        cmd.args(&command_parts[1..]);
        cmd.arg(path);
        
        let output = cmd.output()?;
        if !output.status.success() {
            return Err(anyhow::anyhow!(
                "Command failed: {}",
                String::from_utf8_lossy(&output.stderr)
            ));
        }
        
        Ok(())
    }
    
    pub fn save_selected_image(&self, path: &Path) -> anyhow::Result<()> {
        if let Some(parent) = self.args.selected_image_file.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&self.args.selected_image_file, path.to_string_lossy().as_bytes())?;
        Ok(())
    }
}

impl eframe::App for BackgroundPickerApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.process_thumbnail_results(ctx);
        
        egui::CentralPanel::default().show(ctx, |ui| {
            if self.loading {
                ui.centered_and_justified(|ui| {
                    ui.label("Scanning for images...");
                });
                return;
            }
            
            ui.heading("Background Picker");
            ui.separator();
            
            egui::ScrollArea::vertical().show(ui, |ui| {
                let folders: Vec<String> = self.folder_tree.keys().cloned().collect();
                for folder in folders {
                    let image_indices: Vec<usize> = self.folder_tree.get(&folder)
                        .cloned()
                        .unwrap_or_default();
                    
                    let folder_label = if folder == "." { 
                        format!("Root ({} images)", image_indices.len())
                    } else { 
                        format!("{} ({} images)", folder, image_indices.len())
                    };
                    
                    let header_response = egui::CollapsingHeader::new(folder_label)
                        .default_open(false)
                        .show(ui, |ui| {
                            ui.horizontal_wrapped(|ui| {
                                for index in &image_indices {
                                    self.load_thumbnail(ctx, *index);
                                    
                                    let (_has_thumbnail, is_loading, path, relative_path, texture_ref) = {
                                        if let Ok(images) = self.images.read() {
                                            if *index >= images.len() {
                                                continue;
                                            }
                                            (
                                                images[*index].thumbnail.is_some(),
                                                images[*index].loading,
                                                images[*index].path.clone(),
                                                images[*index].relative_path.clone(),
                                                images[*index].thumbnail.clone()
                                            )
                                        } else {
                                            continue;
                                        }
                                    };
                                    
                                    if let Some(texture) = texture_ref {
                                        let image_button = egui::ImageButton::new(&texture)
                                            .frame(true);
                                        
                                        let button_response = ui.add(image_button);
                                        if button_response.clicked() {
                                            if let Err(e) = self.set_background(&path) {
                                                eprintln!("Failed to set background: {}", e);
                                            } else {
                                                let _ = self.save_selected_image(&path);
                                                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                                            }
                                        }
                                        
                                        button_response.on_hover_text(&relative_path);
                                    } else {
                                        // Show placeholder for loading images
                                        let size = self.args.thumbnail_size as f32;
                                        let (rect, response) = ui.allocate_exact_size(
                                            egui::Vec2::splat(size),
                                            egui::Sense::hover()
                                        );
                                        ui.painter().rect_filled(
                                            rect,
                                            egui::Rounding::same(5.0),
                                            egui::Color32::LIGHT_GRAY
                                        );
                                        
                                        let loading_text = if is_loading { "Loading..." } else { "Click to load" };
                                        ui.painter().text(
                                            rect.center(),
                                            egui::Align2::CENTER_CENTER,
                                            loading_text,
                                            egui::FontId::default(),
                                            egui::Color32::DARK_GRAY
                                        );
                                        response.on_hover_text(&relative_path);
                                    }
                                }
                            });
                        });
                    
                    // If folder was just opened, preload some thumbnails
                    if let Some(body_response) = header_response.body_response {
                        if body_response.rect.height() > 0.0 {
                            self.preload_batch(&image_indices);
                        }
                    }
                }
            });
        });
        
        ctx.request_repaint(); // Keep updating to process thumbnail results
    }
    
}

pub fn is_image_file(path: &Path) -> bool {
    if let Some(ext) = path.extension() {
        IMAGE_EXTENSIONS.contains(&ext.to_string_lossy().to_lowercase().as_str())
    } else {
        false
    }
}

pub fn validate_command(command: &str) -> anyhow::Result<()> {
    let command_parts: Vec<&str> = command.split_whitespace().collect();
    if command_parts.is_empty() {
        return Err(anyhow::anyhow!("Empty command"));
    }
    Ok(())
}