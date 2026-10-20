use std::env;
use std::io::Write;
use std::path::Path;
use std::time::Duration;

use async_trait::async_trait;
use futures_util::StreamExt;
use reqwest::{Client, StatusCode, header};
use serde_json::Value;

use crate::errors::{GpmError, Result};

const USER_AGENT: &str = "gpm-cli";

#[cfg_attr(test, mockall::automock)]
#[async_trait]
pub trait HttpClient: Send + Sync {
    async fn fetch_json(&self, url: &str) -> Result<Value>;
    async fn fetch_json_with_link(&self, url: &str) -> Result<(Value, Option<String>)>;
    async fn download_file(&self, url: &str, dest: &Path) -> Result<()>;
}

pub struct ReqwestClient {
    client: Client,
}

impl ReqwestClient {
    pub fn new() -> Result<Self> {
        let mut headers = header::HeaderMap::new();
        headers.insert(
            header::USER_AGENT,
            header::HeaderValue::from_static(USER_AGENT),
        );

        if let Ok(token) = env::var("GITHUB_TOKEN") {
            let mut auth_value = header::HeaderValue::from_str(&format!("Bearer {}", token))
                .map_err(|e| GpmError::Unknown(format!("Invalid GITHUB_TOKEN: {}", e)))?;
            auth_value.set_sensitive(true);
            headers.insert(header::AUTHORIZATION, auth_value);
        }

        let client = Client::builder()
            .default_headers(headers)
            .build()
            .map_err(|e| GpmError::NetworkError(e.to_string()))?;

        Ok(Self { client })
    }

    async fn send_with_retry(&self, url: &str) -> Result<reqwest::Response> {
        let mut retries = 0;
        let max_retries = 3;

        loop {
            match self.client.get(url).send().await {
                Ok(response) => {
                    let status = response.status();
                    if status.is_success() {
                        return Ok(response);
                    }

                    if (status == StatusCode::TOO_MANY_REQUESTS || status.is_server_error())
                        && retries < max_retries
                    {
                        let wait_sec = if let Some(retry_after) =
                            response.headers().get(header::RETRY_AFTER)
                        {
                            retry_after
                                .to_str()
                                .ok()
                                .and_then(|s| s.parse::<u64>().ok())
                                .unwrap_or(2u64.pow(retries))
                        } else {
                            2u64.pow(retries)
                        };

                        tokio::time::sleep(Duration::from_secs(wait_sec)).await;
                        retries += 1;
                        continue;
                    }

                    return Err(GpmError::NetworkError(format!(
                        "HTTP Error {} for {}",
                        status, url
                    )));
                }
                Err(_e) if retries < max_retries => {
                    let wait_sec = 2u64.pow(retries);
                    tokio::time::sleep(Duration::from_secs(wait_sec)).await;
                    retries += 1;
                    continue;
                }
                Err(e) => return Err(GpmError::NetworkError(e.to_string())),
            }
        }
    }
}

#[async_trait]
impl HttpClient for ReqwestClient {
    async fn fetch_json(&self, url: &str) -> Result<Value> {
        self.fetch_json_with_link(url).await.map(|(v, _)| v)
    }

    async fn fetch_json_with_link(&self, url: &str) -> Result<(Value, Option<String>)> {
        let response = self.send_with_retry(url).await?;
        let next_link = response.headers().get(header::LINK).and_then(|h| {
            let link_str = h.to_str().ok()?;
            for part in link_str.split(',') {
                let mut segments = part.split(';');
                let url_part = segments.next()?.trim();
                let rel_part = segments.next()?.trim();
                if rel_part == "rel=\"next\"" {
                    let url = url_part.trim_start_matches('<').trim_end_matches('>');
                    return Some(url.to_string());
                }
            }
            None
        });

        let value = response
            .json::<Value>()
            .await
            .map_err(|e| GpmError::NetworkError(e.to_string()))?;

        Ok((value, next_link))
    }

    async fn download_file(&self, url: &str, dest: &Path) -> Result<()> {
        let response = self.send_with_retry(url).await?;
        let total_size = response.content_length().unwrap_or(0);
        let pb = crate::ui::create_bytes_progress_bar(total_size, "Downloading…");

        let mut file = std::fs::File::create(dest)?;
        let mut stream = response.bytes_stream();

        while let Some(item) = stream.next().await {
            let chunk = item.map_err(|e| GpmError::NetworkError(e.to_string()))?;
            file.write_all(&chunk)?;
            pb.inc(chunk.len() as u64);
        }

        pb.finish_with_message("downloaded");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::time::Duration;
    use tokio::time;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[tokio::test]
    async fn test_retry_on_429() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/api"))
            .respond_with(ResponseTemplate::new(429))
            .up_to_n_times(1)
            .expect(1)
            .mount(&mock_server)
            .await;

        Mock::given(method("GET"))
            .and(path("/api"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"ok": true})))
            .expect(1)
            .mount(&mock_server)
            .await;

        time::pause();
        let client = ReqwestClient::new().unwrap();
        let url = format!("{}/api", mock_server.uri());
        let res = client.fetch_json(&url).await.unwrap();
        assert_eq!(res["ok"], true);
    }

    #[tokio::test]
    async fn test_retry_on_500() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/api"))
            .respond_with(ResponseTemplate::new(500))
            .up_to_n_times(1)
            .expect(1)
            .mount(&mock_server)
            .await;

        Mock::given(method("GET"))
            .and(path("/api"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"ok": true})))
            .expect(1)
            .mount(&mock_server)
            .await;

        time::pause();
        let client = ReqwestClient::new().unwrap();
        let url = format!("{}/api", mock_server.uri());
        let res = client.fetch_json(&url).await.unwrap();
        assert_eq!(res["ok"], true);
    }

    #[tokio::test]
    async fn test_retry_after_header_respected() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/api"))
            .respond_with(
                ResponseTemplate::new(429).insert_header("Retry-After", "5"),
            )
            .up_to_n_times(1)
            .expect(1)
            .mount(&mock_server)
            .await;

        Mock::given(method("GET"))
            .and(path("/api"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"ok": true})))
            .expect(1)
            .mount(&mock_server)
            .await;

        time::pause();
        let start = time::Instant::now();
        let client = ReqwestClient::new().unwrap();
        let url = format!("{}/api", mock_server.uri());
        let res = client.fetch_json(&url).await.unwrap();
        
        let elapsed = start.elapsed();
        assert_eq!(res["ok"], true);
        assert!(elapsed >= Duration::from_secs(5));
    }

    #[tokio::test]
    async fn test_max_retries_exceeded() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/api"))
            .respond_with(ResponseTemplate::new(500))
            .expect(4) // 1 initial + 3 retries
            .mount(&mock_server)
            .await;

        time::pause();
        let client = ReqwestClient::new().unwrap();
        let url = format!("{}/api", mock_server.uri());
        let res = client.fetch_json(&url).await;
        
        assert!(res.is_err());
        assert!(res.unwrap_err().to_string().contains("HTTP Error 500"));
    }

    #[tokio::test]
    async fn test_non_retryable_error() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/api"))
            .respond_with(ResponseTemplate::new(404))
            .expect(1)
            .mount(&mock_server)
            .await;

        time::pause();
        let client = ReqwestClient::new().unwrap();
        let url = format!("{}/api", mock_server.uri());
        let res = client.fetch_json(&url).await;
        
        assert!(res.is_err());
        assert!(res.unwrap_err().to_string().contains("HTTP Error 404"));
    }
}
