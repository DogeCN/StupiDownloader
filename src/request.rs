#![allow(dead_code)]
use {
    std::sync::Arc,
    tokio::{
        fs::File,
        io::AsyncWriteExt,
        sync::{watch, Mutex},
    },
};

#[derive(Debug, Clone, PartialEq)]
pub enum TaskState {
    Running,
    Paused,
}

pub struct Downloader {
    state: Arc<Mutex<TaskState>>,
    notifier: watch::Sender<TaskState>,
}

impl Downloader {
    /// 创建一个新的 Downloader 实例
    pub fn new() -> Self {
        Self {
            state: Mutex::new(TaskState::Paused).into(),
            notifier: watch::Sender::new(TaskState::Paused),
        }
    }

    /// 启动下载任务
    pub async fn start_download(
        &self,
        url: &str,
        output_file: &str,
    ) -> tokio::task::JoinHandle<()> {
        let state = self.state.clone();
        let rx = self.notifier.subscribe();
        let url = url.to_string();
        let output_file = output_file.to_string();

        tokio::spawn(async move {
            if let Err(e) = download_with_control(&url, &output_file, state, rx).await {
                eprintln!("下载任务出错: {:?}", e);
            }
        })
    }

    /// 设置任务状态为运行
    pub async fn start(&self) {
        *self.state.lock().await = TaskState::Running;
        let _ = self.notifier.send(TaskState::Running);
    }

    /// 设置任务状态为暂停
    pub async fn pause(&self) {
        *self.state.lock().await = TaskState::Paused;
        let _ = self.notifier.send(TaskState::Paused);
    }
}

async fn download_with_control(
    url: &str,
    output_file: &str,
    state: Arc<Mutex<TaskState>>,
    mut notifier: watch::Receiver<TaskState>,
) -> Result<(), Box<dyn std::error::Error>> {
    let client = reqwest::Client::new();
    let mut response = client.get(url).send().await?;

    if response.status().is_success() {
        let mut file = File::create(output_file).await?;

        while let Some(chunk) = response.chunk().await? {
            // 检查任务状态
            if let TaskState::Paused = state.lock().await.clone() {
                notifier.changed().await.unwrap(); // 等待状态变化
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
