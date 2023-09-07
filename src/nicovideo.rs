use crate::api_data::{ApiData, Session};
use crate::Error;
use reqwest::header::{CONTENT_TYPE, ORIGIN, REFERER};
use reqwest::{Client, Response};
use reqwest_cookie_store::{CookieStore, CookieStoreMutex};
use scraper::{Html, Selector};
use serde_json::json;
use std::fs::File;
use std::io;
use std::io::{BufReader, BufWriter};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::task::JoinHandle;
use tokio::time::{sleep, Duration};

#[derive(Debug)]
pub struct NicoVideo {
    client: Arc<Client>,
    cookies_path: PathBuf,
    cookies: Arc<CookieStoreMutex>,
    heartbeat_thread: Option<JoinHandle<()>>,
}

#[derive(Debug)]
struct HeartbeatRunner {
    video_id: String,
    url: String,
    session_string: String,
    client: Arc<Client>,
}

impl HeartbeatRunner {
    pub fn new(
        video_id: String,
        url: String,
        session_string: String,
        client: Arc<Client>,
    ) -> HeartbeatRunner {
        HeartbeatRunner {
            video_id,
            url,
            session_string,
            client,
        }
    }
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
            heartbeat_thread: None,
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

            Ok(Some(serde_json::from_str(api_data).unwrap()))
        } else {
            Ok(None)
        }
    }

    pub async fn create_session(
        self: &mut NicoVideo,
        video_id: &str,
        session: &Session,
    ) -> Result<String, Error> {
        let sreq = json!({
            "session": {
                "recipe_id": session.recipeId,
                "content_id": session.contentId,
                "content_type": "movie",
                "content_src_id_sets": [
                    {
                        "content_src_ids": [
                            {
                                "src_id_to_mux": {
                                    "video_src_ids": session.videos,
                                    "audio_src_ids": session.audios
                                }
                            }
                        ]
                    }
                ],
                "timing_constraint": "unlimited",
                "keep_method": {
                    "heartbeat": {
                        "lifetime": session.heartbeatLifetime
                    }
                },
                "protocol": {
                    "name": "http",
                    "parameters": {
                        "http_parameters": {
                            "parameters": {
                                "hls_parameters": {
                                    "use_well_known_port": "yes",
                                    "use_ssl": "yes",
                                    "transfer_preset": session.transferPresets[0],
                                    "segment_duration": 6000
                                }
                            }
                        }
                    }
                },
                "content_uri": "",
                "session_operation_auth": {
                    "session_operation_auth_by_signature": {
                        "token": session.token,
                        "signature": session.signature
                    }
                },
                "client_info": {
                    "player_id": session.playerId
                },
                "content_auth": {
                    "auth_type": session.authTypes.get("hls"),
                    "content_key_timeout": session.contentKeyTimeout,
                    "service_id": "nicovideo",
                    "service_user_id": session.serviceUserId
                },
                "priority": session.priority,
            }
        });
        let json_sreq = serde_json::to_string(&sreq).unwrap();

        let ret = self
            .client
            .post(format!("{}?_format=json", &session.urls[0].url))
            .body(json_sreq.clone())
            .header(CONTENT_TYPE, "application/json")
            .header(ORIGIN, "https://www.nicovideo.jp")
            .header(
                REFERER,
                format!("https://www.nicovideo.jp/watch/{}", video_id),
            )
            .send()
            .await?
            .text()
            .await?;
        let ret_json: serde_json::Value = serde_json::from_str(&ret).unwrap();
        let session_data = ret_json.get("data").unwrap().get("session").unwrap();
        let content_uri = session_data.get("content_uri").unwrap().as_str().unwrap();
        let session_id = session_data.get("id").unwrap().as_str().unwrap();

        let heartbeat_runner = HeartbeatRunner::new(
            video_id.to_owned().clone(),
            format!(
                "{}/{}?_format=json&_method=PUT",
                &session.urls[0].url, &session_id
            ),
            serde_json::to_string(&json!({
                "session": session_data
            }))
            .unwrap(),
            Arc::clone(&self.client),
        );

        self.heartbeat_thread = Some(tokio::spawn(async move {
            loop {
                let session =
                    reqwest::Body::from(heartbeat_runner.session_string.as_bytes().to_owned());
                heartbeat_runner
                    .client
                    .post(&heartbeat_runner.url)
                    .body(session)
                    .header(CONTENT_TYPE, "application/json")
                    .header(ORIGIN, "https://www.nicovideo.jp")
                    .header(
                        REFERER,
                        format!(
                            "https://www.nicovideo.jp/watch/{}",
                            &heartbeat_runner.video_id
                        ),
                    )
                    .send()
                    .await
                    .unwrap()
                    .text()
                    .await
                    .unwrap();
                // println!("[+] Heartbeat success");
                sleep(Duration::from_secs(30)).await;
            }
        }));

        Ok(content_uri.to_owned())
    }

    pub fn stop_heartbeat(self: &mut NicoVideo) {
        if let Some(handle) = &self.heartbeat_thread {
            handle.abort();
        }
        self.heartbeat_thread = None;
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
        let ret = self.client.post(url).form(&data).send().await?;
        Ok(ret)
    }
}
