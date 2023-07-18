use reqwest::Error;
use std::env;
use std::path::Path;
use std::process;
use std::sync::Arc;

mod NicoVideo;

#[tokio::main]
async fn main() -> Result<(), Error> {
    use crate::NicoVideo::NicoVideo as NV;
    let USERNAME = env::var("NV_USERNAME").unwrap_or("dummy".to_owned());
    let PASSWORD = env::var("NV_PASSWORD").unwrap_or("dummy".to_owned());

    let targets = vec!["sm29247339"];
    let cookies_path = Path::new("cookies.json");
    let mut nv = NV::new(cookies_path).unwrap();
    if !nv.is_login().await? {
        println!("[+] Need login");
        nv.login(&USERNAME, &PASSWORD).await?;
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
        nv.get_video_api_data(target).await?;
    }
    Ok(())
}
