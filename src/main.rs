#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
use std::process::exit;

use arboard::Clipboard;
use eframe::egui;
use egui::{Color32, Frame, ProgressBar, ViewportBuilder};
use stupidownload::Downloader;
use tokio::runtime::Runtime;
use tokio::sync::watch::Receiver;

fn main() -> Result<(), eframe::Error> {
    let options = eframe::NativeOptions {
        viewport: ViewportBuilder::default()
            .with_inner_size([500.0, 10.0])
            .with_resizable(false)
            .with_always_on_top()
            .with_decorations(false)
            .with_taskbar(false)
            .with_transparent(true),
        ..Default::default()
    };
    eframe::run_native(
        "Downloader",
        options,
        Box::new(|_cc| Ok(Box::new(App::default()))),
    )
}

struct App {
    _runtime: Runtime,
    downloader: Downloader,
    tracer: Receiver<u64>,
}

impl Default for App {
    fn default() -> Self {
        let runtime = Runtime::new().unwrap();
        let (tracer, downloader) = runtime.block_on(async {
            let mut downloader = Downloader::new(&Clipboard::new().unwrap().get_text().unwrap())
                .await
                .unwrap();
            downloader.start();
            (downloader.watcher(), downloader)
        });
        Self {
            _runtime: runtime,
            downloader,
            tracer,
        }
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default()
            .frame(Frame::NONE)
            .show(ctx, |ui| {
                if self.downloader.running() {
                    let progress = self.tracer.borrow().clone();
                    ui.add(
                        ProgressBar::new(progress as f32 / 100.0)
                            .fill(Color32::TRANSPARENT)
                            .show_percentage()
                            .animate(true),
                    );
                } else {
                    exit(0)
                }
            });
    }
}
