use gui::Downloader;
use tokio::io::AsyncBufReadExt;
use tokio::io::BufReader;

#[tokio::main]
async fn main() {
    let url = "https://example.com/file.txt";
    let output_file = "downloaded_file.txt";

    let downloader = Downloader::new();
    let download_task = downloader.start_download(url, output_file).await;

    let stdin = tokio::io::stdin();
    let mut reader = BufReader::new(stdin).lines();

    println!("输入命令: (start/pause/exit)");
    while let Ok(Some(line)) = reader.next_line().await {
        match line.trim() {
            "start" => downloader.start().await,
            "pause" => downloader.pause().await,
            "exit" => {
                println!("退出程序...");
                break;
            }
            _ => println!("无效命令，请输入 start/pause/exit"),
        }
    }

    // 等待下载任务完成
    if let Err(e) = download_task.await {
        eprintln!("下载任务出错: {:?}", e);
    }
}
