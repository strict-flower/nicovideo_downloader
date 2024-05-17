use crate::api_data::ApiData;
use crate::downloader::NicoVideoDownloader;
use crate::nicovideo::NicoVideo;
use ffmpeg_cli::{FfmpegBuilder, Parameter as FFParam};
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

macro_rules! error_impl {
    ($name:ident, $t:ty) => {
        impl From<$t> for Error {
            fn from(err: $t) -> Self {
                Error::$name(err)
            }
        }
    };
}

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
            Error::ReqwestError(err) => write!(f, "{}", err),
            Error::IOError(err) => write!(f, "{}", err),
            Error::FFmpegError(err) => write!(f, "{}", err),
            Error::SerdeJsonError(err) => write!(f, "{}", err),
            Error::DownloadError => write!(f, "DownloadError"),
        }
    }
}

impl std::error::Error for Error {}

error_impl!(ReqwestError, reqwest::Error);
error_impl!(IOError, std::io::Error);
error_impl!(FFmpegError, ffmpeg_cli::Error);
error_impl!(SerdeJsonError, serde_json::Error);

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

    for target in args {
        if !target.starts_with("sm") && !target.starts_with("nm") {
            println!("Video ID must start by 'sm' or 'nm'");
            continue;
        }

        println!("\n[+] {}", target);
        download_video(&nv, target).await?;
    }
    Ok(())
}

async fn download_video(nv: &NicoVideo, target: String) -> Result<(), Error> {
    let api_data: ApiData = nv.get_video_api_data(&target).await?.unwrap();
    println!("[+] Title: {}", api_data.video.title);

    let m3u8_url = nv.update_hls_cookie(&api_data, &target).await?;
    if is_debug() {
        println!("master playlist is here: {}", &m3u8_url);
    }

    let outfile = format!("{}.mp4", sanitize_filename::sanitize(api_data.video.title));
    let outfile = Path::new(&outfile);
    if outfile.exists() {
        print!(
            "[?] '{}' is existed. overwrite? [y/N]",
            outfile.to_str().unwrap()
        );
        std::io::stdout().flush()?;
        let mut line = String::new();
        std::io::stdin().read_line(&mut line)?;
        line.pop();
        if line != "y" {
            return Ok(());
        }
    }
    let temp_dir_name = format!("download_temp_{}", target);
    let temp_dir = Path::new(&temp_dir_name);
    if !temp_dir.exists() {
        fs::create_dir(temp_dir)?;
    }

    let downloader = nv.get_downloader();
    let master_playlist_filename = downloader.download_playlist(m3u8_url, temp_dir).await?;

    println!("\n[+] Transcode HLS stream to mp4 video");
    let input_path = &temp_dir.join(master_playlist_filename);

    convert_video(input_path, outfile).await?;

    // cleanup
    if !is_debug() {
        fs::remove_dir_all(temp_dir)?;
    }

    Ok(())
}

async fn convert_video(input_path: &Path, outfile: &Path) -> Result<(), Error> {
    let newline: &str = if !is_debug() { "\r" } else { "\n" };
    let builder = FfmpegBuilder::new()
        .stderr(Stdio::piped())
        .option(FFParam::KeyValue("allowed_extensions", "ALL"))
        .option(FFParam::KeyValue("protocol_whitelist", "file"))
        .input(ffmpeg_cli::File::new(input_path.to_str().unwrap()))
        .output(
            ffmpeg_cli::File::new(outfile.to_str().unwrap()).option(FFParam::KeyValue("g", "15")), // .option(FFParam::KeyValue("tune", "zerolatency")),
        );

    let ffmpeg = builder.run().await?;

    ffmpeg
        .progress
        .for_each(|_x| {
            if let Ok(x) = _x {
                if let Some(t) = x.out_time {
                    print!("\x1b[2K\r");
                    std::io::stdout().flush().unwrap();
                    print!("Processing {:?}{}", t, newline);
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

    println!("Done");

    Ok(())
}
