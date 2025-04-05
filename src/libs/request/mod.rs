mod filename;
use {
    filename::filename_from,
    futures_util::{future, StreamExt},
    reqwest::{Client, Response},
    std::path::{Path, PathBuf},
    thiserror::Error,
    tokio::{
        fs::{remove_file, File},
        io::AsyncWriteExt,
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
}

pub async fn download(url: &str) -> Result<(), DownloadError> {
    let client = Client::new();
    let response = client.head(url).send().await?;
    let output = filename_from(&response);
    let total_size = get_total_size(&response)?;
    let chunk_num = calculate_optimal_chunks(total_size);
    let chunks = calculate_chunk_ranges(total_size, chunk_num);
    let temps: Vec<_> = (0..chunk_num)
        .map(|i| PathBuf::from(format!("{}.part{}", output, i)))
        .collect();

    // 并行下载
    let tasks = chunks.into_iter().zip(&temps).map(|((start, end), path)| {
        let client = client.clone();
        let url = url.to_owned();
        let path = path.clone();

        tokio::spawn(async move { download_chunk(&client, &url, &path, start, end).await })
    });

    // 收集结果
    let results = future::join_all(tasks).await;
    let mut errors = Vec::new();

    for (i, result) in results.into_iter().enumerate() {
        match result {
            Ok(Ok(())) => {}
            Ok(Err(e)) => errors.push(format!("分块 {} 失败: {}", i, e)),
            Err(e) => errors.push(format!("分块 {} 运行时错误: {:?}", i, e)),
        }
    }

    if !errors.is_empty() {
        return Err(DownloadError::ChunkFailure(errors.join("\n")));
    }

    merge_chunks(&output, &temps).await?;
    cleanup(&temps).await?;
    Ok(())
}

/// 分块下载实现
async fn download_chunk(
    client: &Client,
    url: &str,
    path: &Path,
    start: u64,
    end: u64,
) -> Result<(), DownloadError> {
    let response = client
        .get(url)
        .header("Range", format!("bytes={}-{}", start, end))
        .send()
        .await?;

    if !response.status().is_success() {
        return Err(DownloadError::ChunkFailure(format!(
            "无效状态码: {}",
            response.status()
        )));
    }

    let mut file = File::create(path).await?;
    let mut stream = response.bytes_stream();

    while let Some(chunk) = stream.next().await {
        file.write_all(&chunk?).await?;
    }

    Ok(())
}

/// 智能分块计算
fn calculate_optimal_chunks(total_size: u64) -> usize {
    match total_size {
        0..1_048_576 => 1,                // <1MB
        1_048_576..10_485_760 => 2,       // <10MB
        10_485_760..104_857_600 => 4,     // <100MB
        104_857_600..=1_073_741_824 => 8, // <1GB
        _ => 16,                          // >1GB
    }
}

fn get_total_size(resp: &Response) -> Result<u64, DownloadError> {
    // 检查Range支持
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

fn calculate_chunk_ranges(total_size: u64, num_chunks: usize) -> Vec<(u64, u64)> {
    let chunk_size = total_size / num_chunks as u64;
    (0..num_chunks)
        .map(|i| {
            let start = i as u64 * chunk_size;
            let end = if i == num_chunks - 1 {
                total_size - 1
            } else {
                (i + 1) as u64 * chunk_size - 1
            };
            (start, end)
        })
        .collect()
}

async fn merge_chunks(output: &str, temps: &[PathBuf]) -> Result<(), DownloadError> {
    let mut output = File::create(output).await?;

    for path in temps {
        let mut chunk = File::open(path).await?;
        tokio::io::copy(&mut chunk, &mut output).await?;
    }

    Ok(())
}

async fn cleanup(temps: &[PathBuf]) -> Result<(), DownloadError> {
    for path in temps {
        if path.exists() {
            remove_file(path).await?;
        }
    }
    Ok(())
}
