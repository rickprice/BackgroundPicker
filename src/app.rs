use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::mpsc;

use eframe::egui;

use crate::cache;
use crate::cli::Args;
use crate::error::{BackgroundPickerError, Result};
use crate::scanner::{self, ImageInfo};
use crate::thumbnail;

const DEFAULT_PRELOAD_COUNT: usize = 8;
const CHUNK_SIZE: usize = 100;
const MIN_THREAD_COUNT: usize = 4;
const PROGRESS_THRESHOLD: usize = 50;

pub struct BackgroundPickerApp {
    pub args: Args,
    pub images: Vec<ImageInfo>,
    pub folder_tree: BTreeMap<String, Vec<usize>>,
    pub loading: bool,
    pub thumbnail_sender: mpsc::Sender<(usize, egui::ColorImage)>,
    pub thumbnail_receiver: mpsc::Receiver<(usize, egui::ColorImage)>,
    pub thread_pool: rayon::ThreadPool,
    pub cache_dir: PathBuf,
}

impl BackgroundPickerApp {
    /// # Errors
    ///
    /// Returns an error if the background thread pool or the thumbnail cache
    /// directory cannot be created.
    pub fn new(_cc: &eframe::CreationContext<'_>, args: Args) -> Result<Self> {
        let cache_dir = cache::thumbnail_cache_dir()?;
        if args.debug {
            println!("Using thumbnail cache directory: {}", cache_dir.display());
        }

        let mut app = Self::with_state(args, Vec::new(), cache_dir)?;
        app.scan_images();

        if app.args.pregenerate {
            app.pregenerate_all_thumbnails();
            std::process::exit(0);
        }

        Ok(app)
    }

    /// Constructs an app around already-known state, bypassing directory scanning.
    /// Used by [`Self::new`] and by tests that want to exercise behavior without
    /// touching the filesystem scan.
    ///
    /// # Errors
    ///
    /// Returns an error if the background thread pool cannot be created.
    pub fn with_state(args: Args, images: Vec<ImageInfo>, cache_dir: PathBuf) -> Result<Self> {
        let (thumbnail_sender, thumbnail_receiver) = mpsc::channel();
        Ok(Self {
            args,
            images,
            folder_tree: BTreeMap::new(),
            loading: false,
            thumbnail_sender,
            thumbnail_receiver,
            thread_pool: Self::build_thread_pool()?,
            cache_dir,
        })
    }

    fn build_thread_pool() -> Result<rayon::ThreadPool> {
        let num_threads = std::thread::available_parallelism()
            .map_or(MIN_THREAD_COUNT, |n| n.get().max(MIN_THREAD_COUNT));
        Ok(rayon::ThreadPoolBuilder::new()
            .num_threads(num_threads)
            .build()?)
    }

    pub fn scan_images(&mut self) {
        if self.args.debug {
            println!("Scanning directory: {}", self.args.directory.display());
        }

        let scanner::ScanResult {
            images,
            folder_tree,
        } = scanner::scan(&self.args.directory);

        if self.args.debug {
            println!(
                "Found {} images in {} folders",
                images.len(),
                folder_tree.len()
            );
        }

        self.images = images;
        self.folder_tree = folder_tree;
        self.loading = false;
    }

    pub fn pregenerate_all_thumbnails(&mut self) {
        let total_images = self.images.len();
        if total_images == 0 {
            if self.args.debug {
                println!("No images found to pregenerate thumbnails for");
            }
            return;
        }

        if self.args.debug {
            println!("Pre-generating thumbnails for {total_images} images...");
        } else {
            println!("Generating thumbnails for {total_images} images...");
        }

        let start_time = std::time::Instant::now();
        let cache_dir = self.cache_dir.clone();
        let size = self.args.thumbnail_size;
        let debug = self.args.debug;
        let paths: Vec<PathBuf> = self.images.iter().map(|image| image.path.clone()).collect();

        let (cached_count, generated_count) =
            Self::generate_thumbnails_in_parallel(&paths, size, &cache_dir, debug);

        let elapsed = start_time.elapsed();

        if self.args.debug {
            println!("Thumbnail pregeneration complete:");
            println!("  - {cached_count} thumbnails found in cache");
            println!("  - {generated_count} thumbnails generated");
            println!(
                "  - {} thumbnails failed",
                total_images - cached_count - generated_count
            );
            println!("  - Time elapsed: {:.2}s", elapsed.as_secs_f64());
        } else {
            println!(
                "Thumbnail generation complete: {cached_count} cached, {generated_count} generated ({:.1}s)",
                elapsed.as_secs_f64()
            );
        }
    }

