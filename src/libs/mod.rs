mod consts;
mod filename;
use {
    consts::*,
    filename::filename_from,
    futures_util::stream::{iter, StreamExt},
    reqwest::Client,
    std::sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
    thiserror::Error,
    tokio::{
        fs::{File, OpenOptions},
        io::{AsyncSeekExt, AsyncWriteExt, BufWriter},
        sync::watch::{Receiver, Sender},
        task::{JoinError, JoinHandle},
    },
};

#[derive(Error, Debug)]
pub enum DownloadError {
    #[error("HTTP request failed: {0}")]
    HttpRequest(#[from] reqwest::Error),

    #[error("Invalid Response Header")]
    InvalidResponse,

    #[error("File operation failed: {0}")]
    IO(#[from] std::io::Error),

    #[error("\tChunk {0} failed: {1}")]
    ChunkStatus(u64, String),

    #[error("Chunk download failed:\n{0}")]
    ChunkFailure(String),

    #[error("Task join failed: {0}")]
    Join(#[from] JoinError),
}

#[derive(Clone)]
struct Tracer {
    pub total_size: u64,
    counter: Arc<AtomicU64>,
    sender: Sender<u64>,
}

impl Tracer {
    fn new(total_size: u64) -> Self {
        Self {
            total_size,
            counter: Arc::new(AtomicU64::new(0)),
            sender: Sender::new(0),
        }
    }

    fn add(&self, size: u64) {
        self.counter.fetch_add(size, Ordering::Relaxed);
        self.sender
            .send(self.counter.load(Ordering::Relaxed))
            .unwrap();
    }
}

pub struct Downloader {
    handle: Option<JoinHandle<Result<(), DownloadError>>>,
    client: Client,
    tracer: Tracer,
    pub url: String,
    pub output: String,
    pub total_chunk: u64,
}

impl Downloader {
    pub async fn new(url: &str) -> Result<Self, DownloadError> {
        let client = Client::builder().user_agent(UA).build()?;
        let response = client.head(url).send().await?;
        let output = filename_from(&response);
        let total_size = response
            .headers()
            .get(reqwest::header::CONTENT_LENGTH)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.parse().ok())
            .unwrap_or_default();
        if total_size <= 0 {
            return Err(DownloadError::InvalidResponse);
        }
        let total_chunk = match response
            .headers()
            .get("Accept-Ranges")
            .map(|v| v.as_bytes())
        {
            Some(b"bytes") => 1.max(total_size / MB),
            _ => 1,
        };
        Ok(Self {
            handle: None,
            client,
            tracer: Tracer::new(total_size),
            url: url.to_owned(),
            output: output.to_owned(),
            total_chunk,
        })
    }

    pub fn start(&mut self) {
        self.handle.replace(tokio::spawn(download(
            self.client.clone(),
            self.url.clone(),
            self.output.clone(),
            self.total_chunk,
            self.tracer.clone(),
        )));
    }

    pub fn watcher(&self) -> Receiver<u64> {
        self.tracer.sender.subscribe()
    }

    pub fn running(&self) -> bool {
        self.handle.as_ref().is_some_and(|h| !h.is_finished())
    }

    pub async fn join(&mut self) -> Result<(), DownloadError> {
        self.handle.take().unwrap().await?
    }
}

async fn download(
    client: Client,
    url: String,
    output: String,
    total_chunk: u64,
    tracer: Tracer,
) -> Result<(), DownloadError> {
    let total_size = tracer.total_size;
    File::create(&output).await?.set_len(total_size).await?;

    let producers = iter((0..total_chunk).map(|i| {
        let client = client.clone();
        let url = &url;
        let output = &output;
        let tracer = &tracer;
        async move {
            let start = i * MB;
            let end = (i == total_chunk - 1)
                .then_some(total_size)
                .unwrap_or((i + 1) * MB - 1);
            let response = client
                .get(url)
                .header("Range", format!("bytes={}-{}", start, end))
                .send()
                .await?;
            if response.status().is_success() {
                let mut file = BufWriter::new(OpenOptions::new().write(true).open(output).await?);
                file.seek(std::io::SeekFrom::Start(start)).await?;
                let mut stream = response.bytes_stream();
                while let Some(chunk) = stream.next().await {
                    let chunk = chunk?;
                    tracer.add(chunk.len() as u64);
                    file.write_all(&chunk).await?;
                }
                file.flush().await?;
                Ok(())
            } else {
                Err(DownloadError::ChunkStatus(i, response.status().to_string()))
            }
        }
    }))
    .buffer_unordered(32);

    let error: String = producers
        .collect::<Vec<_>>()
        .await
        .into_iter()
        .filter_map(|r| r.err().map(|e| e.to_string()))
        .collect();

    error
        .is_empty()
        .then_some(())
        .ok_or(DownloadError::ChunkFailure(error))
}
