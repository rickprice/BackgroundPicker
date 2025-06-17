use clap::Parser;
use eframe::egui;
use image::imageops::FilterType;
use image::ImageEncoder;
use rayon::prelude::*;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, RwLock};
use std::fs;
use std::io::{self, Write};
use std::time::SystemTime;
use walkdir::WalkDir;

#[derive(Debug, thiserror::Error)]
pub enum BackgroundPickerError {
    #[error("Failed to create thread pool: {0}")]
    ThreadPoolCreation(#[from] rayon::ThreadPoolBuildError),
    
    #[error("Failed to create thumbnail cache directory: {0}")]
    CacheDirectoryCreation(#[from] std::io::Error),
    
    #[error("Failed to generate thumbnail for {path}: {source}")]
    ThumbnailGeneration {
        path: PathBuf,
        source: Box<dyn std::error::Error + Send + Sync>,
    },
    
    #[error("Failed to save selected image path: {0}")]
    SaveSelectedImage(std::io::Error),
    
    #[error("Command execution failed: {0}")]
    CommandExecution(String),
    
    #[error("Invalid image file: {0}")]
    InvalidImageFile(PathBuf),
    
    #[error("Lock acquisition failed")]
    LockAcquisition,
}

pub type Result<T> = std::result::Result<T, BackgroundPickerError>;

const IMAGE_EXTENSIONS: &[&str] = &["jpg", "jpeg", "png", "gif", "bmp", "webp"];
const DEFAULT_PRELOAD_COUNT: usize = 8;
const CHUNK_SIZE: usize = 100;
const MIN_THREAD_COUNT: usize = 4;
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
    pub folder_tree: HashMap<String, Vec<usize>>,
    pub loading: bool,
    pub thumbnail_sender: std::sync::mpsc::Sender<(usize, egui::ColorImage)>,
    pub thumbnail_receiver: std::sync::mpsc::Receiver<(usize, egui::ColorImage)>,
    pub thread_pool: rayon::ThreadPool,
    pub cache_dir: PathBuf,
}

impl BackgroundPickerApp {
    pub fn new(_cc: &eframe::CreationContext<'_>, args: Args) -> Result<Self> {
        let (thumbnail_sender, thumbnail_receiver) = std::sync::mpsc::channel();
        
        // Create thread pool with optimal number of threads
        let thread_pool = rayon::ThreadPoolBuilder::new()
            .num_threads(num_cpus::get().max(MIN_THREAD_COUNT))
            .build()?;
        
        // Set up thumbnail cache directory (freedesktop.org spec)
        let cache_dir = Self::get_thumbnail_cache_dir()?;
        if args.debug {
            println!("Using thumbnail cache directory: {:?}", cache_dir);
        }
        
        let mut app = Self {
            args,
            images: Arc::new(RwLock::new(Vec::new())),
            folder_tree: HashMap::new(),
            loading: true,
            thumbnail_sender,
            thumbnail_receiver,
            thread_pool,
            cache_dir,
        };
        
        app.scan_images()?;
        
        if app.args.pregenerate {
            app.pregenerate_all_thumbnails()?;
            // Exit after pregeneration, don't show GUI
            std::process::exit(0);
        }
        
        Ok(app)
    }
    
