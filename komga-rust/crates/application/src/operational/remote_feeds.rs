use std::collections::BTreeSet;
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use async_trait::async_trait;
use time::OffsetDateTime;

use super::AnnouncementPort;

#[async_trait]
pub trait RemoteFeedPort: Send + Sync {
    async fn load_announcements_feed(&self) -> Result<Option<RemoteAnnouncementsFeed>, String>;
    async fn load_releases(&self) -> Result<Vec<RemoteRelease>, String>;
}

const CACHE_TTL_SECONDS: u64 = 60 * 60;

pub struct RemoteFeedService {
    feeds: Arc<dyn RemoteFeedPort>,
    announcements: Arc<dyn AnnouncementPort>,
    announcements_cache: Arc<Mutex<Option<RemoteFeedCacheEntry<RemoteAnnouncementsFeed>>>>,
    releases_cache: Arc<Mutex<Option<RemoteFeedCacheEntry<Vec<RemoteRelease>>>>>,
}

#[derive(Clone)]
struct RemoteFeedCacheEntry<T> {
    fetched_at_epoch_seconds: u64,
    payload: T,
}

#[derive(Clone, Debug, PartialEq)]
pub struct RemoteAnnouncementsFeed {
    pub version: String,
    pub title: String,
    pub home_page_url: Option<String>,
    pub description: Option<String>,
    pub items: Vec<RemoteAnnouncementItem>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct RemoteAnnouncementItem {
    pub id: String,
    pub url: Option<String>,
    pub title: Option<String>,
    pub summary: Option<String>,
    pub content_html: Option<String>,
    pub date_modified: Option<OffsetDateTime>,
    pub author: Option<RemoteAnnouncementAuthor>,
    pub tags: BTreeSet<String>,
    pub read: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct RemoteAnnouncementAuthor {
    pub name: Option<String>,
    pub url: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct RemoteRelease {
    pub version: String,
    pub release_date: OffsetDateTime,
    pub url: String,
    pub latest: bool,
    pub pre_release: bool,
    pub description: String,
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

    pub async fn save_announcements_read(
        &self,
        user_id: &str,
        ids: &[String],
    ) -> Result<(), String> {
        let ids = deduplicate_announcement_ids(ids);
        self.announcements
            .save_announcements_read(user_id, &ids)
            .await
    }

    pub async fn announcements_for_user(
        &self,
        user_id: &str,
    ) -> Result<Option<RemoteAnnouncementsFeed>, String> {
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

    pub async fn releases(&self) -> Result<Vec<RemoteRelease>, String> {
        self.load_cached_releases().await
    }

    async fn load_cached_announcements_feed(
        &self,
    ) -> Result<Option<RemoteAnnouncementsFeed>, String> {
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

        let Some(feed) = self.feeds.load_announcements_feed().await? else {
            return Ok(None);
        };
        self.store_announcements_cache(now, feed.clone());
        Ok(Some(feed))
    }

    async fn load_cached_releases(&self) -> Result<Vec<RemoteRelease>, String> {
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

        let releases = self.feeds.load_releases().await?;
        self.store_releases_cache(now, releases.clone());
        Ok(releases)
    }

    fn store_announcements_cache(&self, now: u64, payload: RemoteAnnouncementsFeed) {
        let entry = RemoteFeedCacheEntry {
            fetched_at_epoch_seconds: now,
            payload,
        };
        *self
            .announcements_cache
            .lock()
            .expect("announcements cache lock should not be poisoned") = Some(entry);
    }

    fn store_releases_cache(&self, now: u64, payload: Vec<RemoteRelease>) {
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

fn deduplicate_announcement_ids(ids: &[String]) -> Vec<String> {
    let mut deduplicated = Vec::with_capacity(ids.len());
    let mut seen = BTreeSet::new();
    for id in ids {
        if seen.insert(id.clone()) {
            deduplicated.push(id.clone());
        }
    }

    deduplicated
}

fn load_remote_cache_entry_on_access<T: Clone>(
    cache: &mut Option<RemoteFeedCacheEntry<T>>,
    now_epoch_seconds: u64,
    ttl_seconds: u64,
) -> Option<T> {
    let cached = cache.as_ref()?;
    if now_epoch_seconds.saturating_sub(cached.fetched_at_epoch_seconds) >= ttl_seconds {
        return None;
    }
    Some(cached.payload.clone())
}

fn apply_announcement_read_projection(
    mut feed: RemoteAnnouncementsFeed,
    read_ids: &[String],
) -> RemoteAnnouncementsFeed {
    let read_set = read_ids.iter().cloned().collect::<BTreeSet<_>>();

    for item in &mut feed.items {
        item.read = read_set.contains(&item.id);
    }

    feed
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
    use time::format_description::well_known::Rfc3339;

    use super::*;

    struct StubRemoteFeedPort {
        announcements: Mutex<Result<Option<RemoteAnnouncementsFeed>, String>>,
        releases: Mutex<Result<Vec<RemoteRelease>, String>>,
    }

    impl Default for StubRemoteFeedPort {
        fn default() -> Self {
            Self {
                announcements: Mutex::new(Ok(None)),
                releases: Mutex::new(Ok(Vec::new())),
            }
        }
    }

    #[async_trait]
    impl RemoteFeedPort for StubRemoteFeedPort {
        async fn load_announcements_feed(&self) -> Result<Option<RemoteAnnouncementsFeed>, String> {
            self.announcements
                .lock()
                .expect("announcements stub should not be poisoned")
                .clone()
        }

        async fn load_releases(&self) -> Result<Vec<RemoteRelease>, String> {
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
    async fn announcements_for_user_projects_read_state_to_typed_feed() {
        let feeds = Arc::new(StubRemoteFeedPort {
            announcements: Mutex::new(Ok(Some(RemoteAnnouncementsFeed {
                version: "https://jsonfeed.org/version/1.1".to_string(),
                title: "Komga News".to_string(),
                home_page_url: Some("https://komga.org".to_string()),
                description: None,
                items: vec![
                    RemoteAnnouncementItem {
                        id: "announcement-1".to_string(),
                        url: Some("https://komga.org/1".to_string()),
                        title: Some("One".to_string()),
                        summary: None,
                        content_html: None,
                        date_modified: Some(
                            OffsetDateTime::parse("2024-01-01T00:00:00Z", &Rfc3339)
                                .expect("fixture date should parse"),
                        ),
                        author: None,
                        tags: BTreeSet::new(),
                        read: false,
                    },
                    RemoteAnnouncementItem {
                        id: "announcement-2".to_string(),
                        url: None,
                        title: Some("Two".to_string()),
                        summary: None,
                        content_html: None,
                        date_modified: None,
                        author: None,
                        tags: BTreeSet::new(),
                        read: false,
                    },
                ],
            }))),
            releases: Mutex::new(Ok(Vec::new())),
        });
        let announcements = Arc::new(StubAnnouncementPort::default());
        *announcements
            .read_ids
            .lock()
            .expect("read ids stub should not be poisoned") = vec!["announcement-2".to_string()];
        let service = RemoteFeedService::new(feeds, announcements);

        let feed = service
            .announcements_for_user("user-1")
            .await
            .expect("announcements should load")
            .expect("announcements feed should exist");

        assert_eq!(feed.title, "Komga News");
        assert_eq!(feed.items[0].id, "announcement-1");
        assert_eq!(feed.items[0].title.as_deref(), Some("One"));
        assert!(!feed.items[0].read);
        assert_eq!(feed.items[1].id, "announcement-2");
        assert!(feed.items[1].read);
    }

    #[tokio::test]
    async fn save_announcements_read_deduplicates_duplicate_strings() {
        let feeds = Arc::new(StubRemoteFeedPort::default());
        let announcements = Arc::new(StubAnnouncementPort::default());
        let service = RemoteFeedService::new(feeds, announcements.clone());

        service
            .save_announcements_read(
                "user-1",
                &[
                    "announcement-1".to_string(),
                    "announcement-1".to_string(),
                    "announcement-2".to_string(),
                ],
            )
            .await
            .expect("announcement ids should persist");

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
    async fn releases_returns_typed_records() {
        let feeds = Arc::new(StubRemoteFeedPort {
            announcements: Mutex::new(Ok(None)),
            releases: Mutex::new(Ok(vec![RemoteRelease {
                version: "v1.0.0".to_string(),
                release_date: OffsetDateTime::parse("2024-01-02T03:04:05Z", &Rfc3339)
                    .expect("release date should parse"),
                url: "https://github.test/releases/v1".to_string(),
                latest: true,
                pre_release: false,
                description: "Release notes".to_string(),
            }])),
        });
        let announcements = Arc::new(StubAnnouncementPort::default());
        let service = RemoteFeedService::new(feeds, announcements);

        let releases = service.releases().await.expect("releases should load");

        assert_eq!(releases[0].version, "v1.0.0");
        assert!(releases[0].latest);
        assert!(!releases[0].pre_release);
        assert_eq!(
            releases[0]
                .release_date
                .format(&Rfc3339)
                .expect("release date should format"),
            "2024-01-02T03:04:05Z"
        );
    }

    #[test]
    fn remote_feed_cache_access_does_not_extend_fetch_ttl() {
        let mut cache = Some(RemoteFeedCacheEntry {
            fetched_at_epoch_seconds: 100,
            payload: "cached".to_string(),
        });

        assert_eq!(
            load_remote_cache_entry_on_access(
                &mut cache,
                100 + CACHE_TTL_SECONDS - 1,
                CACHE_TTL_SECONDS
            ),
            Some("cached".to_string())
        );

        assert_eq!(
            load_remote_cache_entry_on_access(
                &mut cache,
                100 + CACHE_TTL_SECONDS,
                CACHE_TTL_SECONDS
            ),
            None
        );
        assert_eq!(
            cache
                .as_ref()
                .expect("cache entry should remain available for refresh decision")
                .fetched_at_epoch_seconds,
            100
        );
    }
}
