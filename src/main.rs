use gui::download;

#[tokio::main]
async fn main() {
    if let Err(e) = download(
        "https://jaist.dl.sourceforge.net/project/winmerge/stable/2.16.46/winmerge-2.16.46-full-src.7z?viasf=1",
    )
    .await
    {
        eprintln!("下载失败: {:}", e);
    } else {
        println!("下载成功");
    }
}
