use crate::{Error, UA_STRING};
use futures_util::StreamExt;
use reqwest::header::{ORIGIN, REFERER, USER_AGENT};
use reqwest::{Client, Response};
use reqwest_cookie_store::CookieStoreMutex;
use serde::Serialize;
use std::sync::Arc;

#[derive(Debug, Serialize)]
pub struct SeigaMetadata {
    pub title: String,
    pub description: String,
    pub view_count: i64,
    pub comment_count: i64,
    pub clip_count: i64,
    pub owner_nickname: Option<String>,
    pub owner_id: Option<String>,
    pub created_at: String,
    pub tags: serde_json::Value,
    pub comments: serde_json::Value,
}

pub struct SeigaDownloader {
    client: Arc<Client>,
    cookies: Arc<CookieStoreMutex>,
}

impl SeigaDownloader {
    pub fn new(client: Arc<Client>, cookies: Arc<CookieStoreMutex>) -> Self {
        Self { client, cookies }
    }

    pub async fn download_seiga(
        &self,
        image_id: &str,
    ) -> Result<Option<(crate::seiga::SeigaMetadata, Vec<u8>)>, Error> {
        if !self
            .cookies
            .lock()
            .unwrap()
            .contains("seiga.nicovideo.jp", "/", "skip_fetish_warning")
        {
            let cookie = reqwest_cookie_store::RawCookie::build("skip_fetish_warning", "3")
                .domain("seiga.nicovideo.jp")
                .path("/")
                .finish();
            self.cookies
                .lock()
                .unwrap()
                .insert_raw(
                    &cookie,
                    &url::Url::parse("https://seiga.nicovideo.jp/").unwrap(),
                )
                .unwrap();
        }

        let get_wrapper = |url: String, label| async move {
            Ok(loop {
                break match self.get_raw_html(&url).await {
                    Ok(x) => x,
                    Err(Error::ReqwestError(e)) => {
                        if e.is_connect() {
                            if crate::is_debug() {
                                println!(
                                    "[{label}] Server has been reset a connection, wait and retry..."
                                );
                                dbg!(&url);
                            }
                            Self::sleep_sec(10).await;
                            continue;
                        }
                        return Err(Error::ReqwestError(e));
                    }
                    Err(e) => return Err(e),
                };
            })
        };

        let image_id = if image_id.starts_with("im") {
            image_id.strip_prefix("im").unwrap()
        } else {
            image_id
        };

        let url = format!("https://seiga.nicovideo.jp/seiga/im{image_id}");
        let html = get_wrapper(url, "seiga_page").await?;
        if html.contains("ページが見つかりません") {
            // deleted image
            return Ok(None);
        }
        if html.contains("画像は非公開設定です") {
            // private image
            return Ok(None);
        }

        let html = scraper::Html::parse_document(&html);

        let title_selector = scraper::Selector::parse("h1.title").unwrap();
        let description_selector = scraper::Selector::parse("p.discription").unwrap(); // "discription" is not typo, it's correct term at NicoNicoSeiga
        let view_count_value_selector =
            scraper::Selector::parse("li.view span.count_value").unwrap();
        let comment_count_value_selector =
            scraper::Selector::parse("li.comment span.count_value").unwrap();
        let clip_count_value_selector =
            scraper::Selector::parse("li.clip span.count_value").unwrap();
        let user_id_selector = scraper::Selector::parse("div#ko_watchlist_header.user").unwrap();
        let user_name_selector =
            scraper::Selector::parse("div#ko_watchlist_header.user li.user_name strong").unwrap();
        let image_url_selector = scraper::Selector::parse("a#illust_link > img").unwrap();
        let created_selector = scraper::Selector::parse("span.created").unwrap();

        let title = html
            .select(&title_selector)
            .take(1)
            .last()
            .unwrap()
            .text()
            .next()
            .unwrap()
            .to_string();
        let description = html
            .select(&description_selector)
            .take(1)
            .last()
            .unwrap()
            .text()
            .map(|x| x.to_string())
            .reduce(|x, y| x + &y)
            .unwrap();
        let view_count = html
            .select(&view_count_value_selector)
            .take(1)
            .last()
            .unwrap()
            .text()
            .next()
            .unwrap()
            .parse()
            .unwrap();
        let comment_count = html
            .select(&comment_count_value_selector)
            .take(1)
            .last()
            .unwrap()
            .text()
            .next()
            .unwrap()
            .parse()
            .unwrap();
        let clip_count = html
            .select(&clip_count_value_selector)
            .take(1)
            .last()
            .unwrap()
            .text()
            .next()
            .unwrap()
            .parse()
            .unwrap();
        let user_id_elem = html.select(&user_id_selector).take(1).last().unwrap();
        let user_id = user_id_elem.value().attr("data-id").unwrap().to_string();
        let thumbnail_url = html
            .select(&image_url_selector)
            .take(1)
            .last()
            .unwrap()
            .value()
            .attr("src")
            .unwrap();
        let created_at = html
            .select(&created_selector)
            .take(1)
            .last()
            .unwrap()
            .text()
            .next()
            .unwrap()
            .to_string();
        let (owner_id, owner_nickname) = if user_id.is_empty() {
            (None, None)
        } else {
            (
                Some(user_id),
                Some(
                    html.select(&user_name_selector)
                        .take(1)
                        .last()
                        .unwrap()
                        .text()
                        .next()
                        .unwrap()
                        .to_string(),
                ),
            )
        };
        Self::sleep_sec(1).await;

        // Get Tag list
        let url = format!("https://seiga.nicovideo.jp/ajax/illust/tag/list?id={image_id}");
        let json = get_wrapper(url, "tag").await?;
        let tags: serde_json::Value = loop {
            break match serde_json::from_str(&json) {
                Ok(x) => x,
                Err(e) => {
                    println!("[Tag list] Server returned an invalid json, wait and retry...");
                    if crate::is_debug() {
                        dbg!(e);
                    }
                    Self::sleep_sec(10).await;
                    continue;
                }
            };
        };

        // Get image blob
        // Decide a url that points image blob
        let url = format!("https://seiga.nicovideo.jp/image/source/{image_id}");
        let image_url = loop {
            let html = get_wrapper(url.clone(), "seiga_source_page").await?;
            break if html[..6].as_bytes() == [0xef, 0xbf, 0xbd, 0x50, 0x4e, 0x47] {
                // oekakiko (new)
                // \x89 => \xef\xbf\xbd (U+FFFD REPLACEMENT CHARACTER)
                url
            } else {
                let lines: Vec<&str> = html.split("\n").collect();
                if tags["tag_list"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|x| x["name"].as_str().unwrap() == "お絵カキコ")
                {
                    // oekakiko
                    thumbnail_url.to_string()
                } else {
                    let Some(x) = lines
                        .iter()
                        .filter(|x| x.contains("data-src"))
                        .map(|x| x.split("\"").skip(1).take(1).last().unwrap())
                        .take(1)
                        .last()
                    else {
                        println!("Server returned an invalid html, wait and retry...");
                        if crate::is_debug() {
                            dbg!(image_id);
                            dbg!(html[..6].as_bytes());
                            // println!("html = {}", html);
                        }
                        Self::sleep_sec(10).await;
                        continue;
                    };

                    x.to_string()
                }
            };
        };

        // Download the blob
        let mut v = vec![];
        loop {
            let mut stream = match self.get(&image_url).await {
                Ok(x) => x,
                Err(Error::ReqwestError(e)) => {
                    if e.is_connect() {
                        if crate::is_debug() {
                            println!(
                                "[blob] Server has been reset a connection, wait and retry..."
                            );
                        }
                        Self::sleep_sec(10).await;
                        continue;
                    }
                    return Err(Error::ReqwestError(e));
                }
                Err(e) => return Err(e),
            }
            .bytes_stream();
            while let Some(item) = stream.next().await {
                v.extend_from_slice(&item.unwrap());
            }
            if &v[..6] == "<html>".as_bytes() {
                // 503
                if crate::is_debug() {
                    println!("Server returns 503, wait and retry...");
                }
                Self::sleep_sec(8).await;
                v = vec![];
                continue;
            }
            break;
        }

        Self::sleep_sec(1).await;

        // Get Comment list
        let url =
            format!("https://seiga.nicovideo.jp/ajax/illust/comment/list?id={image_id}&mode=all");
        let json = get_wrapper(url, "comment").await?;
        let comments: serde_json::Value = loop {
            break match serde_json::from_str(&json) {
                Ok(x) => x,
                Err(e) => {
                    println!("[comment] Server returned an invalid json, wait and retry...");
                    if crate::is_debug() {
                        dbg!(e);
                    }
                    Self::sleep_sec(10).await;
                    continue;
                }
            };
        };
        Self::sleep_sec(5).await;

        // Combine and return
        Ok(Some((
            crate::seiga::SeigaMetadata {
                title,
                description,
                view_count,
                comment_count,
                clip_count,
                owner_nickname,
                owner_id,
                created_at,
                tags,
                comments,
            },
            v,
        )))
    }

