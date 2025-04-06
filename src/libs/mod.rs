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
pub struct Tracer {
    counter: Arc<AtomicU64>,
    pub total_size: u64,
}

impl Tracer {
    fn new(total_size: u64) -> Self {
        Self {
            counter: Arc::new(AtomicU64::new(0)),
            total_size,
        }
    }

    fn update(&self, item: &[u8]) {
        self.counter
            .fetch_add(size_of_val(item) as u64, Ordering::Relaxed);
    }

    pub fn progress(&self) -> u64 {
        self.counter.load(Ordering::Relaxed) * 100 / self.total_size
    }

    pub fn running(&self) -> bool {
        self.counter.load(Ordering::Relaxed) < self.total_size
    }
}

pub struct Downloader {
    handle: Option<JoinHandle<Result<(), DownloadError>>>,
    pub url: String,
    pub output: String,
    pub chunk_size: u64,
    pub total_chunk: u64,
    pub tracer: Tracer,
}

impl Downloader {
    pub async fn new(url: &str) -> Result<Self, DownloadError> {
        let client = Client::new();
        let response = client.head(url).header(AGENT.0, AGENT.1).send().await?;
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
            Some(b"bytes") => SIZE_TABLE
                .iter()
                .find_map(|&(size, num)| (size > total_size).then_some(num))
                .unwrap(),
            _ => 1,
        };
        let chunk_size = total_size / total_chunk;
        Ok(Self {
            handle: None,
            url: url.to_owned(),
            output: output.to_owned(),
            chunk_size,
            total_chunk,
            tracer: Tracer::new(total_size),
        })
    }

    pub fn start(&mut self) -> &Tracer {
        self.handle.replace(tokio::spawn(download(
            self.url.clone(),
            self.output.clone(),
            self.chunk_size,
            self.total_chunk,
            self.tracer.clone(),
        )));
        &self.tracer
    }

    pub async fn join(&mut self) -> Result<(), DownloadError> {
        self.handle.take().unwrap().await?
    }
}

async fn download(
    url: String,
    output: String,
    chunk_size: u64,
    total_chunk: u64,
    tracer: Tracer,
) -> Result<(), DownloadError> {
    let total_size = tracer.total_size;

    File::create(&output).await?.set_len(total_size).await?;

    let producers = iter((0..total_chunk).map(|i| {
        let url = &url;
        let output = &output;
        let tracer = &tracer;
        async move {
            let start = i * chunk_size;
            let end = (i == total_chunk - 1)
                .then_some(total_size)
                .unwrap_or((i + 1) * chunk_size - 1);
            let response = Client::new()
                .get(url)
                .header(AGENT.0, AGENT.1)
                .header("Range", format!("bytes={}-{}", start, end))
                .send()
                .await?;
            if response.status().is_success() {
                let mut file = BufWriter::new({
                    let mut file = OpenOptions::new().write(true).open(output).await?;
                    file.seek(std::io::SeekFrom::Start(start)).await?;
                    file
                });
                let mut stream = response.bytes_stream();
                while let Some(chunk) = stream.next().await {
                    let chunk = chunk?;
                    tracer.update(&chunk);
                    file.write_all(&chunk).await?;
                }
                file.flush().await?;
                Ok(())
            } else {
                Err(DownloadError::ChunkStatus(i, response.status().to_string()))
            }
        }
    }))
    .buffer_unordered(12);

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
