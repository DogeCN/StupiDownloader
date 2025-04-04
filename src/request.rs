#![allow(dead_code)]
use reqwest::header::RANGE;
use tokio::io::AsyncSeekExt;
use tokio::{fs::File, io::AsyncWriteExt, sync::watch};

#[derive(Debug, Clone, PartialEq)]
pub enum TaskState {
    Running,
    Paused,
}

pub struct Downloader {
    notifier: watch::Sender<TaskState>,
    downloaded_bytes: watch::Sender<u64>,
}

impl Downloader {
    pub fn new() -> Self {
        Self {
            notifier: watch::Sender::new(TaskState::Paused),
            downloaded_bytes: watch::Sender::new(0),
        }
    }

    pub async fn start_download(
        &self,
        url: &str,
        output_file: &str,
    ) -> tokio::task::JoinHandle<()> {
        let state_rx = self.notifier.subscribe();
        let bytes_tx = self.downloaded_bytes.clone();
        let url = url.to_string();
        let output_file = output_file.to_string();

        tokio::spawn(async move {
            if let Err(e) = download_with_control(&url, &output_file, state_rx, bytes_tx).await {
                eprintln!("下载任务出错: {:?}", e);
            }
        })
    }

    pub fn start(&self) {
        let _ = self.notifier.send(TaskState::Running);
    }

    pub fn pause(&self) {
        let _ = self.notifier.send(TaskState::Paused);
    }

    pub fn get_downloaded_bytes(&self) -> u64 {
        *self.downloaded_bytes.borrow()
    }
}

async fn download_with_control(
    url: &str,
    output_file: &str,
    mut state_rx: watch::Receiver<TaskState>,
    bytes_tx: watch::Sender<u64>,
) -> Result<(), Box<dyn std::error::Error>> {
    let client = reqwest::Client::new();

    let local_file_size = tokio::fs::metadata(output_file)
        .await
        .map(|metadata| metadata.len())
        .unwrap_or(0);

    let mut request = client.get(url);
    if local_file_size > 0 {
        request = request.header(RANGE, format!("bytes={}-", local_file_size));
    }

    let mut response = request.send().await?;
    if response.status().is_success() || response.status() == reqwest::StatusCode::PARTIAL_CONTENT {
        let mut file = if local_file_size > 0 {
            let mut f = File::options().append(true).open(output_file).await?;
            f.seek(tokio::io::SeekFrom::End(0)).await?;
            f
        } else {
            File::create(output_file).await?
        };

        let mut downloaded_bytes = local_file_size;

        while let Some(chunk) = response.chunk().await? {
            if *state_rx.borrow() == TaskState::Paused {
                state_rx.changed().await.unwrap();
            }

            file.write_all(&chunk).await?;
            downloaded_bytes += chunk.len() as u64;

            let _ = bytes_tx.send(downloaded_bytes);
        }

        println!("文件下载完成: {}", output_file);
    } else {
        eprintln!("下载失败，状态码: {}", response.status());
    }

    Ok(())
}
