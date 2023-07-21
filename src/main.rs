use crate::api_data::ApiData;
use crate::nicovideo::NicoVideo;
use ffmpeg_cli::{FfmpegBuilder, Parameter};
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

#[derive(Debug)]
pub enum Error {
    ReqwestError(reqwest::Error),
    IOError(std::io::Error),
    FFmpegError(ffmpeg_cli::Error),
    DownloadError,
}

impl fmt::Display for Error {
    fn fmt(self: &Error, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::ReqwestError(reqwest_error) => write!(f, "{}", reqwest_error),
            Error::IOError(io_error) => write!(f, "{}", io_error),
            Error::FFmpegError(ffmpeg_error) => write!(f, "{}", ffmpeg_error),
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

#[tokio::main]
async fn main() -> Result<(), Error> {
    let username = env::var("NV_USERNAME").unwrap_or("dummy".to_owned());
    let password = env::var("NV_PASSWORD").unwrap_or("dummy".to_owned());
    let cookies_path = Path::new("cookies.json");
    let mut nv = NicoVideo::new(cookies_path).unwrap();
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

        println!("[+] {}", target);

        let api_data: ApiData = nv.get_video_api_data(&target).await?.unwrap();
        println!("[+] Title: {}", api_data.video.title);
        let master_m3u8_url = nv
            .create_session(&target, &api_data.media.delivery.movie.session)
            .await?;
        println!("[+] master.m3u8 is here: {}", &master_m3u8_url);
        let outfile = format!("{}.mp4", sanitize_filename::sanitize(api_data.video.title));
        let temp_dir = Path::new("download_temp");
        let res =
            downloader::download_master_playlist(&master_m3u8_url, temp_dir.to_str().unwrap())
                .await?;

        let demuxer_mylist = temp_dir.join("mylist.txt");
        {
            let mut demuxer_mylist_file = fs::File::create(&demuxer_mylist)?;

            for x in &res {
                writeln!(demuxer_mylist_file, "file '{}'", x)?;
            }
            demuxer_mylist_file.sync_all()?;
        }

        let builder = FfmpegBuilder::new()
            .stderr(Stdio::piped())
            .option(Parameter::KeyValue("f", "concat"))
            .input(ffmpeg_cli::File::new(demuxer_mylist.to_str().unwrap()))
            .output(ffmpeg_cli::File::new(&outfile));

        let ffmpeg = builder.run().await?;

        ffmpeg
            .progress
            .for_each(|x| {
                dbg!(x.unwrap());
                ready(())
            })
            .await;

        let output = ffmpeg.process.wait_with_output()?;

        println!(
            "{}\nstderr:\n{}",
            output.status,
            std::str::from_utf8(&output.stderr).unwrap()
        );

        // cleanup
        for file in &res {
            let fpath = temp_dir.join(file);
            fs::remove_file(fpath)?;
        }
        fs::remove_file(temp_dir.join("mylist.txt"))?;
        fs::remove_dir(temp_dir)?;

        nv.stop_heartbeat();
    }
    Ok(())
}
