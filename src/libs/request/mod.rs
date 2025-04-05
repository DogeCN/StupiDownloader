mod consts;
mod filename;
use {
    consts::*,
    filename::filename_from,
    futures_util::stream::{iter, StreamExt},
    reqwest::{Client, Response},
    std::{
        path::{Path, PathBuf},
        sync::atomic::{AtomicUsize, Ordering},
    },
    thiserror::Error,
    tokio::{
        fs::{remove_file, File, OpenOptions},
        io::{AsyncSeekExt, AsyncWriteExt, BufWriter},
        sync::{mpsc, Semaphore},
        task::{JoinError, JoinSet},
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
    let total_size = get_total_size(&response)?;
    let total_tasks = calculate_concurrent_tasks();
    let total_chunk = calculate_optimal_chunks(total_size);
    let chunks = calculate_chunk_ranges(total_size, total_chunk);

    // 创建临时文件列表
    let temps: Vec<_> = (0..total_chunk)
        .map(|i| PathBuf::from(format!("{}.part{}", output, i)))
        .collect();

    // 使用多级并行流水线
    let (notifier, mut tracer) = mpsc::channel::<(usize, PathBuf)>(total_chunk);
    let state = std::sync::Arc::new(AtomicUsize::new(0));

    // 生产者：并行下载分块
    let producers = iter(chunks.into_iter().zip(&temps).enumerate())
        .map(|(id, ((start, end), path))| {
            let client = client.clone();
            let url = url.to_owned();
            let path = path.clone();
            let notifier = notifier.clone();
            let state = state.clone();
            let semaphore = std::sync::Arc::new(Semaphore::new(total_tasks));

            async move {
                let _ = semaphore.acquire().await;
                state.fetch_add(1, Ordering::Relaxed);

                let result = download_chunk(&client, &url, &path, start, end).await;

                if result.is_ok() {
                    let _ = notifier.send((id, path)).await;
                }
                state.fetch_sub(1, Ordering::Relaxed);
                result
            }
        })
        .buffer_unordered(total_tasks);

    // 消费者：并行合并分块
    let consumer = tokio::spawn({
        let output = output.clone();
        async move {
            let mut mix = JoinSet::new();

            // 预创建输出文件
            let _ = OpenOptions::new()
                .create(true)
                .write(true)
                .open(&output)
                .await?;

            while let Some((_, path)) = tracer.recv().await {
                let output = output.clone();
                mix.spawn(async move {
                    let mut out = OpenOptions::new().write(true).open(&output).await?;

                    let mut chunk = File::open(&path).await?;
                    let pos = out.stream_position().await?;
                    out.seek(std::io::SeekFrom::Start(pos)).await?;

                    tokio::io::copy(&mut chunk, &mut out).await?;

                    drop(chunk);
                    remove_file(&path).await?;

                    Ok::<_, std::io::Error>(pos)
                });
            }

            // 等待所有合并任务完成
            let mut results = Vec::new();
            while let Some(res) = mix.join_next().await {
                results.push(res??);
            }

            // 检查是否所有分块都已合并
            if results.len() != total_chunk {
                return Err(DownloadError::MergeFailure(format!(
                    "Missing chunks, expected {} got {}",
                    total_chunk,
                    results.len()
                )));
            }

            Ok(())
        }
    });

    // 执行下载并收集结果
    let results: Vec<Result<(), DownloadError>> =
        producers.collect::<Vec<_>>().await.into_iter().collect();

    // 检查下载错误
    let errors: Vec<String> = results
        .into_iter()
        .filter_map(|r| r.err().map(|e| e.to_string()))
        .collect();

    if !errors.is_empty() {
        return Err(DownloadError::ChunkFailure(errors.join("\n")));
    }

    // 等待合并完成
    consumer.await??;

    Ok(())
}

/// 自适应并发控制
fn calculate_concurrent_tasks() -> usize {
    let num_cpus = num_cpus::get();
    match num_cpus {
        1..=2 => 2,
        3..=4 => 4,
        5..=8 => 8,
        _ => 16.min(32), // 不超过32个并发
    }
}

/// 优化后的分块下载实现
async fn download_chunk(
    client: &Client,
    url: &str,
    path: &Path,
    start: u64,
    end: u64,
) -> Result<(), DownloadError> {
    let response = client
        .get(url)
        .header(AGENT.0, AGENT.1)
        .header("Range", format!("bytes={}-{}", start, end))
        .send()
        .await?;

    if !response.status().is_success() {
        return Err(DownloadError::ChunkFailure(format!(
            "Invalid status code: {}",
            response.status()
        )));
    }

    let mut file = BufWriter::new(File::create(path).await?);
    let mut stream = response.bytes_stream();

    while let Some(chunk) = stream.next().await {
        file.write_all(&chunk?).await?;
    }

    file.flush().await?;
    Ok(())
}

/// 智能分块计算 (优化版)
fn calculate_optimal_chunks(total_size: u64) -> usize {
    const SIZE_TABLE: [(u64, usize); 6] = [
        (MB, 1),        // 1MB
        (10 * MB, 2),   // 10MB
        (100 * MB, 4),  // 100MB
        (GB, 8),        // 1GB
        (10 * GB, 16),  // 10GB
        (u64::MAX, 32), // 10+GB
    ];

    let base_chunks = SIZE_TABLE
        .iter()
        .find_map(|&(size, chunks)| (total_size > size).then_some(chunks))
        .unwrap();

    // 考虑CPU核心数
    base_chunks.max(num_cpus::get()).min(64) // 不超过64个分块
}

fn get_total_size(resp: &Response) -> Result<u64, DownloadError> {
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
