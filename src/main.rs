use reqwest::{Client, Error};

#[tokio::main]
async fn main() -> Result<(), Error> {
    let client = Client::new();
    let raw_html = client.get("https://www.nicovideo.jp/my").send().await?.text().await?;
    if raw_html.contains('"login_status":"not_login"') {
    }
    println!("{}", raw_html);
    Ok(())
}
