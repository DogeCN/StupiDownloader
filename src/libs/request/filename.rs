use percent_encoding::percent_decode_str;
use reqwest::header::CONTENT_DISPOSITION;
use std::path::Path;

pub fn filename_from(response: &reqwest::Response) -> String {
    response
        .headers()
        .get(CONTENT_DISPOSITION)
        .and_then(|v| v.to_str().ok())
        .and_then(parse)
        .or_else(|| {
            Path::new(response.url().path())
                .file_name()
                .and_then(|s| s.to_str())
                .map(|s| percent_decode_str(s).decode_utf8_lossy().into_owned())
        })
        .unwrap_or("Download".to_owned())
        .chars()
        .map(|c| match c {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '_',
            _ => c,
        })
        .collect::<String>()
        .trim()
        .trim_start_matches('.')
        .to_owned()
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
