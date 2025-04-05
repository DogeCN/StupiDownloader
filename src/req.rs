#![allow(dead_code)]
use reqwest::header::RANGE;
use tokio::io::AsyncSeekExt;
use tokio::{fs::File, io::AsyncWriteExt, sync::watch};

#[derive(Debug, Clone, PartialEq)]
pub enum TaskState {
    Running(u8),   // 包含当前进度（百分比）
    Paused,        // 暂停状态
    Finished,      // 下载完成状态
    Error(String), // 包含错误信息
}

pub struct Downloader {
    notifier: watch::Sender<TaskState>, // 用于任务状态管理
}

impl Downloader {
    /// 创建一个新的 Downloader 实例
    pub fn new() -> Self {
        Self {
            notifier: watch::Sender::new(TaskState::Running(0)),
        }
    }

    /// 启动下载任务
    pub async fn start_download(
        &self,
        url: &str,
        output_file: &str,
    ) -> tokio::task::JoinHandle<()> {
        let notifier = self.notifier.subscribe();
        let url = url.to_string();
        let output_file = output_file.to_string();

        tokio::spawn(async move {
            if let Err(e) = download_with_control(&url, &output_file, notifier).await {
                eprintln!("下载任务出错: {:?}", e);
            }
        })
    }

    /// 自动开始下载任务
    pub fn start(&self) {
        let _ = self.notifier.send(TaskState::Running(0)); // 初始进度为 0%
    }

    /// 暂停下载任务
    pub fn pause(&self) {
        let _ = self.notifier.send(TaskState::Paused);
    }

    /// 获取当前任务状态
    pub fn get_state(&self) -> TaskState {
        self.notifier.borrow().clone()
    }
}

async fn download_with_control(
    url: &str,
    output_file: &str,
    notifier: watch::Sender<TaskState>,
) -> Result<(), Box<dyn std::error::Error>> {
    let client = reqwest::Client::new();

    // 检查本地文件是否存在以及已下载的大小
    let local_file_size = tokio::fs::metadata(output_file)
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
        let _ = notifier.send(TaskState::Finished);
        return Ok(());
    }

    // 设置 Range 请求头以支持断点续传
    let mut request = client.get(url);
    if local_file_size > 0 {
        request = request.header(RANGE, format!("bytes={}-", local_file_size));
    }

    let mut response = request.send().await?;
    if response.status().is_success() {
        let mut file = if local_file_size > 0 {
            // 如果文件已存在，打开文件并移动到末尾
            let mut f = File::options().append(true).open(output_file).await?;
            f.seek(tokio::io::SeekFrom::End(0)).await?;
            f
        } else {
            // 如果文件不存在，创建新文件
            File::create(output_file).await?
        };

        let mut downloaded_bytes = local_file_size;

        while let Some(chunk) = response.chunk().await? {
            // 检查任务状态
            while let TaskState::Paused = *notifier.borrow() {
                notifier.subscribe().changed().await?;
            }

            // 写入文件
            file.write_all(&chunk).await?;
            downloaded_bytes += chunk.len() as u64;

            // 更新任务状态为运行，并发送当前进度
            let progress = (downloaded_bytes * 100 / total_size) as u8;
            let _ = notifier.send(TaskState::Running(progress));
        }

        println!("文件下载完成: {}", output_file);
        let _ = notifier.send(TaskState::Finished);
    } else {
        let error_message = format!("下载失败，状态码: {}", response.status());
        let _ = notifier.send(TaskState::Error(error_message));
    }

    Ok(())
}
