use axum::Json;
use axum::body::Bytes;
use axum::extract::Extension;
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use reqwest::Client;
use serde_json::{Value, json};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::http::identity_access::auth::{require_admin, resolved_auth_user, user_id};
use crate::operational_settings_access::announcements as announcements_access;

use super::super::super::OperationalState;

pub(crate) async fn get_announcements(
    Extension(state): Extension<OperationalState>,
    headers: HeaderMap,
) -> Response {
    if let Some(response) = require_admin(&headers) {
        return response;
    }

    let Some(current_user) = resolved_auth_user(&headers) else {
        return StatusCode::UNAUTHORIZED.into_response();
    };

    let feed = match load_cached_announcements_feed(&state).await {
        Ok(Some(feed)) => feed,
        Ok(None) => return StatusCode::NOT_FOUND.into_response(),
        Err(_) => return StatusCode::NOT_FOUND.into_response(),
    };

    let read_ids = match announcements_access::load_announcement_read_ids(
        state.runtime.database_file.as_path(),
        user_id(&current_user),
    )
    .await
    {
        Ok(ids) => ids,
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };

    Json(apply_announcement_read_projection(feed, &read_ids)).into_response()
}

pub(crate) async fn put_announcements(
    Extension(state): Extension<OperationalState>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    if let Some(response) = require_admin(&headers) {
        return response;
    }

    let Some(current_user) = resolved_auth_user(&headers) else {
        return StatusCode::UNAUTHORIZED.into_response();
    };

    let Ok(payload) = serde_json::from_slice::<Value>(&body) else {
        return StatusCode::BAD_REQUEST.into_response();
    };
    let Some(announcement_ids) = payload.as_array() else {
        return StatusCode::BAD_REQUEST.into_response();
    };

    let mut ids = Vec::with_capacity(announcement_ids.len());
    for id in announcement_ids {
        let Some(id) = id.as_str().map(str::trim).filter(|value| !value.is_empty()) else {
            return StatusCode::BAD_REQUEST.into_response();
        };
        ids.push(id.to_string());
    }

    if announcements_access::save_announcements_read(
        state.runtime.database_file.as_path(),
        user_id(&current_user),
        &ids,
    )
    .await
    .is_err()
    {
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    }

    StatusCode::NO_CONTENT.into_response()
}

pub(crate) async fn get_releases(
    Extension(state): Extension<OperationalState>,
    headers: HeaderMap,
) -> Response {
    if let Some(response) = require_admin(&headers) {
        return response;
    }

    let releases = match load_cached_releases(&state).await {
        Ok(Some(releases)) => releases,
        Ok(None) => return StatusCode::NOT_FOUND.into_response(),
        Err(_) => return StatusCode::NOT_FOUND.into_response(),
    };

    Json(releases).into_response()
}

async fn load_cached_announcements_feed(
    state: &OperationalState,
) -> Result<Option<Value>, reqwest::Error> {
    const CACHE_TTL_SECONDS: u64 = 60 * 60;
    let now = now_epoch_seconds();
    {
        let cache = state
            .announcements_cache
            .lock()
            .expect("announcements cache lock should not be poisoned");
        if let Some(cached) = cache.as_ref()
            && now.saturating_sub(cached.fetched_at_epoch_seconds) < CACHE_TTL_SECONDS
        {
            return Ok(Some(cached.payload.clone()));
        }
    }

    let url = std::env::var("KOMGA_RUST_ANNOUNCEMENTS_URL")
        .unwrap_or_else(|_| "https://komga.org/blog/feed.json".to_string());
    let payload = Client::new().get(url).send().await?.json::<Value>().await?;

    {
        let mut cache = state
            .announcements_cache
            .lock()
            .expect("announcements cache lock should not be poisoned");
        *cache = Some(super::super::super::RemoteCacheEntry {
            fetched_at_epoch_seconds: now,
            payload: payload.clone(),
        });
    }

    Ok(Some(payload))
}

async fn load_cached_releases(state: &OperationalState) -> Result<Option<Value>, reqwest::Error> {
    const CACHE_TTL_SECONDS: u64 = 60 * 60;
    let now = now_epoch_seconds();
    {
        let cache = state
            .releases_cache
            .lock()
            .expect("releases cache lock should not be poisoned");
        if let Some(cached) = cache.as_ref()
            && now.saturating_sub(cached.fetched_at_epoch_seconds) < CACHE_TTL_SECONDS
        {
            return Ok(Some(cached.payload.clone()));
        }
    }

    let url = std::env::var("KOMGA_RUST_RELEASES_URL").unwrap_or_else(|_| {
        "https://api.github.com/repos/gotson/komga/releases?per_page=20".to_string()
    });
    let upstream = Client::new()
        .get(url)
        .header("User-Agent", "komga-rust-runtime")
        .send()
        .await?
        .json::<Value>()
        .await?;

    let payload = map_github_releases(upstream);
    {
        let mut cache = state
            .releases_cache
            .lock()
            .expect("releases cache lock should not be poisoned");
        *cache = Some(super::super::super::RemoteCacheEntry {
            fetched_at_epoch_seconds: now,
            payload: payload.clone(),
        });
    }

    Ok(Some(payload))
}

fn map_github_releases(upstream: Value) -> Value {
    let Some(items) = upstream.as_array() else {
        return Value::Array(Vec::new());
    };

    Value::Array(
        items
            .iter()
            .enumerate()
            .map(|(index, release)| {
                json!({
                    "version": release.get("tag_name").cloned().unwrap_or(Value::Null),
                    "releaseDate": release.get("published_at").cloned().unwrap_or(Value::Null),
                    "url": release.get("html_url").cloned().unwrap_or(Value::Null),
                    "latest": index == 0,
                    "preRelease": release.get("prerelease").cloned().unwrap_or(Value::Bool(false)),
                    "description": release.get("body").cloned().unwrap_or(Value::Null),
                })
            })
            .collect(),
    )
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
