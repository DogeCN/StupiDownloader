use gui::download;

#[tokio::main]
async fn main() {
    if let Err(e) = download(
        "https://zenlayer.dl.sourceforge.net/project/winmerge/stable/2.16.46/WinMerge-2.16.46-x64-Setup.exe",
    )
    .await
    {
        eprintln!("下载失败: {:}", e);
    } else {
        println!("下载成功");
    }
}
