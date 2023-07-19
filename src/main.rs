use crate::api_data::ApiData;
use crate::nicovideo::NicoVideo;
use reqwest::header::{ORIGIN, REFERER};
use reqwest::{Client, Error};
use std::env;
use std::path::Path;
use std::process;

mod api_data;
mod nicovideo;

async fn get(client: &Client, url: &str) -> Result<String, Error> {
    client
        .get(url)
        .header(REFERER, "https://www.nicovideo.jp")
        .header(ORIGIN, "https://www.nicovideo.jp")
        .send()
        .await?
        .text()
        .await
}

async fn do_download(master_m3u8_url: &str) -> Result<(), Error> {
    let client = Client::new();
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
    Ok(())
}

#[tokio::main]
async fn main() -> Result<(), Error> {
    let username = env::var("NV_USERNAME").unwrap_or("dummy".to_owned());
    let password = env::var("NV_PASSWORD").unwrap_or("dummy".to_owned());

    let targets = vec!["sm29247339"];
    let cookies_path = Path::new("cookies.json");
    let mut nv = NicoVideo::new(cookies_path).unwrap();
    if !nv.is_login().await? {
        println!("[+] Need login");
        nv.login(&username, &password).await?;
        if !nv.is_login().await? {
            println!("[-] Login failed");
            process::exit(1);
        }
    }
    println!("[+] Login OK");
    for target in targets {
        if !target.starts_with("sm") && !target.starts_with("nm") {
            println!("Video ID must start by 'sm' or 'nm'");
            continue;
        }
        let api_data: ApiData = nv.get_video_api_data(target).await?.unwrap();
        println!("[+] Title: {}", api_data.video.title);
        let master_m3u8_url = nv
            .create_session(target, &api_data.media.delivery.movie.session)
            .await?;
        println!("[+] master.m3u8 is here: {}", &master_m3u8_url);
        do_download(&master_m3u8_url).await?;
        nv.stop_heartbeat();
    }
    Ok(())
}