    pub async fn get_clips(
        &self,
        clip_id: &str,
        page: i32,
    ) -> Result<(Vec<String>, Option<i32>), Error> {
        let url = format!("https://seiga.nicovideo.jp/clip/{clip_id}?page={page}&sort=clip_number");
        let html = self.get_raw_html(&url).await?;
        let lines: Vec<&str> = html.split("\n").collect();
        let images: Vec<String> = lines
            .iter()
            .filter(|x| x.contains("<a href=\"/seiga/im"))
            .copied()
            .map(|x| {
                x.split("\"")
                    .skip(1)
                    .take(1)
                    .last()
                    .unwrap()
                    .strip_prefix("/seiga/")
                    .unwrap()
                    .to_string()
            })
            .collect();
        let next_page = lines
            .iter()
            .filter(|x| x.contains("<span class=\"page_now\">"))
            .take(1)
            .copied()
            .collect::<Vec<&str>>()[0];
        let next_page = next_page.split("|").last().unwrap();
        let next_page: Option<i32> = if next_page.contains("<span class=\"nolink\">") {
            None
        } else {
            let next_page = next_page.strip_prefix("<span><a href=\"?page=").unwrap();
            let next_page = next_page.split("&amp;").take(1).collect::<Vec<&str>>()[0];
            Some(next_page.parse().unwrap())
        };

        Ok((images, next_page))
    }

