use eframe::egui;
use egui::ViewportBuilder;
use gui::Downloader;
use std::sync::{Arc, Mutex};
use tokio::runtime::Runtime;

fn main() -> Result<(), eframe::Error> {
    let options = eframe::NativeOptions {
        viewport: ViewportBuilder::default()
            .with_inner_size([400.0, 300.0])
            .with_always_on_top(),
        ..Default::default()
    };
    eframe::run_native(
        "Downloader",
        options,
        Box::new(|_cc| Ok(Box::new(App::default()))),
    )
}

struct App {
    runtime: Arc<Runtime>,
    downloader: Arc<Mutex<Option<Downloader>>>,
    url: String,
}

impl Default for App {
    fn default() -> Self {
        Self {
            runtime: Arc::new(Runtime::new().unwrap()),
            downloader: Arc::new(Mutex::new(None)),
            url: String::new(),
        }
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ctx, |ui| {
            match self.downloader.lock().unwrap().as_mut() {
                Some(downloader) => {
                    if downloader.running() {
                        ui.label(format!("Progress: {}%", downloader.progress()));
                    } else {
                        ctx.request_repaint();
                        match self.runtime.block_on(downloader.join()) {
                            Ok(()) => {
                                ui.label("Download complete!");
                            }
                            Err(e) => {
                                ui.label(format!("Error: {}", e));
                            }
                        }
                        self.downloader.lock().unwrap().take();
                    }
                }
                _ => {
                    ui.horizontal(|ui| {
                        ui.label("URL:");
                        ui.text_edit_singleline(&mut self.url);
                    });

                    if ui.button("Start").clicked() {
                        let runtime = self.runtime.clone();
                        let downloader = self.downloader.clone();
                        let url = self.url.clone();
                        let ctx = ctx.clone();
                        runtime.spawn(async move {
                            match Downloader::new(&url).await {
                                Ok(mut d) => {
                                    d.start();
                                    downloader.lock().unwrap().replace(d);
                                }
                                Err(e) => {
                                    ctx.request_repaint();
                                    egui::CentralPanel::default().show(&ctx, |ui| {
                                        ui.label(format!("Error: {}", e));
                                    });
                                    downloader.lock().unwrap().take();
                                }
                            }
                        });
                    }
                }
            }

            ctx.request_repaint_after(std::time::Duration::from_millis(100));
        });
    }
}
