use crate::Error;
use blockingqueue::BlockingQueue;
use futures_util::StreamExt;
use reqwest::header::{ORIGIN, REFERER};
use reqwest::Client;
use std::fs;
use std::io::Write;
use std::path::Path;
use std::sync::Arc;

async fn download_ts(client: &Client, url: &str, outpath: &Path) -> Result<(), Error> {
    let res = client
        .get(url)
        .header(REFERER, "https://www.nicovideo.jp")
        .header(ORIGIN, "https://www.nicovideo.jp")
        .send()
        .await?;
    let mut stream = res.bytes_stream();

    let mut f = fs::File::create(outpath)?;
    while let Some(item) = stream.next().await {
        let bytes = item?;
        f.write_all(bytes.as_ref())?;
    }
    f.sync_all()?;

    Ok(())
}

async fn get(client: &Client, url: &str) -> Result<String, Error> {
    let response = client
        .get(url)
        .header(REFERER, "https://www.nicovideo.jp")
        .header(ORIGIN, "https://www.nicovideo.jp")
        .send()
        .await?;
    let ret = response.text().await?;
    Ok(ret)
}

struct TSDownloadRunner {
    client: Arc<Client>,
    queue_result: BlockingQueue<String>,
    queue_url: BlockingQueue<String>,
}

impl TSDownloadRunner {
    async fn run(self) -> Result<(), Error> {
        loop {
            let url = self.queue_url.pop();
            let base_url = url.split('?').next().unwrap();
            let filename = base_url.split('/').last().unwrap();
            let outpath = Path::new("tmp").join(filename);
            println!("[+] Downloading: {}", filename);
            download_ts(&self.client, &url, outpath.as_path()).await?;
            self.queue_result
                .push(outpath.as_path().to_str().unwrap().to_string());
        }
    }
}

pub async fn download_master_playlist(master_m3u8_url: &str, filename: &str) -> Result<(), Error> {
    let client = Arc::new(Client::new());
    let master_m3u8 = get(&client, master_m3u8_url).await?;
    let playlist_m3u8_path = master_m3u8
        .split('\n')
        .filter(|x| x.contains("playlist.m3u8"))
        .last()
        .unwrap();
    let prefix = master_m3u8_url
        .split("master.m3u8")
        .filter(|x| x.contains("dmc.nico"))
        .last()
        .unwrap();
    let playlist = m3u8_rs::parse_media_playlist_res(
        get(&client, &format!("{}{}", prefix, playlist_m3u8_path))
            .await?
            .as_bytes(),
    )
    .unwrap();
    let prefix_video = format!(
        "{}/{}",
        prefix,
        playlist_m3u8_path.split("playlist.m3u8").next().unwrap()
    );
    let mut urls = Vec::new();
    for segment in playlist.segments {
        urls.push(format!("{}{}", prefix_video, segment.uri));
    }

    println!("{:?}", urls);

    fs::create_dir("tmp")?;

    let queue_result = BlockingQueue::new();
    let queue_url = BlockingQueue::new();
    let mut threads = vec![];

    for _ in 0..4 {
        let runner = TSDownloadRunner {
            queue_result: queue_result.clone(),
            queue_url: queue_url.clone(),
            client: client.clone(),
        };
        threads.push(tokio::spawn(async move { runner.run().await }));
    }

    for url in &urls {
        queue_url.push(url.clone());
    }

    let mut downloaded = vec![];

    while downloaded.len() < urls.len() {
        let v = queue_result.pop();
        downloaded.push(v);
    }

    for thread in threads {
        thread.abort();
    }

    for file in downloaded {
        let fpath = Path::new(&file);
        fs::remove_file(fpath)?;
    }

    fs::remove_dir("tmp")?;

    Ok(())
}
