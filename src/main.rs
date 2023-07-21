use crate::api_data::ApiData;
use crate::nicovideo::NicoVideo;
use std::env;
use std::fmt;
use std::path::Path;
use std::process;

mod api_data;
mod downloader;
mod nicovideo;

#[derive(Debug)]
pub enum Error {
    ReqwestError(reqwest::Error),
    IOError(std::io::Error),
}

impl fmt::Display for Error {
    fn fmt(self: &Error, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::ReqwestError(reqwest_error) => write!(f, "{}", reqwest_error),
            Error::IOError(io_error) => write!(f, "{}", io_error),
        }
    }
}

impl std::error::Error for Error {}

impl From<reqwest::Error> for Error {
    fn from(err: reqwest::Error) -> Self {
        Error::ReqwestError(err)
    }
}

impl From<std::io::Error> for Error {
    fn from(err: std::io::Error) -> Self {
        Error::IOError(err)
    }
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
        let outfile = format!("{}.mp4", sanitize_filename::sanitize(api_data.video.title));
        downloader::download_master_playlist(&master_m3u8_url, &outfile).await?;
        nv.stop_heartbeat();
    }
    Ok(())
}