    pub fn get_thumbnail_cache_dir() -> Result<PathBuf> {
        // Use freedesktop.org thumbnail specification
        let cache_home = dirs::cache_dir()
            .or_else(|| dirs::home_dir().map(|h| h.join(".cache")))
            .unwrap_or_else(|| PathBuf::from(".cache"));
            
        let normal_dir = cache_home.join("thumbnails").join("normal");
        
        // Create the directory structure if it doesn't exist
        fs::create_dir_all(&normal_dir)
            .map_err(BackgroundPickerError::CacheDirectoryCreation)?;
        
        Ok(normal_dir)
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
    
    
    
    pub fn scan_images(&mut self) -> Result<()> {
        let base_path = &self.args.directory;
        
        // Clear existing data
        {
            let mut images = self.images.write()
                .map_err(|_| BackgroundPickerError::LockAcquisition)?;
            images.clear();
        }
        self.folder_tree.clear();
        
        if self.args.debug {
            println!("Scanning directory: {:?}", base_path);
        }
        
        // Pre-allocate collections to avoid repeated reallocations
        let mut temp_images = Vec::new();
        let mut temp_folders: HashMap<String, Vec<usize>> = HashMap::new();
        
        // Collect all image files first
        for entry in WalkDir::new(&self.args.directory)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_type().is_file())
        {
            if let Some(ext) = entry.path().extension() {
                let ext_str = ext.to_string_lossy();
                if IMAGE_EXTENSIONS.iter().any(|&valid_ext| valid_ext.eq_ignore_ascii_case(&ext_str)) {
                    let relative_path = entry.path()
                        .strip_prefix(base_path)
                        .map(|p| p.to_string_lossy().into_owned())
                        .unwrap_or_else(|_| entry.path().to_string_lossy().into_owned());
                    
                    let folder = entry.path()
                        .parent()
                        .and_then(|p| p.strip_prefix(base_path).ok())
                        .map(|p| p.to_string_lossy().into_owned())
                        .unwrap_or_else(|| ".".to_owned());
                    
                    let image_index = temp_images.len();
                    temp_images.push(ImageInfo {
                        path: entry.path().to_path_buf(),
                        thumbnail: None,
                        relative_path,
                        loading: false,
                    });
                    
                    temp_folders
                        .entry(folder)
                        .or_default()
                        .push(image_index);
                }
            }
        }
        
        // Update the main data structures
        {
            let mut images = self.images.write()
                .map_err(|_| BackgroundPickerError::LockAcquisition)?;
            *images = temp_images;
        }
        self.folder_tree = temp_folders;
        
        if self.args.debug {
            println!("Found {} images in {} folders", 
                self.images.read().map(|i| i.len()).unwrap_or(0), 
                self.folder_tree.len());
        }
        
        self.loading = false;
        Ok(())
    }
    
