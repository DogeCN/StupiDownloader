use std::io::Write;

use gui::Downloader;

#[tokio::main]
async fn main() {
    let url = "https://github.com/DogeCN/Plume-Lexicon/releases/download/v1.15.3/Plume-Lexicon.exe";

    match Downloader::new(url).await {
        Ok(mut downloader) => {
            let tracer = downloader.start();
            while tracer.running() {
                print!("\r下载进度 {}%", tracer.progress());
                std::io::stdout().flush().unwrap();
                tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
            }
            match downloader.join().await {
                Ok(()) => println!("\r下载成功"),
                Err(e) => eprintln!("\r下载失败 {:}", e),
            }
        }
        Err(e) => eprintln!("初始化失败 {:}", e),
    }
}
