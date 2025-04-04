#![allow(dead_code)]
use tokio::{fs::File, io::AsyncWriteExt, sync::watch};

#[derive(Debug, Clone, PartialEq)]
pub enum TaskState {
    Running,
    Paused,
}

pub struct Downloader {
    notifier: watch::Sender<TaskState>, // 直接使用 watch::Sender 存储状态
}

impl Downloader {
    /// 创建一个新的 Downloader 实例
    pub fn new() -> Self {
        Self {
            notifier: watch::Sender::new(TaskState::Paused),
        }
    }

    /// 启动下载任务
    pub async fn start_download(
        &self,
        url: &str,
        output_file: &str,
    ) -> tokio::task::JoinHandle<()> {
        let rx = self.notifier.subscribe(); // 订阅状态变化
        let url = url.to_string();
        let output_file = output_file.to_string();

        tokio::spawn(async move {
            if let Err(e) = download_with_control(&url, &output_file, rx).await {
                eprintln!("下载任务出错: {:?}", e);
            }
        })
    }

    /// 设置任务状态为运行
    pub fn start(&self) {
        let _ = self.notifier.send(TaskState::Running); // 更新状态为 Running
    }

    /// 设置任务状态为暂停
    pub fn pause(&self) {
        let _ = self.notifier.send(TaskState::Paused); // 更新状态为 Paused
    }
}

async fn download_with_control(
    url: &str,
    output_file: &str,
    mut rx: watch::Receiver<TaskState>, // 使用 watch::Receiver 监听状态
) -> Result<(), Box<dyn std::error::Error>> {
    let client = reqwest::Client::new();
    let mut response = client.get(url).send().await?;

    if response.status().is_success() {
        let mut file = File::create(output_file).await?;

        while let Some(chunk) = response.chunk().await? {
            // 检查任务状态
            if *rx.borrow() == TaskState::Paused {
                rx.changed().await.unwrap(); // 等待状态变化
            }

            // 写入文件
            file.write_all(&chunk).await?;
        }

        println!("文件下载完成: {}", output_file);
    } else {
        eprintln!("下载失败，状态码: {}", response.status());
    }

    Ok(())
}
