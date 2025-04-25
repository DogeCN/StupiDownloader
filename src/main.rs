#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
use arboard::Clipboard;
use eframe::{App, egui};
use egui::{Pos2, ProgressBar, ViewportBuilder};
use std::process::exit;
use stupidownloader::Downloader;
use tokio::runtime::{Builder, Runtime};
use tokio::sync::watch::Receiver;

fn main() -> Result<(), eframe::Error> {
    let options = eframe::NativeOptions {
        viewport: ViewportBuilder::default()
            .with_resizable(false)
            .with_always_on_top()
            .with_decorations(false)
            .with_taskbar(false)
            .with_transparent(true)
            .with_inner_size([400.0, 18.0])
            .with_position(Pos2::ZERO),
        ..Default::default()
    };
    eframe::run_native(
        "StupiDownloader",
        options,
        Box::new(|_cc| Ok(Box::new(StupidApp::default()))),
    )
}

struct StupidApp {
    _runtime: Runtime,
    downloader: Downloader,
    tracer: Receiver<u64>,
}

impl Default for StupidApp {
    fn default() -> Self {
        let runtime = Builder::new_multi_thread()
            .worker_threads(16)
            .enable_all()
            .build()
            .unwrap();
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

impl App for StupidApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::Area::new("area".into())
            .fixed_pos(Pos2::ZERO)
            .show(ctx, |ui| {
                if self.downloader.running() {
                    ui.add(
                        ProgressBar::new(self.tracer.borrow().clone() as f32 / 100.0)
                            .show_percentage()
                            .animate(true),
                    );
                } else {
                    exit(0)
                }
            });

        ctx.request_repaint_after(std::time::Duration::from_millis(100));
    }
    fn clear_color(&self, _: &egui::Visuals) -> [f32; 4] {
        [0.0, 0.0, 0.0, 0.0]
    }
}
