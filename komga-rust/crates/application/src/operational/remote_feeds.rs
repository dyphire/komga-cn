use std::collections::BTreeSet;
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;

use super::AnnouncementPort;

#[async_trait]
pub trait RemoteFeedPort: Send + Sync {
    async fn load_announcements_feed_bytes(&self) -> Result<Vec<u8>, String>;
    async fn load_releases_bytes(&self) -> Result<Vec<u8>, String>;
}

const CACHE_TTL_SECONDS: u64 = 60 * 60;

pub struct RemoteFeedService {
    feeds: Arc<dyn RemoteFeedPort>,
    announcements: Arc<dyn AnnouncementPort>,
    announcements_cache: Arc<Mutex<Option<RemoteFeedCacheEntry>>>,
    releases_cache: Arc<Mutex<Option<RemoteFeedCacheEntry>>>,
}

#[derive(Clone)]
struct RemoteFeedCacheEntry {
    fetched_at_epoch_seconds: u64,
    payload: Value,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SaveAnnouncementsReadError {
    InvalidPayload(String),
    Persist(String),
}

impl RemoteFeedService {
    pub fn new(feeds: Arc<dyn RemoteFeedPort>, announcements: Arc<dyn AnnouncementPort>) -> Self {
        Self {
            feeds,
            announcements,
            announcements_cache: Arc::new(Mutex::new(None)),
            releases_cache: Arc::new(Mutex::new(None)),
        }
    }

    pub async fn save_announcements_read_from_body(
        &self,
        user_id: &str,
        body: &[u8],
    ) -> Result<(), SaveAnnouncementsReadError> {
        let ids =
            parse_announcement_ids(body).map_err(SaveAnnouncementsReadError::InvalidPayload)?;
        self.announcements
            .save_announcements_read(user_id, &ids)
            .await
            .map_err(SaveAnnouncementsReadError::Persist)
    }

    pub async fn announcements_for_user(&self, user_id: &str) -> Result<Option<Value>, String> {
        let feed = match self.load_cached_announcements_feed().await? {
            Some(feed) => feed,
            None => return Ok(None),
        };
        let read_ids = self
            .announcements
            .load_announcement_read_ids(user_id)
            .await?;
        Ok(Some(apply_announcement_read_projection(feed, &read_ids)))
    }

    pub async fn releases(&self) -> Result<Value, String> {
        self.load_cached_releases().await
    }

    async fn load_cached_announcements_feed(&self) -> Result<Option<Value>, String> {
        let now = now_epoch_seconds();
        {
            let mut cache = self
                .announcements_cache
                .lock()
                .expect("announcements cache lock should not be poisoned");
            if let Some(payload) =
                load_remote_cache_entry_on_access(&mut cache, now, CACHE_TTL_SECONDS)
            {
                return Ok(Some(payload));
            }
        }

        let bytes = self.feeds.load_announcements_feed_bytes().await?;
        if bytes.is_empty() {
            return Ok(None);
        }
        let payload = serde_json::from_slice::<Value>(&bytes).map_err(|error| error.to_string())?;
        if payload.is_null() {
            return Ok(None);
        }
        let dto = serde_json::from_value::<AnnouncementsFeedDto>(payload)
            .map_err(|error| error.to_string())?;
        let payload = serde_json::to_value(dto).map_err(|error| error.to_string())?;
        self.store_announcements_cache(now, payload.clone());
        Ok(Some(payload))
    }

    async fn load_cached_releases(&self) -> Result<Value, String> {
        let now = now_epoch_seconds();
        {
            let mut cache = self
                .releases_cache
                .lock()
                .expect("releases cache lock should not be poisoned");
            if let Some(payload) =
                load_remote_cache_entry_on_access(&mut cache, now, CACHE_TTL_SECONDS)
            {
                return Ok(payload);
            }
        }

        let bytes = self.feeds.load_releases_bytes().await?;
        let upstream = serde_json::from_slice::<Vec<GithubReleaseUpstreamDto>>(&bytes)
            .map_err(|error| error.to_string())?;
        let payload = map_github_releases(upstream);
        self.store_releases_cache(now, payload.clone());
        Ok(payload)
    }

    fn store_announcements_cache(&self, now: u64, payload: Value) {
        let entry = RemoteFeedCacheEntry {
            fetched_at_epoch_seconds: now,
            payload,
        };
        *self
            .announcements_cache
            .lock()
            .expect("announcements cache lock should not be poisoned") = Some(entry);
    }

