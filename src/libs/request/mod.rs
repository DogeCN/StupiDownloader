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
        task::JoinError,
    },
};

#[derive(Error, Debug)]
pub enum DownloadError {
    #[error("HTTP request failed: {0}")]
    HttpRequest(#[from] reqwest::Error),

    #[error("File operation failed: {0}")]
    IO(#[from] std::io::Error),

    #[error("\tChunk {0} failed: {1}")]
    ChunkStatus(u64, String),

    #[error("Chunk download failed:\n{0}")]
    ChunkFailure(String),

    #[error("Task join failed: {0}")]
    Join(#[from] JoinError),
}

pub async fn download(url: &str) -> Result<(), DownloadError> {
    let client = Client::new();
    let response = client.head(url).header(AGENT.0, AGENT.1).send().await?;
    let output = filename_from(&response);
    let total_size = response
        .headers()
        .get(reqwest::header::CONTENT_LENGTH)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse().ok())
        .unwrap_or(0);
    let tracer = Arc::new(AtomicU64::new(0));
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

    File::create(&output).await?.set_len(total_size).await?;

    let producers = iter((0..total_chunk).map(|i| {
        let client = &client;
        let output = &output;
        let tracer = &tracer;
        async move {
            let start = i * chunk_size;
            let end = (i == total_chunk - 1)
                .then_some(total_size)
                .unwrap_or((i + 1) * chunk_size - 1);
            let response = client
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
                    tracer.fetch_add(size_of_val(&chunk) as u64, Ordering::Relaxed);
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
