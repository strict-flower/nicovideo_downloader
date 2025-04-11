use crate::api_data::ApiData;
use crate::seiga::SeigaDownloader;
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

    pub async fn login(
        &self,
        username: &str,
        password: &str,
        totp_secret: Option<&str>,
    ) -> Result<(), Error> {
        let form_data = vec![("mail_tel", username), ("password", password)];
        let res = self
            .client
            .post("https://account.nicovideo.jp/login/redirector?site=niconico&next_url=%2F")
            .header(USER_AGENT, UA_STRING)
            .header(REFERER, "https://account.nicovideo.jp/login?site=niconico")
            .header(reqwest::header::ACCEPT_LANGUAGE, "en-US,en;q=0.7,en;q=0.3") // for simple
            .form(&form_data)
            .send()
            .await?;
        let raw_html = &res.text().await?;
        if raw_html.contains("2-Step Verification") {
            // MFA flow
            let secret = base32::decode(
                base32::Alphabet::Rfc4648 { padding: true },
                totp_secret.expect("Neeed to specify totp_secret"),
            )
            .expect("Invalid base32 string: totp secret");
            let current = chrono::Local::now().timestamp() as u64;
            let totp = compute_totp(&secret, current, 30, 0, 6);
            let (next_url, device_name) = {
                let html = Html::parse_fragment(raw_html);
                let form_selector = Selector::parse("form").unwrap();
                let form_elem = html.select(&form_selector).next().unwrap().value();
                let name_selector = Selector::parse("input#deviceNameInput").unwrap();
                let name_elem = html.select(&name_selector).next().unwrap().value();
                (
                    form_elem.attr("action").unwrap().to_string(),
                    name_elem.attr("value").unwrap().to_string(),
                )
            };
            // totp and device_name are String type, so it causes type incosistency between (&str, &'static str) and (&str, &String)
            let totp: &str = &totp;
            let device_name: &str = &device_name;
            let form_data = vec![
                ("otp", totp),
                ("device_name", device_name),
                ("is_mfa_trusted_device", "true"),
                ("loginBtn", "Login"),
            ];
            let res = self
                .post(
                    &format!("https://account.nicovideo.jp{next_url}"),
                    &form_data,
                )
                .await?
                .text()
                .await?;
            if crate::is_debug() {
                dbg!(res);
            }
        }
        self.save_cookie().unwrap();
        Ok(())
    }

    pub async fn is_login(&self) -> Result<bool, Error> {
        let raw_html = self.get_raw_html("https://www.nicovideo.jp/my").await?;
        Ok(!raw_html.contains("\"login_status\":\"not_login\""))
    }

    pub async fn get_video_api_data(&self, video_id: &str) -> Result<Option<ApiData>, Error> {
        let video_url = format!("https://www.nicovideo.jp/watch/{}", video_id);
        let raw_html = self.get_raw_html(video_url.as_str()).await?;
        let html = Html::parse_fragment(raw_html.as_str());
        let selector = Selector::parse("meta[name=\"server-response\"]").unwrap();
        if let Some(selected) = html.select(&selector).next() {
            let elem = &selected.value();
            let api_data = elem.attr("content").unwrap();
            if crate::is_debug() {
                dbg!(&api_data);
            }

            let api_data: serde_json::Value = serde_json::from_str(api_data).unwrap();
            let status = api_data["meta"]["status"].as_i64().unwrap();

            if status != 200 {
                println!("Status: {}", status);
                return Ok(None);
            }

            let api_data = &api_data["data"]["response"];

            Ok(Some(serde_json::from_value(api_data.clone()).unwrap()))
        } else {
            Ok(None)
        }
    }

    pub async fn get_comments(&self, api_data: &ApiData) -> Result<serde_json::Value, Error> {
        let comment = api_data.comment.as_ref().unwrap();
        let nv_comment = comment["nvComment"].clone();

        let req = json! {{
            "additionals": {},
            "params": {
                "targets": nv_comment["params"]["targets"],
                "language": "ja-jp"
            },
            "threadKey": nv_comment["threadKey"],
        }};
        let req_json_str = serde_json::to_string(&req).unwrap();
        let url = format!("{}/v1/threads", nv_comment["server"].as_str().unwrap());

        if crate::is_debug() {
            println!("[+] Comment Server: {}", url);
        }

        let res = self
            .client
            .post(url)
            .header(REFERER, "https://www.nicovideo.jp")
            .header(ORIGIN, "https://www.nicovideo.jp")
            .header(USER_AGENT, UA_STRING)
            .header(CONTENT_TYPE, "application/json")
            .header("X-Frontend-Id", "6")
            .header("X-Frontend-Version", "0")
            .header("X-NicoNico-Language", "ja-jp")
            .body(req_json_str)
            .send()
            .await?
            .text()
            .await?;

        let res: serde_json::Value = serde_json::from_str(&res)?;
        Ok(res)
    }

    pub async fn get_series(&self, series_id: &str) -> Result<crate::series::Series, Error> {
        let mut page = 1;
        let mut items = vec![];
        loop {
            let json = self.get_series_impl(series_id, page).await?;
            let total = json["data"]["totalCount"].as_i64().unwrap();
            json["data"]["items"]
                .as_array()
                .unwrap()
                .iter()
                .for_each(|x| items.push(x["meta"]["id"].as_str().unwrap().to_string()));
            if total < page * 100 {
                return Ok(crate::series::Series {
                    id: str::parse(series_id).unwrap(),
                    owner: str::parse(json["data"]["detail"]["owner"]["id"].as_str().unwrap())
                        .unwrap(),
                    owner_name: json["data"]["detail"]["owner"]["user"]["nickname"]
                        .as_str()
                        .unwrap()
                        .to_string(),
                    title: json["data"]["detail"]["title"]
                        .as_str()
                        .unwrap()
                        .to_string(),
                    description: json["data"]["detail"]["description"]
                        .as_str()
                        .unwrap()
                        .to_string(),
                    decorated_description_html: json["data"]["detail"]["decoratedDescriptionHtml"]
                        .as_str()
                        .unwrap()
                        .to_string(),
                    thumbnail_url: json["data"]["detail"]["thumbnailUrl"]
                        .as_str()
                        .unwrap()
                        .to_string(),
                    is_listed: json["data"]["detail"]["isListed"].as_bool().unwrap(),
                    created_at: json["data"]["detail"]["createdAt"]
                        .as_str()
                        .unwrap()
                        .to_string(),
                    updated_at: json["data"]["detail"]["updatedAt"]
                        .as_str()
                        .unwrap()
                        .to_string(),
                    items,
                });
            }
            page += 1;
            tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
        }
    }

    async fn get_series_impl(
        &self,
        series_id: &str,
        page: i64,
    ) -> Result<serde_json::Value, Error> {
        let api_url = format!(
            "https://nvapi.nicovideo.jp/v2/series/{}?page={}&sensitiveContents=mask&pageSize=100",
            series_id, page
        );
        let res = self
            .client
            .get(&api_url)
            .header(REFERER, "https://www.nicovideo.jp")
            .header(ORIGIN, "https://www.nicovideo.jp")
            .header(USER_AGENT, UA_STRING)
            .header("X-Frontend-Id", "6")
            .header("X-Frontend-Version", "0")
            .header("X-NicoNico-Language", "ja-jp")
            .send()
            .await?
            .text()
            .await?;
        let json: serde_json::Value = serde_json::from_str(res.as_str())?;
        if crate::is_debug() {
            dbg!(api_url);
            dbg!(&json);
        }
        let status_code = json["meta"]["status"].as_i64().unwrap_or(-1);
        if status_code != 200 {
            println!(
                "Error: Series API didn't return correctly result (expected: 200, actual: {status_code})"
            );
            return Err(Error::DownloadError);
        }
        Ok(json)
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
            &videos
                .iter()
                .filter(|x| x.isAvailable)
                .next_back()
                .unwrap()
                .id
        };
        let id_audio_domand = {
            let mut audios = Vec::new();
            audios.extend(&domand.audios);
            audios.sort_by_key(|x| x.qualityLevel);
            &audios
                .iter()
                .filter(|x| x.isAvailable)
                .next_back()
                .unwrap()
                .id
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
            .header("X-NicoNico-Language", "ja-jp")
            .body(req_json_str)
            .send()
            .await?
            .text()
            .await?;
        let res: serde_json::Value = serde_json::from_str(&res)?;
        Ok(res["data"]["contentUrl"].as_str().unwrap().to_string())
    }

    fn save_cookie(&self) -> Result<(), io::Error> {
        let mut writer = File::create(self.cookies_path.as_path()).map(BufWriter::new)?;
        let store = self.cookies.lock().unwrap();
        store.save_json(&mut writer).unwrap();
        Ok(())
    }

    async fn get(&self, url: &str) -> Result<Response, Error> {
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

    async fn get_raw_html(&self, url: &str) -> Result<String, Error> {
        let raw_html = self.get(url).await?.text().await?;
        Ok(raw_html)
    }

    async fn post(&self, url: &str, data: &Vec<(&str, &str)>) -> Result<Response, Error> {
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

    pub fn get_seiga_downloader(&self) -> SeigaDownloader {
        SeigaDownloader::new(self.client.clone(), self.cookies.clone())
    }
}

fn compute_totp(secret: &[u8], time: u64, period: u64, t0: u64, digits: usize) -> String {
    use hmac::{Hmac, Mac};
    use sha1::Sha1;

    let t = (time - t0) / period;
    let c = t.to_be_bytes();
    let mut hmac = Hmac::<Sha1>::new_from_slice(secret).unwrap();
    hmac.update(&c);
    let hs = hmac.finalize();
    let hs = hs.into_bytes();
    let offset = (*hs.iter().last().unwrap() as usize) & 0xf;
    let hs_sliced: [u8; 4] = (&hs[offset..(offset + 4)]).try_into().unwrap();
    let s = u32::from_be_bytes(hs_sliced) & 0x7FFFFFFF;
    let base = 10u32;
    let s = s % (base.pow(digits as u32));
    format!("{:0>digits$}", s)
}

#[cfg(test)]
mod test {
    #[test]
    fn test_totp() {
        use crate::nicovideo::compute_totp;

        let k = b"12345678901234567890";
        let time_list: [u64; 6] = [
            59,
            1111111109,
            1111111111,
            1234567890,
            2000000000,
            20000000000,
        ];
        let expected: [&str; 6] = [
            "94287082", "07081804", "14050471", "89005924", "69279037", "65353130",
        ];
        let period = 30;
        let t0 = 0;
        for (t, e) in time_list.iter().zip(expected) {
            assert_eq!(&compute_totp(k, *t, period, t0, 8), e);
        }
    }
}
