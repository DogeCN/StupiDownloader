use gui::download;

#[tokio::main]
async fn main() {
    if let Err(e) = download(
        "https://github.com/DogeCN/Plume-Lexicon/releases/download/v1.15.3/Plume-Lexicon.exe",
    )
    .await
    {
        eprintln!("下载失败: {:}", e);
    } else {
        println!("下载成功");
    }
}
