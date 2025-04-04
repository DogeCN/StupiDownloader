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
    tracer: watch::Sender<u8>,
}

impl Downloader {
    pub fn new() -> Self {
        Self {
            notifier: watch::Sender::new(TaskState::Running),
            tracer: watch::Sender::new(0),
        }
    }

    pub async fn start_download(
        &self,
        url: &str,
        output_file: &str,
    ) -> tokio::task::JoinHandle<()> {
        let notifier = self.notifier.subscribe();
        let tracer = self.tracer.clone();
        let url = url.to_owned();
        let output_file = output_file.to_owned();

        tokio::spawn(async move {
            if let Err(e) = download_with_control(&url, &output_file, notifier, tracer).await {
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

    pub fn progress(&self) -> u8 {
        *self.tracer.borrow()
    }
}

async fn download_with_control(
    url: &str,
    output: &str,
    mut notifier: watch::Receiver<TaskState>,
    tracer: watch::Sender<u8>,
) -> Result<(), Box<dyn std::error::Error>> {
    let client = reqwest::Client::new();

    // 检查本地文件是否存在以及已下载的大小
    let local_file_size = tokio::fs::metadata(output)
        .await
        .map(|metadata| metadata.len())
        .unwrap_or(0);

    // 获取服务器文件的总大小
    let total_size = {
        let head_response = client.head(url).send().await?;
        if head_response.status().is_success() {
            head_response
                .headers()
                .get(reqwest::header::CONTENT_LENGTH)
                .and_then(|v| v.to_str().ok()?.parse::<u64>().ok())
                .unwrap_or(0)
        } else {
            0
        }
    };

    // 校验本地文件是否完整
    if local_file_size >= total_size && total_size > 0 {
        println!("文件已完整下载，无需重新下载");
        return Ok(());
    }

    // 设置 Range 请求头以支持断点续传
    let mut request = client.get(url);
    if local_file_size > 0 {
        request = request.header(RANGE, format!("bytes={}-", local_file_size));
    }

    let mut response = request.send().await?;
    if response.status().is_success() || response.status() == reqwest::StatusCode::PARTIAL_CONTENT {
        let mut file = if local_file_size > 0 {
            // 如果文件已存在，打开文件并移动到末尾
            let mut f = File::options().append(true).open(output).await?;
            f.seek(tokio::io::SeekFrom::End(0)).await?;
            f
        } else {
            // 如果文件不存在，创建新文件
            File::create(output).await?
        };

        let mut downloaded_bytes = local_file_size;

        while let Some(chunk) = response.chunk().await? {
            // 检查任务状态
            if *notifier.borrow() == TaskState::Paused {
                notifier.changed().await.unwrap();
            }

            // 写入文件
            file.write_all(&chunk).await?;
            downloaded_bytes += chunk.len() as u64;

            // 更新已下载字节数
            let _ = tracer.send((downloaded_bytes * 100 / total_size) as u8);
        }

        println!("文件下载完成: {}", output);
    } else {
        eprintln!("下载失败，状态码: {}", response.status());
    }

    Ok(())
}
