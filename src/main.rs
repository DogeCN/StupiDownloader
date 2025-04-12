#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
use arboard::Clipboard;
use eframe::egui;
use egui::{ProgressBar, ViewportBuilder};
use gui::Downloader;
use tokio::runtime::Runtime;
use tokio::sync::watch::Receiver;

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
    tracer: Receiver<u64>,
}

impl Default for App {
    fn default() -> Self {
        Self {
            tracer: Runtime::new().unwrap().block_on(async {
                let mut downloader =
                    Downloader::new(&Clipboard::new().unwrap().get_text().unwrap())
                        .await
                        .unwrap();
                downloader.start();
                downloader.watcher()
            }),
        }
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ctx, |ui| {
            let progress = self.tracer.borrow().clone();
            ui.add(ProgressBar::new(progress as f32 / 100.0).text(format!("{:}%", progress)));
        });
    }
}
