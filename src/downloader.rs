use crate::{Error, UA_STRING};
use aes::cipher::{block_padding::Pkcs7, BlockDecryptMut, KeyIvInit};
use futures_util::StreamExt;
use reqwest::header::{ORIGIN, REFERER, USER_AGENT};
use reqwest::{Client, Response};
use std::io::prelude::*;
use std::path::Path;
use std::sync::Arc;
use tokio::time::{sleep, Duration};

pub struct NicoVideoDownloader {
    client: Arc<Client>,
}

fn url_to_filename<'a>(url: &'a str, extension: &'a str) -> &'a str {
    let path = url.split_once('?').unwrap().0;
    path.split('/')
        .filter(|x| x.ends_with(&extension))
        .last()
        .unwrap()
}

impl NicoVideoDownloader {
    pub fn new(client: Arc<Client>) -> NicoVideoDownloader {
        Self { client }
    }

    pub async fn download_m3u8(&self, m3u8_url: &str) -> Result<String, Error> {
        Ok(self.get(m3u8_url).await?.text().await?.to_string())
    }

    pub async fn download_raw(&self, url: &str) -> Result<Vec<u8>, Error> {
        let mut stream = self.get(url).await?.bytes_stream();

        let mut v: Vec<u8> = vec![];

        while let Some(item) = stream.next().await {
            v.extend_from_slice(&item.unwrap());
        }

        Ok(v)
    }

    async fn download_and_decrypt(
        &self,
        url: &str,
        file: &Path,
        key: &[u8],
        iv: &[u8],
    ) -> Result<(), Error> {
        type Aes128CbcDec = cbc::Decryptor<aes::Aes128>;
        let ciphertext = self.download_raw(url).await?;
        let plaintext = Aes128CbcDec::new(key.into(), iv.into())
            .decrypt_padded_vec_mut::<Pkcs7>(&ciphertext)
            .unwrap();

        let mut f = std::fs::File::create(file)?;
        f.write_all(&plaintext)?;

        Ok(())
    }

    async fn download_into_file(&self, url: &str, file: &Path) -> Result<(), Error> {
        let mut f = std::fs::File::create(file)?;
        let mut stream = self.get(url).await?.bytes_stream();

        while let Some(item) = stream.next().await {
            f.write_all(&item.unwrap())?;
        }

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

    pub async fn download_media_playlist(
        &self,
        playlist: &mut m3u8_rs::MediaPlaylist,
        temp_dir: &Path,
        extension: &str,
    ) -> Result<(), Error> {
        let newline: &str = if !crate::is_debug() { "\r" } else { "\n" };

        let mut key_bytes: Vec<u8> = vec![0; 16];
        let mut iv_bytes: Vec<u8> = vec![0; 16];
        for segment in &mut playlist.segments {
            if let Some(key) = &segment.key {
                println!("[+] Downloading key...");
                let key_url = key.uri.as_ref().unwrap();
                key_bytes = self.download_raw(key_url).await?;
                // strip leading "0x"
                let _ = hex::decode_to_slice(&key.iv.as_ref().unwrap()[2..], &mut iv_bytes);
                segment.key = None;
                break;
            }
        }
        println!("[+] Key = {:?}", key_bytes);
        println!("[+] IV = {:?}", iv_bytes);

        for segment in &mut playlist.segments {
            sleep(Duration::from_millis(250)).await;
            if let Some(map) = &segment.map {
                let map_url = &map.uri;
                let map_file = Path::new(url_to_filename(map_url, extension));
                self.download_into_file(map_url, temp_dir.join(map_file).as_path())
                    .await?;
                segment.map.as_mut().unwrap().uri = map_file.to_str().unwrap().to_string();
            }

            let filename = Path::new(url_to_filename(&segment.uri, extension));
            print!("[+] {}{}", filename.to_str().unwrap(), newline);
            std::io::stdout().flush().unwrap();

            let filepath = temp_dir.join(filename);
            self.download_and_decrypt(&segment.uri, filepath.as_path(), &key_bytes, &iv_bytes)
                .await?;
            segment.uri = filename.to_str().unwrap().to_string();
        }
        println!();

        Ok(())
    }
}
