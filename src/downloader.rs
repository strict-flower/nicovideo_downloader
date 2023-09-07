use crate::Error;
use blockingqueue::BlockingQueue;
use futures_util::StreamExt;
use reqwest::header::{ORIGIN, REFERER};
use reqwest::Client;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::fs;
use tokio::io::AsyncWriteExt;
use tokio::time::{sleep, Duration};

async fn download_ts(client: &Client, url: &str, outpath: &Path) -> Result<(), Error> {
    let res = client
        .get(url)
        .header(REFERER, "https://www.nicovideo.jp")
        .header(ORIGIN, "https://www.nicovideo.jp")
        .send()
        .await?;

    if res.content_length().unwrap() < 100 {
        return Err(Error::DownloadError);
    }

    let mut stream = res.bytes_stream();

    let mut f = fs::File::create(outpath).await?;

    while let Some(item) = stream.next().await {
        let bytes = item?;
        f.write_all(bytes.as_ref()).await?;
    }

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
    temp_dir: PathBuf,
}

impl TSDownloadRunner {
    async fn run(self) -> Result<(), Error> {
        loop {
            let url = self.queue_url.pop();
            let base_url = url.split('?').next().unwrap();
            let filename = base_url.split('/').last().unwrap();
            let outpath = self.temp_dir.join(filename);
            if outpath.as_path().exists() {
                println!("[{}] Already Downloaded", filename);
                self.queue_result.push(filename.to_string());
            } else {
                println!("[{}] Start download", filename);
                if let Err(Error::DownloadError) =
                    download_ts(&self.client, &url, outpath.as_path()).await
                {
                    println!("[{}] Download error: retry...", filename);
                    self.queue_url.push(url);
                } else {
                    println!("[{}] Done", filename);
                    self.queue_result.push(filename.to_string());
                }
            }
            sleep(Duration::from_secs(3)).await;
        }
    }
}

pub async fn download_master_playlist(
    master_m3u8_url: &str,
    dest_dir_name: &str,
) -> Result<Vec<String>, Error> {
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

    let temp_dir = Path::new(dest_dir_name);
    if !Path::exists(temp_dir) {
        fs::create_dir(temp_dir).await?;
    }

    let queue_result = BlockingQueue::new();
    let queue_url = BlockingQueue::new();
    let mut threads = vec![];
    let mut downloaded = vec![];

    for i in 0..4 {
        let runner = TSDownloadRunner {
            queue_result: queue_result.clone(),
            queue_url: queue_url.clone(),
            client: client.clone(),
            temp_dir: temp_dir.to_path_buf(),
        };
        threads.push(tokio::spawn(async move {
            sleep(Duration::from_secs(i)).await;
            runner.run().await
        }));
    }

    for url in &urls {
        queue_url.push(url.clone());
    }

    while downloaded.len() < urls.len() {
        let v = queue_result.pop();
        downloaded.push(v);
    }

    for thread in threads {
        thread.abort();
    }

    downloaded.sort_by_key(|x| x.split('.').next().unwrap().parse::<i32>().unwrap());

    println!("[+] Downloaded: {:?}", downloaded);

    Ok(downloaded)
}
