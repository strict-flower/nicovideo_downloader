use crate::ApiData::ApiData;
use reqwest::{Client, Error, Response};
use reqwest_cookie_store::{CookieStore, CookieStoreMutex};
use scraper::{Html, Selector};
use std::fs::File;
use std::io;
use std::io::{BufReader, BufWriter};
use std::path::Path;
use std::sync::Arc;

#[derive(Debug)]
pub struct NicoVideo<'a> {
    client: Client,
    cookies_path: &'a Path,
    cookies: Arc<CookieStoreMutex>,
}

impl<'a> NicoVideo<'a> {
    pub fn new(cookies_path: &'a Path) -> Result<NicoVideo<'a>, io::Error> {
        let cookies = {
            if !cookies_path.exists() {
                Ok::<CookieStore, io::Error>(CookieStore::new(None))
            } else {
                let reader = File::open(cookies_path).map(BufReader::new)?;
                Ok(CookieStore::load_json(reader).unwrap())
            }
        }?;

        let cookies_mutex = CookieStoreMutex::new(cookies);
        let cookies_arc = Arc::new(cookies_mutex);

        Ok(NicoVideo {
            client: Client::builder()
                .cookie_provider(Arc::clone(&cookies_arc))
                .build()
                .unwrap(),
            cookies_path,
            cookies: cookies_arc,
        })
    }

    pub async fn login(self: &NicoVideo<'a>, username: &str, password: &str) -> Result<(), Error> {
        let form_data = vec![("mail", username), ("password", password)];
        self.post("https://secure.nicovideo.jp/secure/login", &form_data)
            .await?;
        self.save_cookie().unwrap();
        Ok(())
    }

    pub async fn is_login(self: &NicoVideo<'a>) -> Result<bool, Error> {
        let raw_html = self.get_raw_html("https://www.nicovideo.jp/my").await?;
        Ok(!raw_html.contains("\"login_status\":\"not_login\""))
    }

    pub async fn get_video_api_data(
        self: &NicoVideo<'a>,
        video_id: &str,
    ) -> Result<Option<ApiData>, Error> {
        let video_url = format!("https://www.nicovideo.jp/watch/{}", video_id);
        let raw_html = self.get_raw_html(video_url.as_str()).await?;
        let html = Html::parse_fragment(raw_html.as_str());
        let selector = Selector::parse("div#js-initial-watch-data").unwrap();
        if let Some(selected) = html.select(&selector).next() {
            let elem = &selected.value();
            let api_data = elem.attr("data-api-data").unwrap();
            println!("{:?}", api_data);

            Ok(Some(serde_json::from_str(api_data).unwrap()))
        } else {
            Ok(None)
        }
    }

    fn save_cookie(self: &NicoVideo<'a>) -> Result<(), io::Error> {
        let mut writer = File::create(self.cookies_path).map(BufWriter::new)?;
        let store = self.cookies.lock().unwrap();
        store.save_json(&mut writer).unwrap();
        Ok(())
    }

    async fn get(self: &NicoVideo<'a>, url: &str) -> Result<Response, Error> {
        let res = self.client.get(url).send().await?;
        Ok(res)
    }

    async fn get_raw_html(self: &NicoVideo<'a>, url: &str) -> Result<String, Error> {
        let raw_html = self.get(url).await?.text().await?;
        Ok(raw_html)
    }

    async fn post(
        self: &NicoVideo<'a>,
        url: &str,
        data: &Vec<(&str, &str)>,
    ) -> Result<Response, Error> {
        let ret = self.client.post(url).form(&data).send().await?;
        Ok(ret)
    }
}