    fn generate_thumbnails_in_parallel(
        paths: &[PathBuf],
        size: u32,
        cache_dir: &Path,
        debug: bool,
    ) -> (usize, usize) {
        use rayon::prelude::*;

        let total = paths.len();
        let results: Vec<(bool, bool)> = (0..total)
            .collect::<Vec<_>>()
            .par_chunks(CHUNK_SIZE)
            .enumerate()
            .flat_map(|(chunk_idx, chunk)| {
                let chunk_results: Vec<(bool, bool)> = chunk
                    .par_iter()
                    .map(|&index| {
                        Self::generate_one_thumbnail(&paths[index], index, size, cache_dir, debug)
                    })
                    .collect();

                if !debug && total > PROGRESS_THRESHOLD {
                    let completed = (chunk_idx + 1) * CHUNK_SIZE.min(total);
                    print!("\rProgress: {completed}/{total} images processed");
                    let _ = std::io::Write::flush(&mut std::io::stdout());
                }

                chunk_results
            })
            .collect();

        if !debug && total > PROGRESS_THRESHOLD {
            println!();
        }

        let cached_count = results.iter().filter(|(was_cached, _)| *was_cached).count();
        let generated_count = results
            .iter()
            .filter(|(_, was_generated)| *was_generated)
            .count();
        (cached_count, generated_count)
    }

    /// Returns `(was_cached, was_generated)` for a single image.
    #[allow(clippy::option_if_let_else)] // closures would obscure the debug-logging branches
    fn generate_one_thumbnail(
        path: &Path,
        index: usize,
        size: u32,
        cache_dir: &Path,
        debug: bool,
    ) -> (bool, bool) {
        let abs_path = fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
        let file_name = path.file_name().unwrap_or_default();

        if let Some(existing) = cache::find_existing_thumbnail(&abs_path) {
            if thumbnail::load_cached(&existing, size).is_some() {
                if debug {
                    println!("  [{}] Found existing thumbnail: {file_name:?}", index + 1);
                }
                return (true, false);
            }
        }

        let cache_path = cache::cached_thumbnail_path(&abs_path, cache_dir);
        if cache::is_cache_valid(&abs_path, &cache_path)
            && thumbnail::load_cached(&cache_path, size).is_some()
        {
            if debug {
                println!("  [{}] Found cached thumbnail: {file_name:?}", index + 1);
            }
            return (true, false);
        }

        if let Some(color_image) = thumbnail::generate(&abs_path, size) {
            thumbnail::save_to_cache(&color_image, &cache_path, &abs_path);
            if debug {
                println!("  [{}] Generated thumbnail: {file_name:?}", index + 1);
            }
            (false, true)
        } else {
            if debug {
                println!(
                    "  [{}] Failed to generate thumbnail: {file_name:?}",
                    index + 1
                );
            }
            (false, false)
        }
    }

    /// Kicks off background loading of a single thumbnail, if it isn't already
    /// loaded or in flight. `ctx` is cloned into the worker so it can wake the
    /// UI thread the moment the result is ready.
    pub fn load_thumbnail(&mut self, ctx: &egui::Context, index: usize) {
        let Some(image) = self.images.get_mut(index) else {
            return;
        };
        if image.thumbnail.is_some() || image.loading {
            return;
        }
        image.loading = true;
        let path = image.path.clone();

        self.spawn_thumbnail_load(ctx, index, path);
    }

    /// Kicks off background loading for the first few not-yet-loaded images in
    /// `indices`, typically called when a folder is expanded in the UI.
    pub fn preload_batch(&mut self, ctx: &egui::Context, indices: &[usize]) {
        for &index in indices.iter().take(DEFAULT_PRELOAD_COUNT) {
            let Some(image) = self.images.get_mut(index) else {
                continue;
            };
            if image.thumbnail.is_some() || image.loading {
                continue;
            }
            image.loading = true;
            let path = image.path.clone();

            self.spawn_thumbnail_load(ctx, index, path);
        }
    }

    fn spawn_thumbnail_load(&self, ctx: &egui::Context, index: usize, path: PathBuf) {
        let sender = self.thumbnail_sender.clone();
        let size = self.args.thumbnail_size;
        let cache_dir = self.cache_dir.clone();
        let debug = self.args.debug;
        let ctx = ctx.clone();

        self.thread_pool.spawn(move || {
            if let Some(color_image) = thumbnail::load_or_generate(&path, size, &cache_dir, debug) {
                if sender.send((index, color_image)).is_ok() {
                    ctx.request_repaint();
                }
            }
        });
    }