    pub fn pregenerate_all_thumbnails(&mut self) -> Result<()> {
        let total_images = self.images.read()
            .map_err(|_| BackgroundPickerError::LockAcquisition)?
            .len();
        
        if total_images == 0 {
            if self.args.debug {
                println!("No images found to pregenerate thumbnails for");
            }
            return Ok(());
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
        let cache_dir = &self.cache_dir;
        let size = self.args.thumbnail_size;
        let debug = self.args.debug;
        let images = Arc::clone(&self.images);
        
        let results: Vec<(bool, bool)> = (0..total_images)
            .collect::<Vec<_>>()
            .par_chunks(CHUNK_SIZE) // Process in chunks for progress reporting
            .enumerate()
            .flat_map(|(chunk_idx, chunk)| {
                let chunk_results: Vec<(bool, bool)> = chunk.par_iter().map(|&index| {
                    let path = {
                        match images.read() {
                            Ok(images_guard) => {
                                if index >= images_guard.len() {
                                    return (false, false); // (was_cached, was_generated)
                                }
                                images_guard[index].path.clone()
                            }
                            Err(_) => return (false, false),
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
                    
                    if let Some(cache_path) = Self::get_cached_thumbnail_path_static(&abs_path, cache_dir) {
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
                        if let Some(cache_path) = Self::get_cached_thumbnail_path_static(&abs_path, cache_dir) {
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
        
        Ok(())
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
        
        // Pre-allocate vector with exact capacity for better performance
        let mut pixels = Vec::with_capacity(color_image.pixels.len() * 4);
        for pixel in &color_image.pixels {
            pixels.extend_from_slice(&[pixel.r(), pixel.g(), pixel.b(), pixel.a()]);
        }
        
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
        let reader = image::io::Reader::open(path).ok()?
            .with_guessed_format().ok()?;
        
        // Try to get dimensions first to avoid full decode if possible
        let img = reader.decode().ok()?;
        let (width, height) = (img.width(), img.height());
        
        // Early return for already small images
        if width <= size && height <= size {
            return Self::create_thumbnail_fast(img, size);
        }
        
        // Calculate optimal resize strategy based on image size
        let scale_factor = (width.max(height) as f32 / size as f32).max(1.0);
        
        if scale_factor > 8.0 {
            // For very large images, use three-step resize for better quality/performance balance
            let first_step = (size as f32 * 4.0) as u32;
            let second_step = (size as f32 * 2.0) as u32;
            
            let step1 = img.resize(first_step, first_step, FilterType::Nearest);
            let step2 = step1.resize(second_step, second_step, FilterType::Triangle);
            Self::create_thumbnail_fast(step2, size)
        } else if scale_factor > 4.0 {
            // For large images, use two-step resize
            let intermediate_size = size * 2;
            let intermediate = img.resize(intermediate_size, intermediate_size, FilterType::Nearest);
            Self::create_thumbnail_fast(intermediate, size)
        } else {
            // For moderately sized images, direct resize with higher quality filter
            Self::create_thumbnail_fast(img, size)
        }
    }
    
    pub fn create_thumbnail_fast(img: image::DynamicImage, size: u32) -> Option<egui::ColorImage> {
        // Use fastest resize algorithm for thumbnails
        let thumbnail = img.resize(size, size, FilterType::Nearest);
        let rgba = thumbnail.to_rgba8();
        let (width, height) = (thumbnail.width() as usize, thumbnail.height() as usize);
        
        // Pre-allocate the pixel buffer for better performance
        let pixel_count = width * height;
        let raw_pixels = rgba.as_raw();
        
        if raw_pixels.len() != pixel_count * 4 {
            return None; // Safety check
        }
        
        Some(egui::ColorImage::from_rgba_unmultiplied(
            [width, height],
            raw_pixels,
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
        let images_len = match self.images.read() {
            Ok(images) => images.len(),
            Err(_) => return,
        };
        
        // Collect paths that need loading to minimize lock time
        let mut paths_to_load = Vec::new();
        
        {
            let mut images = match self.images.write() {
                Ok(images) => images,
                Err(_) => return,
            };
            
            for &index in indices.iter().take(DEFAULT_PRELOAD_COUNT) {
                if index >= images_len {
                    continue;
                }
                
                if images[index].thumbnail.is_none() && !images[index].loading {
                    images[index].loading = true;
                    paths_to_load.push((index, images[index].path.clone()));
                }
            }
        }
        
        // Spawn loading tasks
        let sender = self.thumbnail_sender.clone();
        let size = self.args.thumbnail_size;
        let cache_dir = self.cache_dir.clone();
        let debug = self.args.debug;
        
        for (index, path) in paths_to_load {
            let sender = sender.clone();
            let cache_dir = cache_dir.clone();
            
            self.thread_pool.spawn(move || {
                if let Some(color_image) = Self::load_or_generate_thumbnail(&path, size, &cache_dir, debug) {
                    let _ = sender.send((index, color_image));
                }
            });
        }
    }
    
    pub fn set_background(&self, path: &Path) -> Result<()> {
        let command_parts: Vec<&str> = self.args.command.split_whitespace().collect();
        if command_parts.is_empty() {
            return Err(BackgroundPickerError::CommandExecution("Empty command".to_owned()));
        }
        
        let mut cmd = Command::new(command_parts[0]);
        cmd.args(&command_parts[1..]);
        cmd.arg(path);
        
        let output = cmd.output()
            .map_err(|e| BackgroundPickerError::CommandExecution(e.to_string()))?;
        
        if !output.status.success() {
            let error_msg = String::from_utf8_lossy(&output.stderr);
            return Err(BackgroundPickerError::CommandExecution(error_msg.into_owned()));
        }
        
        Ok(())
    }
    
    pub fn save_selected_image(&self, path: &Path) -> Result<()> {
        if let Some(parent) = self.args.selected_image_file.parent() {
            fs::create_dir_all(parent)
                .map_err(BackgroundPickerError::SaveSelectedImage)?;
        }
        
        let path_str = path.to_string_lossy();
        fs::write(&self.args.selected_image_file, path_str.as_bytes())
            .map_err(BackgroundPickerError::SaveSelectedImage)?;
        
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
                // Clone folder data to avoid borrowing issues
                let folders: Vec<(String, Vec<usize>)> = self.folder_tree.iter()
                    .map(|(k, v)| (k.clone(), v.clone()))
                    .collect();
                
                for (folder, image_indices) in folders {
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
                                    
                                    let image_info = {
                                        match self.images.read() {
                                            Ok(images) => {
                                                if *index >= images.len() {
                                                    continue;
                                                }
                                                // Clone the data we need
                                                (
                                                    images[*index].loading,
                                                    images[*index].path.clone(),
                                                    images[*index].relative_path.clone(),
                                                    images[*index].thumbnail.clone()
                                                )
                                            }
                                            Err(_) => continue,
                                        }
                                    };
                                    
                                    let (is_loading, path, relative_path, texture_ref) = image_info;
                                    
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
    path.extension()
        .and_then(|ext| ext.to_str())
        .map(|ext_str| IMAGE_EXTENSIONS.iter().any(|&valid_ext| valid_ext.eq_ignore_ascii_case(ext_str)))
        .unwrap_or(false)
}

pub fn validate_command(command: &str) -> Result<()> {
    // Check if command has any non-whitespace characters without allocating
    if command.trim().is_empty() {
        return Err(BackgroundPickerError::CommandExecution("Empty command".to_owned()));
    }
    Ok(())
}