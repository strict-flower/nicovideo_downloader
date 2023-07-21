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
        let res =
            downloader::download_master_playlist(&master_m3u8_url, temp_dir.to_str().unwrap())
                .await?;

        let mut input_files: Vec<String> = vec![];
        for f in &res {
            let t = temp_dir.join(f);
            input_files.push(t.to_str().unwrap().to_string());
        }

        let input_file_line = format!("concat:{}", input_files.as_slice().join("|"));

        let builder = FfmpegBuilder::new()
            .stderr(Stdio::piped())
            .input(ffmpeg_cli::File::new(&input_file_line))
            .output(ffmpeg_cli::File::new(&outfile).option(Parameter::KeyValue("c", "copy")));

        let ffmpeg = builder.run().await?;

        ffmpeg
            .progress
            .for_each(|_x| {
                if let Ok(x) = _x {
                    if let Some(t) = x.out_time {
                        println!("[+] Processing {:?}", t);
                    }
                }
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
        fs::remove_dir(temp_dir)?;

        nv.stop_heartbeat();
    }
    Ok(())
}
