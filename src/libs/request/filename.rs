use percent_encoding::percent_decode_str;
use reqwest::header::{CONTENT_DISPOSITION, CONTENT_TYPE};
use std::path::Path;

pub fn filename_from(response: &reqwest::Response) -> String {
    let headers = response.headers();

    // 1. 尝试从Content-Disposition头获取文件名
    let filename = headers
        .get(CONTENT_DISPOSITION)
        .and_then(|v| v.to_str().ok())
        .and_then(parse);

    // 2. 尝试从URL路径获取文件名
    let path = Path::new(response.url().path());
    let [stem, ext] = [path.file_stem(), path.extension()].map(|s| {
        s.and_then(|s| s.to_str())
            .map(|s| percent_decode_str(s).decode_utf8_lossy().into())
    });

    // 3. 确定基础名称
    let base_name = filename.or(stem).unwrap_or("Download".to_owned());

    // 4. 获取扩展名
    let ext = headers
        .get(CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .and_then(|ct| ct.split('/').last())
        .and_then(|s| s.split(';').next())
        .or(ext.as_deref())
        .unwrap_or("bin");

    format!(
        "{}.{}",
        clear(&base_name),
        clear(ext)
    )
}

fn parse(header: &str) -> Option<String> {
    header.split(';').find_map(|part| {
        let part = part.trim();
        if part.starts_with("filename*=") {
            part.splitn(3, '\'')
                .nth(2)
                .map(|s| percent_decode_str(s).decode_utf8_lossy().into_owned())
        } else if part.starts_with("filename=") {
            Some(
                percent_decode_str(part[9..].trim_matches(|c| c == '"' || c == ' '))
                    .decode_utf8_lossy()
                    .into(),
            )
        } else {
            None
        }
    })
}

fn clear(name: &str) -> String {
    name.chars()
        .map(|c| match c {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '_',
            _ => c,
        })
        .collect::<String>()
        .trim()
        .trim_start_matches('.')
        .to_owned()
}
