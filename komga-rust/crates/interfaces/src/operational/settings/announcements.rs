use std::collections::BTreeSet;

use axum::Json;
use axum::body::Bytes;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use komga_application::operational::{
    RemoteAnnouncementAuthor, RemoteAnnouncementItem, RemoteAnnouncementsFeed, RemoteRelease,
};
use serde::Serialize;
use time::format_description::well_known::Rfc3339;

use crate::identity_access::auth::Admin;
use crate::state::OperationalApiState;
use komga_application::identity_access::user_id;

pub(crate) async fn get_announcements(
    State(app): State<OperationalApiState>,
    admin: Admin,
) -> Response {
    match app
        .remote_feeds
        .announcements_for_user(user_id(&admin))
        .await
    {
        Ok(Some(feed)) => Json(announcements_feed_payload(&feed)).into_response(),
        Ok(None) => StatusCode::NOT_FOUND.into_response(),
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
}

pub(crate) async fn put_announcements(
    State(app): State<OperationalApiState>,
    admin: Admin,
    body: Bytes,
) -> Response {
    let ids = match parse_announcement_ids(&body) {
        Ok(ids) => ids,
        Err(status) => return status.into_response(),
    };

    match app
        .remote_feeds
        .save_announcements_read(user_id(&admin), &ids)
        .await
    {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
}

pub(crate) async fn get_releases(
    State(app): State<OperationalApiState>,
    _admin: Admin,
) -> Response {
    match app.remote_feeds.releases().await {
        Ok(releases) => Json(releases_payload(&releases)).into_response(),
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
}

fn parse_announcement_ids(body: &[u8]) -> Result<Vec<String>, StatusCode> {
    serde_json::from_slice::<Vec<String>>(body).map_err(|_| StatusCode::BAD_REQUEST)
}

#[derive(Serialize)]
struct AnnouncementsFeedPayload<'a> {
    version: &'a str,
    title: &'a str,
    #[serde(rename = "home_page_url")]
    home_page_url: Option<&'a str>,
    description: Option<&'a str>,
    items: Vec<AnnouncementItemPayload<'a>>,
}

#[derive(Serialize)]
struct AnnouncementItemPayload<'a> {
    id: &'a str,
    url: Option<&'a str>,
    title: Option<&'a str>,
    summary: Option<&'a str>,
    #[serde(rename = "content_html")]
    content_html: Option<&'a str>,
    #[serde(rename = "date_modified")]
    date_modified: Option<String>,
    author: Option<AnnouncementAuthorPayload<'a>>,
    tags: &'a BTreeSet<String>,
    #[serde(rename = "_komga")]
    komga: AnnouncementKomgaPayload,
}

#[derive(Serialize)]
struct AnnouncementAuthorPayload<'a> {
    name: Option<&'a str>,
    url: Option<&'a str>,
}

#[derive(Serialize)]
struct AnnouncementKomgaPayload {
    read: bool,
}

#[derive(Serialize)]
struct ReleasePayload<'a> {
    version: &'a str,
    #[serde(rename = "releaseDate")]
    release_date: String,
    url: &'a str,
    latest: bool,
    #[serde(rename = "preRelease")]
    pre_release: bool,
    description: &'a str,
}

fn announcements_feed_payload(feed: &RemoteAnnouncementsFeed) -> AnnouncementsFeedPayload<'_> {
    AnnouncementsFeedPayload {
        version: &feed.version,
        title: &feed.title,
        home_page_url: feed.home_page_url.as_deref(),
        description: feed.description.as_deref(),
        items: feed.items.iter().map(announcement_item_payload).collect(),
    }
}

fn announcement_item_payload(item: &RemoteAnnouncementItem) -> AnnouncementItemPayload<'_> {
    AnnouncementItemPayload {
        id: &item.id,
        url: item.url.as_deref(),
        title: item.title.as_deref(),
        summary: item.summary.as_deref(),
        content_html: item.content_html.as_deref(),
        date_modified: item
            .date_modified
            .as_ref()
            .map(|date| date.format(&Rfc3339).expect("date_modified should format")),
        author: item.author.as_ref().map(announcement_author_payload),
        tags: &item.tags,
        komga: AnnouncementKomgaPayload { read: item.read },
    }
}

fn announcement_author_payload(author: &RemoteAnnouncementAuthor) -> AnnouncementAuthorPayload<'_> {
    AnnouncementAuthorPayload {
        name: author.name.as_deref(),
        url: author.url.as_deref(),
    }
}

fn releases_payload(releases: &[RemoteRelease]) -> Vec<ReleasePayload<'_>> {
    releases
        .iter()
        .map(|release| ReleasePayload {
            version: &release.version,
            release_date: release
                .release_date
                .format(&Rfc3339)
                .expect("release_date should format"),
            url: &release.url,
            latest: release.latest,
            pre_release: release.pre_release,
            description: &release.description,
        })
        .collect()
}
