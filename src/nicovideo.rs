use crate::api_data::ApiData;
use crate::{Error, NicoVideoDownloader, UA_STRING};
use reqwest::header::{CONTENT_TYPE, ORIGIN, REFERER, USER_AGENT};
use reqwest::{Client, Response};
use reqwest_cookie_store::{CookieStore, CookieStoreMutex};
use scraper::{Html, Selector};
use serde_json::json;
use std::fs::File;
use std::io;
use std::io::{BufReader, BufWriter};
use std::path::{Path, PathBuf};
use std::sync::Arc;

#[derive(Debug)]
pub struct NicoVideo {
    client: Arc<Client>,
    cookies_path: PathBuf,
    cookies: Arc<CookieStoreMutex>,
}

impl NicoVideo {
    pub fn new(cookies_path: &Path) -> Result<NicoVideo, Error> {
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
            client: Arc::new(
                Client::builder()
                    .cookie_provider(Arc::clone(&cookies_arc))
                    .build()
                    .unwrap(),
            ),
            cookies_path: cookies_path.to_owned(),
            cookies: cookies_arc,
        })
    }

    pub async fn login(self: &NicoVideo, username: &str, password: &str) -> Result<(), Error> {
        let form_data = vec![("mail", username), ("password", password)];
        self.post("https://secure.nicovideo.jp/secure/login", &form_data)
            .await?;
        self.save_cookie().unwrap();
        Ok(())
    }

    pub async fn is_login(self: &NicoVideo) -> Result<bool, Error> {
        let raw_html = self.get_raw_html("https://www.nicovideo.jp/my").await?;
        Ok(!raw_html.contains("\"login_status\":\"not_login\""))
    }

    pub async fn get_video_api_data(
        self: &NicoVideo,
        video_id: &str,
    ) -> Result<Option<ApiData>, Error> {
        let video_url = format!("https://www.nicovideo.jp/watch/{}", video_id);
        let raw_html = self.get_raw_html(video_url.as_str()).await?;
        let html = Html::parse_fragment(raw_html.as_str());
        let selector = Selector::parse("div#js-initial-watch-data").unwrap();
        if let Some(selected) = html.select(&selector).next() {
            let elem = &selected.value();
            let api_data = elem.attr("data-api-data").unwrap();
            if crate::is_debug() {
                dbg!(&api_data);
            }

            Ok(Some(serde_json::from_str(api_data).unwrap()))
        } else {
            Ok(None)
        }
    }

    pub async fn update_hls_cookie(
        &self,
        api_data: &ApiData,
        video_id: &str,
    ) -> Result<String, Error> {
        let domand = &api_data.media.domand;
        let action_track_id = &api_data.client.watchTrackId;
        let url = format!(
            "https://nvapi.nicovideo.jp/v1/watch/{}/access-rights/hls?actionTrackId={}",
            video_id, action_track_id
        );
        let id_video_domand = {
            let mut videos = Vec::new();
            videos.extend(&domand.videos);
            videos.sort_by_key(|x| x.qualityLevel);
            &videos.iter().filter(|x| x.isAvailable).last().unwrap().id
        };
        let id_audio_domand = {
            let mut audios = Vec::new();
            audios.extend(&domand.audios);
            audios.sort_by_key(|x| x.qualityLevel);
            &audios.iter().filter(|x| x.isAvailable).last().unwrap().id
        };
        let req_json = json! {{
            "outputs": [
            [id_video_domand, id_audio_domand]
            ]
        }};
        let req_json_str = serde_json::to_string(&req_json).unwrap();
        let res = self
            .client
            .post(url)
            .header(REFERER, "https://www.nicovideo.jp")
            .header(ORIGIN, "https://www.nicovideo.jp")
            .header(USER_AGENT, UA_STRING)
            .header(CONTENT_TYPE, "application/json")
            .header("X-Request-With", "https://www.nicovideo.jp")
            .header("X-Access-Right-Key", &domand.accessRightKey)
            .header("X-Frontend-Id", "6")
            .header("X-Frontend-Version", "0")
            .body(req_json_str)
            .send()
            .await?
            .text()
            .await?;
        let res: serde_json::Value = serde_json::from_str(&res)?;
        Ok(res["data"]["contentUrl"].as_str().unwrap().to_string())
    }

    fn save_cookie(self: &NicoVideo) -> Result<(), io::Error> {
        let mut writer = File::create(self.cookies_path.as_path()).map(BufWriter::new)?;
        let store = self.cookies.lock().unwrap();
        store.save_json(&mut writer).unwrap();
        Ok(())
    }

    async fn get(self: &NicoVideo, url: &str) -> Result<Response, Error> {
        let res = self
            .client
            .get(url)
            .header(REFERER, "https://www.nicovideo.jp")
            .header(ORIGIN, "https://www.nicovideo.jp")
            .header(USER_AGENT, UA_STRING)
            .send()
            .await?;
        Ok(res)
    }

    async fn get_raw_html(self: &NicoVideo, url: &str) -> Result<String, Error> {
        let raw_html = self.get(url).await?.text().await?;
        Ok(raw_html)
    }

    async fn post(
        self: &NicoVideo,
        url: &str,
        data: &Vec<(&str, &str)>,
    ) -> Result<Response, Error> {
        let ret = self
            .client
            .post(url)
            .header(USER_AGENT, UA_STRING)
            .form(&data)
            .send()
            .await?;
        Ok(ret)
    }

    pub fn get_downloader(&self) -> NicoVideoDownloader {
        NicoVideoDownloader::new(self.client.clone())
    }
}