    pub async fn get_tags(
        &self,
        tag: &str,
        page: i32,
    ) -> Result<(Vec<String>, Option<i32>), Error> {
        if !self
            .cookies
            .lock()
            .unwrap()
            .contains("seiga.nicovideo.jp", "/", "skip_fetish_warning")
        {
            let cookie = reqwest_cookie_store::RawCookie::build("skip_fetish_warning", "3")
                .domain("seiga.nicovideo.jp")
                .path("/")
                .finish();
            self.cookies
                .lock()
                .unwrap()
                .insert_raw(
                    &cookie,
                    &url::Url::parse("https://seiga.nicovideo.jp/").unwrap(),
                )
                .unwrap();
        }

        let url = format!(
            "https://seiga.nicovideo.jp/tag/{}?sort=image_created_a&target=illust_all&page={page}",
            tag
        );
        let html = self.get_raw_html(&url).await?;
        let lines: Vec<&str> = html.split("\n").collect();
        let images: Vec<String> = lines
            .iter()
            .filter(|x| x.contains("<a href=\"/seiga/im"))
            .copied()
            .map(|x| {
                x.split("\"")
                    .skip(3)
                    .take(1)
                    .last()
                    .unwrap()
                    .strip_prefix("/seiga/")
                    .unwrap()
                    .to_string()
            })
            .collect();
        let next_page = lines
            .iter()
            .filter(|x| x.contains("li class=\"next"))
            .take(1)
            .copied()
            .last()
            .unwrap();
        let next_page: Option<i32> = if next_page.contains("next disabled") {
            None
        } else {
            let next_page = next_page.split("?page=").last().unwrap();
            let next_page = next_page.split("&amp;").take(1).last().unwrap();
            Some(next_page.parse().unwrap())
        };

        Ok((images, next_page))
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

    async fn sleep_sec(sec: u64) {
        tokio::time::sleep(tokio::time::Duration::from_secs(sec)).await;
    }
}