    fn store_releases_cache(&self, now: u64, payload: Value) {
        let entry = RemoteFeedCacheEntry {
            fetched_at_epoch_seconds: now,
            payload,
        };
        *self
            .releases_cache
            .lock()
            .expect("releases cache lock should not be poisoned") = Some(entry);
    }
}

fn parse_announcement_ids(body: &[u8]) -> Result<Vec<String>, String> {
    let payload = serde_json::from_slice::<Value>(body).map_err(|error| error.to_string())?;
    let announcement_ids = payload
        .as_array()
        .ok_or_else(|| "announcement ids must be a JSON array".to_string())?;

    let mut ids = Vec::with_capacity(announcement_ids.len());
    let mut seen = BTreeSet::new();
    for id in announcement_ids {
        let id = id
            .as_str()
            .ok_or_else(|| "announcement ids must be strings".to_string())?
            .to_string();
        if seen.insert(id.clone()) {
            ids.push(id);
        }
    }

    Ok(ids)
}

fn load_remote_cache_entry_on_access(
    cache: &mut Option<RemoteFeedCacheEntry>,
    now_epoch_seconds: u64,
    ttl_seconds: u64,
) -> Option<Value> {
    let cached = cache.as_mut()?;
    if now_epoch_seconds.saturating_sub(cached.fetched_at_epoch_seconds) >= ttl_seconds {
        return None;
    }
    cached.fetched_at_epoch_seconds = now_epoch_seconds;
    Some(cached.payload.clone())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct AnnouncementsFeedDto {
    version: String,
    title: String,
    #[serde(rename = "home_page_url")]
    home_page_url: Option<String>,
    description: Option<String>,
    #[serde(default)]
    items: Vec<AnnouncementItemDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct AnnouncementItemDto {
    id: String,
    url: Option<String>,
    title: Option<String>,
    summary: Option<String>,
    #[serde(rename = "content_html")]
    content_html: Option<String>,
    #[serde(default)]
    #[serde(with = "optional_rfc3339")]
    #[serde(rename = "date_modified")]
    date_modified: Option<OffsetDateTime>,
    author: Option<AnnouncementAuthorDto>,
    #[serde(default)]
    tags: BTreeSet<String>,
    #[serde(rename = "_komga")]
    komga_extension: Option<AnnouncementKomgaExtensionDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct AnnouncementAuthorDto {
    name: Option<String>,
    url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct AnnouncementKomgaExtensionDto {
    read: bool,
}

mod optional_rfc3339 {
    use super::{OffsetDateTime, Rfc3339};
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S>(value: &Option<OffsetDateTime>, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match value {
            Some(value) => serializer
                .serialize_some(&value.format(&Rfc3339).map_err(serde::ser::Error::custom)?),
            None => serializer.serialize_none(),
        }
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Option<OffsetDateTime>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = Option::<String>::deserialize(deserializer)?;
        value
            .map(|value| OffsetDateTime::parse(&value, &Rfc3339).map_err(serde::de::Error::custom))
            .transpose()
    }
}

fn map_github_releases(upstream: Vec<GithubReleaseUpstreamDto>) -> Value {
    Value::Array(
        upstream
            .iter()
            .enumerate()
            .map(|(index, release)| {
                let release_date = release
                    .published_at
                    .format(&Rfc3339)
                    .expect("release published_at should format as rfc3339");
                json!({
                    "version": release.tag_name,
                    "releaseDate": release_date,
                    "url": release.html_url,
                    "latest": index == 0,
                    "preRelease": release.prerelease,
                    "description": release.body,
                })
            })
            .collect(),
    )
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct GithubReleaseUpstreamDto {
    html_url: String,
    tag_name: String,
    #[serde(with = "required_rfc3339")]
    published_at: OffsetDateTime,
    body: String,
    prerelease: bool,
}

mod required_rfc3339 {
    use super::{OffsetDateTime, Rfc3339};
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S>(value: &OffsetDateTime, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&value.format(&Rfc3339).map_err(serde::ser::Error::custom)?)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<OffsetDateTime, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        OffsetDateTime::parse(&value, &Rfc3339).map_err(serde::de::Error::custom)
    }
}

fn apply_announcement_read_projection(feed: Value, read_ids: &[String]) -> Value {
    let mut projected = feed;
    let read_set = read_ids.iter().cloned().collect::<BTreeSet<_>>();

    if let Some(items) = projected
        .as_object_mut()
        .and_then(|object| object.get_mut("items"))
        .and_then(Value::as_array_mut)
    {
        for item in items {
            let read = item
                .get("id")
                .and_then(Value::as_str)
                .is_some_and(|id| read_set.contains(id));
            if let Some(object) = item.as_object_mut() {
                object.insert("_komga".to_string(), json!({ "read": read }));
            }
        }
    }

    projected
}

fn now_epoch_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use async_trait::async_trait;
    use serde_json::json;

    use super::*;

    struct StubRemoteFeedPort {
        announcements: Mutex<Result<Vec<u8>, String>>,
        releases: Mutex<Result<Vec<u8>, String>>,
    }

    impl Default for StubRemoteFeedPort {
        fn default() -> Self {
            Self {
                announcements: Mutex::new(Ok(Vec::new())),
                releases: Mutex::new(Ok(Vec::new())),
            }
        }
    }

    #[async_trait]
    impl RemoteFeedPort for StubRemoteFeedPort {
        async fn load_announcements_feed_bytes(&self) -> Result<Vec<u8>, String> {
            self.announcements
                .lock()
                .expect("announcements stub should not be poisoned")
                .clone()
        }

        async fn load_releases_bytes(&self) -> Result<Vec<u8>, String> {
            self.releases
                .lock()
                .expect("releases stub should not be poisoned")
                .clone()
        }
    }

    #[derive(Default)]
    struct StubAnnouncementPort {
        read_ids: Mutex<Vec<String>>,
        saved_ids: Mutex<Vec<String>>,
    }

    #[async_trait]
    impl AnnouncementPort for StubAnnouncementPort {
        async fn load_announcement_read_ids(&self, _user_id: &str) -> Result<Vec<String>, String> {
            Ok(self
                .read_ids
                .lock()
                .expect("read ids stub should not be poisoned")
                .clone())
        }

        async fn save_announcements_read(
            &self,
            _user_id: &str,
            ids: &[String],
        ) -> Result<(), String> {
            *self
                .saved_ids
                .lock()
                .expect("saved ids stub should not be poisoned") = ids.to_vec();
            Ok(())
        }
    }

    #[tokio::test]
    async fn announcements_for_user_projects_read_state_and_strips_unknown_fields() {
        let feeds = Arc::new(StubRemoteFeedPort {
            announcements: Mutex::new(Ok(serde_json::to_vec(&json!({
                "version": "https://jsonfeed.org/version/1.1",
                "title": "Komga News",
                "home_page_url": "https://komga.org",
                "unknown": "removed",
                "items": [
                    {
                        "id": "announcement-1",
                        "url": "https://komga.org/1",
                        "title": "One",
                        "date_modified": "2024-01-01T00:00:00Z",
                        "unknown": "removed"
                    },
                    {
                        "id": "announcement-2",
                        "title": "Two"
                    }
                ]
            }))
            .expect("feed json should serialize"))),
            releases: Mutex::new(Ok(Vec::new())),
        });
        let announcements = Arc::new(StubAnnouncementPort::default());
        *announcements
            .read_ids
            .lock()
            .expect("read ids stub should not be poisoned") = vec!["announcement-2".to_string()];
        let service = RemoteFeedService::new(feeds, announcements);

        let payload = service
            .announcements_for_user("user-1")
            .await
            .expect("announcements should load")
            .expect("announcements feed should exist");

        assert_eq!(payload["title"], "Komga News");
        assert!(payload.get("unknown").is_none());
        assert!(payload["items"][0].get("unknown").is_none());
        assert_eq!(payload["items"][0]["_komga"]["read"], false);
        assert_eq!(payload["items"][1]["_komga"]["read"], true);
    }

    #[tokio::test]
    async fn save_announcements_read_deduplicates_duplicate_strings() {
        let feeds = Arc::new(StubRemoteFeedPort::default());
        let announcements = Arc::new(StubAnnouncementPort::default());
        let service = RemoteFeedService::new(feeds, announcements.clone());

        service
            .save_announcements_read_from_body(
                "user-1",
                br#"["announcement-1","announcement-1","announcement-2"]"#,
            )
            .await
            .expect("announcement ids should parse");

        assert_eq!(
            announcements
                .saved_ids
                .lock()
                .expect("saved ids stub should not be poisoned")
                .clone(),
            vec!["announcement-1".to_string(), "announcement-2".to_string()]
        );
    }

    #[tokio::test]
    async fn releases_maps_github_payload_once_at_service_boundary() {
        let feeds = Arc::new(StubRemoteFeedPort {
            announcements: Mutex::new(Ok(Vec::new())),
            releases: Mutex::new(Ok(br#"[{
                    "html_url": "https://github.test/releases/v1",
                    "tag_name": "v1.0.0",
                    "published_at": "2024-01-02T03:04:05Z",
                    "body": "Release notes",
                    "prerelease": false
                }]"#
            .to_vec())),
        });
        let announcements = Arc::new(StubAnnouncementPort::default());
        let service = RemoteFeedService::new(feeds, announcements);

        let payload = service.releases().await.expect("releases should load");

        assert_eq!(payload[0]["version"], "v1.0.0");
        assert_eq!(payload[0]["latest"], true);
        assert_eq!(payload[0]["preRelease"], false);
        assert_eq!(payload[0]["releaseDate"], "2024-01-02T03:04:05Z");
    }

    #[tokio::test]
    async fn releases_rejects_null_payload() {
        let feeds = Arc::new(StubRemoteFeedPort {
            announcements: Mutex::new(Ok(Vec::new())),
            releases: Mutex::new(Ok(b"null".to_vec())),
        });
        let announcements = Arc::new(StubAnnouncementPort::default());
        let service = RemoteFeedService::new(feeds, announcements);

        service
            .releases()
            .await
            .expect_err("null releases payload should be rejected");
    }
}