    pub fn process_thumbnail_results(&mut self, ctx: &egui::Context) {
        while let Ok((index, color_image)) = self.thumbnail_receiver.try_recv() {
            let texture = ctx.load_texture(
                format!("thumbnail_{index}"),
                color_image,
                egui::TextureOptions::default(),
            );

            if let Some(image) = self.images.get_mut(index) {
                image.thumbnail = Some(texture);
                image.loading = false;
            }
        }
    }

    /// # Errors
    ///
    /// Returns an error if `self.args.command` is empty or exits with a non-zero
    /// status.
    pub fn set_background(&self, path: &Path) -> Result<()> {
        let mut command_parts = self.args.command.split_whitespace();
        let program = command_parts
            .next()
            .ok_or_else(|| BackgroundPickerError::CommandExecution("Empty command".to_owned()))?;

        let output = Command::new(program)
            .args(command_parts)
            .arg(path)
            .output()
            .map_err(|e| BackgroundPickerError::CommandExecution(e.to_string()))?;

        if !output.status.success() {
            let error_msg = String::from_utf8_lossy(&output.stderr).into_owned();
            return Err(BackgroundPickerError::CommandExecution(error_msg));
        }

        Ok(())
    }

    /// # Errors
    ///
    /// Returns an error if the selected-image file (or its parent directory)
    /// cannot be written.
    pub fn save_selected_image(&self, path: &Path) -> Result<()> {
        if let Some(parent) = self.args.selected_image_file.parent() {
            fs::create_dir_all(parent).map_err(BackgroundPickerError::SaveSelectedImage)?;
        }

        fs::write(
            &self.args.selected_image_file,
            path.to_string_lossy().as_bytes(),
        )
        .map_err(BackgroundPickerError::SaveSelectedImage)
    }
}

impl eframe::App for BackgroundPickerApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.process_thumbnail_results(&ctx);

        egui::CentralPanel::default().show(ui, |ui| {
            if self.loading {
                ui.centered_and_justified(|ui| {
                    ui.label("Scanning for images...");
                });
                return;
            }

            ui.heading("Background Picker");
            ui.separator();

            egui::ScrollArea::vertical().show(ui, |ui| {
                // Move the folder tree out of `self` for the duration of the loop so we can
                // freely call `&mut self` methods (loading thumbnails) while iterating it,
                // without paying for a full clone of every frame's folder/index data.
                let folder_tree = std::mem::take(&mut self.folder_tree);

                for (folder, image_indices) in &folder_tree {
                    let folder_label = if folder.is_empty() {
                        format!("Root ({} images)", image_indices.len())
                    } else {
                        format!("{folder} ({} images)", image_indices.len())
                    };

                    let header_response = egui::CollapsingHeader::new(folder_label)
                        .default_open(false)
                        .show(ui, |ui| {
                            ui.horizontal_wrapped(|ui| {
                                for &index in image_indices {
                                    self.load_thumbnail(&ctx, index);
                                    self.show_thumbnail(ui, index);
                                }
                            });
                        });

                    if header_response
                        .body_response
                        .is_some_and(|r| r.rect.height() > 0.0)
                    {
                        self.preload_batch(&ctx, image_indices);
                    }
                }

                self.folder_tree = folder_tree;
            });
        });
    }
}

impl BackgroundPickerApp {
    #[allow(clippy::cast_precision_loss)] // thumbnail sizes are UI pixel dimensions, always tiny
    fn show_thumbnail(&self, ui: &mut egui::Ui, index: usize) {
        let Some(image) = self.images.get(index) else {
            return;
        };

        if let Some(texture) = image.thumbnail.as_ref() {
            let response = ui.add(egui::Image::new(texture).sense(egui::Sense::click()));
            if response.clicked() {
                let path = image.path.clone();
                if let Err(e) = self.set_background(&path) {
                    eprintln!("Failed to set background: {e}");
                } else {
                    let _ = self.save_selected_image(&path);
                    ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
                }
            }
            response.on_hover_text(&image.relative_path);
        } else {
            let loading = image.loading;
            let relative_path = image.relative_path.clone();
            let size = self.args.thumbnail_size as f32;

            let (rect, response) =
                ui.allocate_exact_size(egui::Vec2::splat(size), egui::Sense::hover());
            ui.painter()
                .rect_filled(rect, 5.0_f32, egui::Color32::LIGHT_GRAY);

            let loading_text = if loading {
                "Loading..."
            } else {
                "Click to load"
            };
            ui.painter().text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                loading_text,
                egui::FontId::default(),
                egui::Color32::DARK_GRAY,
            );
            response.on_hover_text(&relative_path);
        }
    }
}
