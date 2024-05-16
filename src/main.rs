use crate::api_data::ApiData;
use crate::downloader::NicoVideoDownloader;
use crate::nicovideo::NicoVideo;
use ffmpeg_cli::FfmpegBuilder;
use ffmpeg_cli::Parameter as FFParam;
use futures_util::{future::ready, StreamExt};
use std::env;
use std::fmt;
use std::fs;
use std::io::Write;
use std::path::Path;
use std::process;
use std::process::Stdio;

mod api_data;
mod downloader;
mod nicovideo;

pub const UA_STRING: &str = "Mozilla/5.0 (Windows NT 10.0; rv:126.0) Gecko/20100101 Firefox/126.0";

#[derive(Debug)]
pub enum Error {
    ReqwestError(reqwest::Error),
    IOError(std::io::Error),
    FFmpegError(ffmpeg_cli::Error),
    SerdeJsonError(serde_json::Error),
    DownloadError,
}

impl fmt::Display for Error {
    fn fmt(self: &Error, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::ReqwestError(reqwest_error) => write!(f, "{}", reqwest_error),
            Error::IOError(io_error) => write!(f, "{}", io_error),
            Error::FFmpegError(ffmpeg_error) => write!(f, "{}", ffmpeg_error),
            Error::SerdeJsonError(json_error) => write!(f, "{}", json_error),
            Error::DownloadError => write!(f, "DownloadError"),
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

impl From<ffmpeg_cli::Error> for Error {
    fn from(err: ffmpeg_cli::Error) -> Self {
        Error::FFmpegError(err)
    }
}

impl From<serde_json::Error> for Error {
    fn from(err: serde_json::Error) -> Self {
        Error::SerdeJsonError(err)
    }
}

pub fn is_debug() -> bool {
    env::var("NV_DEBUG").is_ok()
}

#[tokio::main]
async fn main() -> Result<(), Error> {
    let username = env::var("NV_USERNAME").unwrap_or("dummy".to_owned());
    let password = env::var("NV_PASSWORD").unwrap_or("dummy".to_owned());
    let cookies_path = Path::new("cookies.json");
    let nv = NicoVideo::new(cookies_path).unwrap();
    let mut args = env::args();
    let prog_name = args.next().unwrap();

    if args.len() == 0 {
        println!("Usage: {} [smXXX] [smYYY] ...", prog_name);
        return Ok(());
    }

    if !nv.is_login().await? {
        println!("[+] Need login");
        nv.login(&username, &password).await?;
        if !nv.is_login().await? {
            println!("[-] Login failed");
            process::exit(1);
        }
    }
    println!("[+] Login OK");

    let newline: &str = if !is_debug() { "\r" } else { "\n" };

    for target in args {
        if !target.starts_with("sm") && !target.starts_with("nm") {
            println!("Video ID must start by 'sm' or 'nm'");
            continue;
        }

        println!("[+] {}", target);

        let api_data: ApiData = nv.get_video_api_data(&target).await?.unwrap();
        println!("[+] Title: {}", api_data.video.title);
        let m3u8_url = nv.update_hls_cookie(&api_data, &target).await?;
        if is_debug() {
            println!("[+] master playlist is here: {}", &m3u8_url);
        }
        let outfile = format!("{}.mp4", sanitize_filename::sanitize(api_data.video.title));
        if Path::new(&outfile).exists() {
            print!("[+] '{}' is existed. overwrite? [y/N]", outfile);
            std::io::stdout().flush()?;
            let mut line = String::new();
            std::io::stdin().read_line(&mut line)?;
            line.pop();
            if line != "y" {
                continue;
            }
        }
        let temp_dir = Path::new("download_temp");
        if !temp_dir.exists() {
            fs::create_dir(temp_dir)?;
        }

        let downloader = nv.get_downloader();
        let master_playlist_filename = download_playlist(m3u8_url, &downloader, temp_dir).await?;

        let input_path = temp_dir.join(master_playlist_filename);

        let builder = FfmpegBuilder::new()
            .stderr(Stdio::piped())
            .option(FFParam::KeyValue("allowed_extensions", "ALL"))
            .option(FFParam::KeyValue("protocol_whitelist", "file"))
            .input(ffmpeg_cli::File::new(input_path.to_str().unwrap()))
            .output(
                ffmpeg_cli::File::new(&outfile).option(FFParam::KeyValue("g", "15")), // .option(FFParam::KeyValue("tune", "zerolatency")),
            );

        let ffmpeg = builder.run().await?;

        ffmpeg
            .progress
            .for_each(|_x| {
                if let Ok(x) = _x {
                    if let Some(t) = x.out_time {
                        print!("\x1b[2K\r");
                        std::io::stdout().flush().unwrap();
                        print!("[+] Processing {:?}{}", t, newline);
                        std::io::stdout().flush().unwrap();
                    }
                }
                ready(())
            })
            .await;

        println!();
        if is_debug() {
            let output = ffmpeg.process.wait_with_output()?;
            println!(
                "{}\nstderr:\n{}",
                output.status,
                std::str::from_utf8(&output.stderr).unwrap()
            );
        }

        // cleanup
        if !is_debug() {
            fs::remove_dir_all(temp_dir)?;
        }
    }
    Ok(())
}

async fn download_playlist(
    m3u8_url: String,
    downloader: &NicoVideoDownloader,
    temp_dir: &Path,
) -> Result<String, Error> {
    let master_m3u8 = downloader.download_m3u8(&m3u8_url).await?;
    let mut master_m3u8 = m3u8_rs::parse_master_playlist(&master_m3u8.into_bytes())
        .unwrap()
        .1;
    let video_info = &master_m3u8.variants[0];
    let audio_info = &master_m3u8.alternatives[0];
    let video_resolution = video_info.resolution.unwrap();
    let codec = video_info.codecs.as_ref().unwrap();
    println!(
        "[+] Video: {} ({}x{})",
        codec, video_resolution.width, video_resolution.height,
    );
    println!("[+] Audio: {}", audio_info.group_id);
    let mut video_m3u8 =
        m3u8_rs::parse_media_playlist(downloader.download_m3u8(&video_info.uri).await?.as_bytes())
            .unwrap()
            .1;
    let mut audio_m3u8 = m3u8_rs::parse_media_playlist(
        downloader
            .download_m3u8(audio_info.uri.as_ref().unwrap())
            .await?
            .as_bytes(),
    )
    .unwrap()
    .1;

    println!("[+] Video");
    downloader
        .download_media_playlist(&mut video_m3u8, temp_dir, "cmfv")
        .await?;

    println!("[+] Audio");
    downloader
        .download_media_playlist(&mut audio_m3u8, temp_dir, "cmfa")
        .await?;

    {
        let mut f = fs::File::create(temp_dir.join("video.m3u8"))?;
        video_m3u8.write_to(&mut f)?;
    }
    {
        let mut f = fs::File::create(temp_dir.join("audio.m3u8"))?;
        audio_m3u8.write_to(&mut f)?;
    }

    master_m3u8.variants[0].uri = "video.m3u8".to_string();
    master_m3u8.alternatives[0].uri = Some("audio.m3u8".to_string());
    {
        let mut f = fs::File::create(temp_dir.join("master.m3u8"))?;
        master_m3u8.write_to(&mut f)?;
    }

    Ok("master.m3u8".to_string())
}
