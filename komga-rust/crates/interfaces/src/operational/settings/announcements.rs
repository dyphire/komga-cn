use axum::Json;
use axum::body::Bytes;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;

use crate::identity_access::auth::{require_admin, resolved_auth_user, user_id};
use crate::state::HttpAppState;

pub(crate) async fn get_announcements(
    State(app): State<Arc<HttpAppState>>,
    headers: HeaderMap,
) -> Response {
    if let Some(response) = require_admin(&*app.services.runtime_identity, &headers) {
        return response;
    }

    let Some(current_user) = resolved_auth_user(&*app.services.runtime_identity, &headers) else {
        return StatusCode::UNAUTHORIZED.into_response();
    };

    let feed = match load_cached_announcements_feed(&app).await {
        Ok(Some(feed)) => feed,
        Ok(None) => return StatusCode::NOT_FOUND.into_response(),
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };

    let read_ids = match app
        .services
        .operational_settings
        .load_announcement_read_ids(user_id(&current_user).to_string())
        .await
    {
        Ok(ids) => ids,
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };

    Json(apply_announcement_read_projection(feed, &read_ids)).into_response()
}

pub(crate) async fn put_announcements(
    State(app): State<Arc<HttpAppState>>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    if let Some(response) = require_admin(&*app.services.runtime_identity, &headers) {
        return response;
    }

    let Some(current_user) = resolved_auth_user(&*app.services.runtime_identity, &headers) else {
        return StatusCode::UNAUTHORIZED.into_response();
    };

    let Ok(ids) = parse_announcement_ids(&body) else {
        return StatusCode::BAD_REQUEST.into_response();
    };

    if app
        .services
        .operational_settings
        .save_announcements_read(user_id(&current_user).to_string(), ids)
        .await
        .is_err()
    {
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    }

    StatusCode::NO_CONTENT.into_response()
}

fn parse_announcement_ids(body: &[u8]) -> Result<Vec<String>, ()> {
    let payload = serde_json::from_slice::<Value>(body).map_err(|_| ())?;
    let announcement_ids = payload.as_array().ok_or(())?;

    let mut ids = Vec::with_capacity(announcement_ids.len());
    let mut seen = std::collections::BTreeSet::new();
    for id in announcement_ids {
        let Some(id) = id.as_str() else {
            return Err(());
        };
        let id = id.to_string();
        if seen.insert(id.clone()) {
            ids.push(id);
        }
    }

    Ok(ids)
}

pub(crate) async fn get_releases(
    State(app): State<Arc<HttpAppState>>,
    headers: HeaderMap,
) -> Response {
    if let Some(response) = require_admin(&*app.services.runtime_identity, &headers) {
        return response;
    }

    let releases = match load_cached_releases(&app).await {
        Ok(Some(releases)) => releases,
        Ok(None) => return StatusCode::NOT_FOUND.into_response(),
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };

    Json(releases).into_response()
}

async fn load_cached_announcements_feed(app: &HttpAppState) -> Result<Option<Value>, String> {
    const CACHE_TTL_SECONDS: u64 = 60 * 60;
    let now = now_epoch_seconds();
    {
        let mut cache = app
            .operational
            .announcements_cache
            .lock()
            .expect("announcements cache lock should not be poisoned");
        if let Some(payload) = load_remote_cache_entry_on_access(&mut cache, now, CACHE_TTL_SECONDS)
        {
            return Ok(Some(payload));
        }
    }

    let url = std::env::var("KOMGA_RUST_ANNOUNCEMENTS_URL")
        .unwrap_or_else(|_| "https://komga.org/blog/feed.json".to_string());
    let response = Client::new()
        .get(url)
        .send()
        .await
        .map_err(|error| error.to_string())?
        .error_for_status()
        .map_err(|error| error.to_string())?;
    let bytes = response.bytes().await.map_err(|error| error.to_string())?;
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

    {
        let mut cache = app
            .operational
            .announcements_cache
            .lock()
            .expect("announcements cache lock should not be poisoned");
        *cache = Some(crate::state::RemoteCacheEntry {
            fetched_at_epoch_seconds: now,
            payload: payload.clone(),
        });
    }

    Ok(Some(payload))
}

fn load_remote_cache_entry_on_access(
    cache: &mut Option<crate::state::RemoteCacheEntry>,
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
    #[serde(with = "optional_rfc3339")]
    #[serde(rename = "date_modified")]
    date_modified: Option<OffsetDateTime>,
    author: Option<AnnouncementAuthorDto>,
    #[serde(default)]
    tags: std::collections::BTreeSet<String>,
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

async fn load_cached_releases(app: &HttpAppState) -> Result<Option<Value>, reqwest::Error> {
    const CACHE_TTL_SECONDS: u64 = 60 * 60;
    let now = now_epoch_seconds();
    {
        let mut cache = app
            .operational
            .releases_cache
            .lock()
            .expect("releases cache lock should not be poisoned");
        if let Some(payload) = load_remote_cache_entry_on_access(&mut cache, now, CACHE_TTL_SECONDS)
        {
            return Ok(Some(payload));
        }
    }

    let url = std::env::var("KOMGA_RUST_RELEASES_URL").unwrap_or_else(|_| {
        "https://api.github.com/repos/huihuimoe/komga-riir/releases?per_page=20".to_string()
    });
    let upstream = Client::new()
        .get(url)
        .header("User-Agent", "komga-rust-runtime")
        .send()
        .await?
        .error_for_status()?
        .json::<Vec<GithubReleaseUpstreamDto>>()
        .await?;

    let payload = map_github_releases(upstream);
    {
        let mut cache = app
            .operational
            .releases_cache
            .lock()
            .expect("releases cache lock should not be poisoned");
        *cache = Some(crate::state::RemoteCacheEntry {
            fetched_at_epoch_seconds: now,
            payload: payload.clone(),
        });
    }

    Ok(Some(payload))
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
    let read_set = read_ids
        .iter()
        .cloned()
        .collect::<std::collections::BTreeSet<_>>();

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
    use super::{load_remote_cache_entry_on_access, parse_announcement_ids};
    use crate::state::RemoteCacheEntry;
    use serde_json::json;

    #[test]
    fn parse_announcement_ids_deduplicates_duplicate_strings() {
        let ids =
            parse_announcement_ids(br#"["announcement-1","announcement-1","announcement-2"]"#)
                .expect("announcement ids should parse");

        assert_eq!(
            ids,
            vec!["announcement-1".to_string(), "announcement-2".to_string()]
        );
    }

    #[test]
    fn load_remote_cache_entry_on_access_refreshes_timestamp_for_fresh_hit() {
        let mut cache = Some(RemoteCacheEntry {
            fetched_at_epoch_seconds: 100,
            payload: json!({"title": "Komga News"}),
        });

        let payload = load_remote_cache_entry_on_access(&mut cache, 150, 3600)
            .expect("fresh cache hit should return payload");

        assert_eq!(payload["title"], "Komga News");
        assert_eq!(
            cache
                .as_ref()
                .expect("cache entry should remain present")
                .fetched_at_epoch_seconds,
            150
        );
    }
}
