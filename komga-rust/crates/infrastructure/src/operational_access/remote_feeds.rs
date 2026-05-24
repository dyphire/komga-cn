use async_trait::async_trait;
use komga_application::operational::RemoteFeedPort;
use reqwest::Client;

#[derive(Clone, Debug, Default)]
pub struct RemoteFeedAccess;

#[async_trait]
impl RemoteFeedPort for RemoteFeedAccess {
    async fn load_announcements_feed_bytes(&self) -> Result<Vec<u8>, String> {
        let url = std::env::var("KOMGA_RUST_ANNOUNCEMENTS_URL")
            .unwrap_or_else(|_| "https://komga.org/blog/feed.json".to_string());
        let bytes = Client::new()
            .get(url)
            .send()
            .await
            .map_err(|error| error.to_string())?
            .error_for_status()
            .map_err(|error| error.to_string())?
            .bytes()
            .await
            .map_err(|error| error.to_string())?;
        Ok(bytes.to_vec())
    }

    async fn load_releases_bytes(&self) -> Result<Vec<u8>, String> {
        let url = std::env::var("KOMGA_RUST_RELEASES_URL").unwrap_or_else(|_| {
            "https://api.github.com/repos/huihuimoe/komga-riir/releases?per_page=20".to_string()
        });
        let bytes = Client::new()
            .get(url)
            .header("User-Agent", "komga-rust-runtime")
            .send()
            .await
            .map_err(|error| error.to_string())?
            .error_for_status()
            .map_err(|error| error.to_string())?
            .bytes()
            .await
            .map_err(|error| error.to_string())?;
        Ok(bytes.to_vec())
    }
}
