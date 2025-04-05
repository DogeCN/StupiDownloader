mod consts;
mod filename;
use {
    consts::*,
    filename::filename_from,
    futures_util::stream::{iter, StreamExt},
    reqwest::{Client, Response},
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
    Io(#[from] std::io::Error),

    #[error("Server does not support range requests")]
    RangeNotSupported,

    #[error("Chunk download failed: {0}")]
    ChunkFailure(String),

    #[error("Merge failed: {0}")]
    MergeFailure(String),

    #[error("Task join failed: {0}")]
    Join(#[from] JoinError),
}

pub async fn download(url: &str) -> Result<(), DownloadError> {
    let client = Client::new();
    let response = client.head(url).header(AGENT.0, AGENT.1).send().await?;
    let output = filename_from(&response);
    let total_size = size_of(&response)?;
    let total_chunk = fits(total_size);
    let chunk_size = total_size / total_chunk;

    File::create(&output).await?.set_len(total_size).await?;

    let producers = iter((0..total_chunk).map(|i| async {
        let start = i * chunk_size;
        let response = client
            .get(url)
            .header(AGENT.0, AGENT.1)
            .header(
                "Range",
                format!(
                    "bytes={}-{}",
                    start,
                    (i == total_chunk - 1)
                        .then_some(total_size - 1)
                        .unwrap_or((i + 1) * chunk_size - 1)
                ),
            )
            .send()
            .await?;
        if !response.status().is_success() {
            return Err(DownloadError::ChunkFailure(format!(
                "Invalid status code: {}",
                response.status()
            )));
        }
        let mut file = BufWriter::new({
            let mut file = OpenOptions::new().write(true).open(output).await?;
            file.seek(std::io::SeekFrom::Start(start)).await?;
            file
        });
        let mut stream = response.bytes_stream();
        while let Some(chunk) = stream.next().await {
            file.write_all(&chunk?).await?;
        }
        Ok(())
    }))
    .buffer_unordered(8);

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

fn fits(total: u64) -> u64 {
    SIZE_TABLE
        .iter()
        .find_map(|&(size, num)| (size > total).then_some(num))
        .unwrap()
}

fn size_of(resp: &Response) -> Result<u64, DownloadError> {
    match resp.headers().get("Accept-Ranges").map(|v| v.as_bytes()) {
        Some(b"bytes") => Ok(resp
            .headers()
            .get(reqwest::header::CONTENT_LENGTH)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.parse().ok())
            .unwrap_or(0)),
        _ => Err(DownloadError::RangeNotSupported),
    }
}
