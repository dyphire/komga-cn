use axum::Json;
use axum::body::Bytes;
use axum::extract::Path;
use axum::Router;
use axum::extract::Extension;
use axum::http::{HeaderMap, HeaderName, HeaderValue, StatusCode, Uri, header};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, patch};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use serde_json::{Value, json};
use tokio::net::TcpListener;

const LAST_MODIFIED: &str = "Mon, 01 Jan 2024 22:04:05 GMT";
const PAGE_BODY: &[u8] = b"\x89PNG\r\n\x1a\nplaceholder";
const THUMBNAIL_BODY: &[u8] = b"\xff\xd8\xff\xdb\x00C\x00placeholder-jpeg\xff\xd9";
const PDF_BODY: &[u8] = b"%PDF-1.7\n%komga-rust-placeholder\n";
const THUMBNAIL_ETAG: &str = "\"048bbf960d13687d84948688ab74aaa59\"";

const COMPAT_PROFILE_ENV: &str = "KOMGA_RUST_COMPAT_PROFILE";
const JAVA_LIVE_LOCALDB_PROFILE: &str = "java-live-localdb";

#[derive(Clone, Copy)]
struct PlaceholderUser {
    id: &'static str,
    email: &'static str,
    password: &'static str,
}

const PLACEHOLDER_USERS: &[PlaceholderUser] = &[
    PlaceholderUser {
        id: "admin",
        email: "admin@example.org",
        password: "admin",
    },
    PlaceholderUser {
        id: "user",
        email: "user@example.org",
        password: "user",
    },
];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CompatProfile {
    SnapshotAligned,
    JavaLiveLocaldb,
}

pub fn build_router() -> Router {
    build_router_with_profile(compat_profile_from_env())
}

pub fn build_router_with_profile(profile: CompatProfile) -> Router {
    Router::new()
        .route("/api/v1/libraries", get(libraries))
        .route("/api/v1/series", get(series))
        .route("/api/v1/books", get(books))
        .route("/api/v1/books/latest", get(books_latest))
        .route("/api/v1/books/{book_id}/pages", get(book_pages))
        .route(
            "/api/v1/books/{book_id}/pages/{page_number}",
            get(book_page),
        )
        .route(
            "/api/v1/books/{book_id}/pages/{page_number}/thumbnail",
            get(book_page_thumbnail),
        )
        .route("/api/v1/books/{book_id}/thumbnail", get(book_thumbnail))
        .route("/api/v1/books/{book_id}/file", get(book_file))
        .route(
            "/api/v1/books/{book_id}/read-progress",
            patch(book_read_progress),
        )
        .route("/api/v2/users/me", get(users_me))
        .route("/opds/v2/auth", get(opds_auth))
        .route("/opds/v2/catalog", get(opds_catalog))
        .route("/opds/v2/books/{book_id}/manifest", get(opds_manifest))
        .route("/api/v1/login/set-cookie", get(login_set_cookie))
        .layer(Extension(profile))
}

pub async fn serve(listener: TcpListener) -> std::io::Result<()> {
    axum::serve(listener, build_router()).await
}

async fn libraries(Extension(profile): Extension<CompatProfile>, headers: HeaderMap) -> Response {
    if let Some(response) = require_auth(&headers) {
        return response;
    }

    Json(snapshot_json("libraries-list-admin.json", profile)).into_response()
}

async fn series(Extension(profile): Extension<CompatProfile>, headers: HeaderMap) -> Response {
    if let Some(response) = require_auth(&headers) {
        return response;
    }

    Json(snapshot_json("series-list.json", profile)).into_response()
}

async fn books(Extension(profile): Extension<CompatProfile>, headers: HeaderMap) -> Response {
    if let Some(response) = require_auth(&headers) {
        return response;
    }

    Json(snapshot_json("books-list.json", profile)).into_response()
}

async fn books_latest(
    Extension(profile): Extension<CompatProfile>,
    headers: HeaderMap,
) -> Response {
    if let Some(response) = require_auth(&headers) {
        return response;
    }

    Json(books_latest_json(profile)).into_response()
}

async fn users_me(headers: HeaderMap, uri: Uri) -> Response {
    match basic_user(&headers) {
        AuthOutcome::Valid(user) if remember_me_requested(&uri) => {
            if empty_auth_token_supplied(&headers) {
                bootstrap_user_with_remember_me_token(user, resolved_token(&headers))
            } else {
                bootstrap_user_with_remember_me_cookies(user)
            }
        }
        AuthOutcome::Valid(user) => bootstrap_user(user, session_token(user)),
        AuthOutcome::Invalid => StatusCode::UNAUTHORIZED.into_response(),
        AuthOutcome::Missing => bootstrap_user(PLACEHOLDER_USERS[0], resolved_token(&headers)),
    }
}

fn bootstrap_user(user: PlaceholderUser, token: String) -> Response {
    let cookie = format!("KOMGA-SESSION={token}; Path=/");

    (
        StatusCode::OK,
        [
            (
                HeaderName::from_static("x-auth-token"),
                HeaderValue::from_str(&token)
                    .unwrap_or_else(|_| HeaderValue::from_static("generated-token")),
            ),
            (
                header::SET_COOKIE,
                HeaderValue::from_str(&cookie)
                    .unwrap_or_else(|_| HeaderValue::from_static("KOMGA-SESSION=; Path=/")),
            ),
        ],
        Json(placeholder_user_json(user)),
    )
        .into_response()
}

fn bootstrap_user_with_remember_me_cookies(user: PlaceholderUser) -> Response {
    let session_cookie = format!(
        "KOMGA-SESSION={}; Path=/; HttpOnly; SameSite=Lax",
        session_token(user)
    );
    let remember_me_cookie = format!(
        "komga-remember-me={}; Path=/; HttpOnly; Max-Age=2592000; Expires=Sun, 18 Apr 2038 23:59:59 GMT",
        remember_me_token(user)
    );

    let mut response = (StatusCode::OK, Json(placeholder_user_json(user))).into_response();
    let headers = response.headers_mut();
    headers.append(
        header::SET_COOKIE,
        HeaderValue::from_str(&remember_me_cookie)
            .unwrap_or_else(|_| HeaderValue::from_static("komga-remember-me=; Path=/; HttpOnly")),
    );
    headers.append(
        header::SET_COOKIE,
        HeaderValue::from_str(&session_cookie).unwrap_or_else(|_| {
            HeaderValue::from_static("KOMGA-SESSION=; Path=/; HttpOnly; SameSite=Lax")
        }),
    );
    response
}

fn bootstrap_user_with_remember_me_token(user: PlaceholderUser, token: String) -> Response {
    let remember_me_cookie = format!(
        "komga-remember-me={}; Path=/; HttpOnly; Max-Age=2592000; Expires=Sun, 18 Apr 2038 23:59:59 GMT",
        remember_me_token(user)
    );

    let mut response = (StatusCode::OK, Json(placeholder_user_json(user))).into_response();
    let headers = response.headers_mut();
    headers.insert(
        HeaderName::from_static("x-auth-token"),
        HeaderValue::from_str(&token)
            .unwrap_or_else(|_| HeaderValue::from_static("generated-token")),
    );
    headers.append(
        header::SET_COOKIE,
        HeaderValue::from_str(&remember_me_cookie)
            .unwrap_or_else(|_| HeaderValue::from_static("komga-remember-me=; Path=/; HttpOnly")),
    );
    response
}

async fn login_set_cookie(headers: HeaderMap) -> Response {
    if let Some(response) = require_auth(&headers) {
        return response;
    }

    let token = resolved_token(&headers);

    let cookie = format!("KOMGA-SESSION={token}; Path=/");

    (
        StatusCode::NO_CONTENT,
        [(
            header::SET_COOKIE,
            HeaderValue::from_str(&cookie)
                .unwrap_or_else(|_| HeaderValue::from_static("KOMGA-SESSION=; Path=/")),
        )],
    )
        .into_response()
}

async fn book_page(Extension(profile): Extension<CompatProfile>, headers: HeaderMap) -> Response {
    if let Some(response) = require_auth(&headers) {
        return response;
    }

    if headers
        .get(header::IF_MODIFIED_SINCE)
        .and_then(|value| value.to_str().ok())
        == Some(LAST_MODIFIED)
    {
        return (
            StatusCode::NOT_MODIFIED,
            [(header::LAST_MODIFIED, LAST_MODIFIED)],
        )
            .into_response();
    }

    if profile == CompatProfile::JavaLiveLocaldb {
        return (
            StatusCode::OK,
            [
                (header::CONTENT_TYPE, "image/png"),
                (header::LAST_MODIFIED, LAST_MODIFIED),
                (
                    header::CONTENT_DISPOSITION,
                    "inline; filename=\"=?UTF-8?Q?book.cbr-1.png?=\"; filename*=UTF-8''book.cbr-1.png",
                ),
            ],
            PAGE_BODY,
        )
            .into_response();
    }

    (
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, "application/pdf"),
            (header::LAST_MODIFIED, LAST_MODIFIED),
        ],
        PDF_BODY,
    )
        .into_response()
}

async fn book_page_thumbnail(
    Extension(_profile): Extension<CompatProfile>,
    headers: HeaderMap,
) -> Response {
    if let Some(response) = require_auth(&headers) {
        return response;
    }

    if headers
        .get(header::IF_MODIFIED_SINCE)
        .and_then(|value| value.to_str().ok())
        == Some(LAST_MODIFIED)
    {
        return (
            StatusCode::NOT_MODIFIED,
            [(header::LAST_MODIFIED, LAST_MODIFIED)],
        )
            .into_response();
    }

    (
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, "image/jpeg"),
            (header::CACHE_CONTROL, "max-age=0, must-revalidate, private"),
            (header::LAST_MODIFIED, LAST_MODIFIED),
            (header::ETAG, THUMBNAIL_ETAG),
        ],
        THUMBNAIL_BODY,
    )
        .into_response()
}

async fn book_thumbnail(Extension(profile): Extension<CompatProfile>, headers: HeaderMap) -> Response {
    if let Some(response) = require_auth(&headers) {
        return response;
    }

    if profile == CompatProfile::JavaLiveLocaldb {
        return StatusCode::NOT_FOUND.into_response();
    }

    StatusCode::NOT_FOUND.into_response()
}

async fn book_pages(Extension(profile): Extension<CompatProfile>, headers: HeaderMap) -> Response {
    if let Some(response) = require_auth(&headers) {
        return response;
    }

    Json(book_pages_json(profile)).into_response()
}

async fn book_file(Extension(profile): Extension<CompatProfile>, headers: HeaderMap) -> Response {
    if let Some(response) = require_auth(&headers) {
        return response;
    }

    if profile == CompatProfile::JavaLiveLocaldb {
        return (
            StatusCode::OK,
            [
                (header::CONTENT_TYPE, "application/zip"),
                (
                    header::CONTENT_DISPOSITION,
                    "attachment; filename=\"=?UTF-8?Q?book.cbr?=\"; filename*=UTF-8''book.cbr",
                ),
            ],
            PAGE_BODY,
        )
            .into_response();
    }

    (
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, "application/pdf"),
            (header::CONTENT_DISPOSITION, "attachment; filename=book.pdf"),
        ],
        PDF_BODY,
    )
        .into_response()
}

async fn book_read_progress(
    Extension(_profile): Extension<CompatProfile>,
    headers: HeaderMap,
    Path(book_id): Path<String>,
    body: Bytes,
) -> Response {
    if let Some(response) = require_auth(&headers) {
        return response;
    }

    if book_id != "book-1" {
        return StatusCode::NOT_FOUND.into_response();
    }

    let Ok(payload) = serde_json::from_slice::<Value>(&body) else {
        return invalid_read_progress_payload();
    };

    if payload
        .get("completed")
        .and_then(|value| value.as_bool())
        == Some(true)
        || payload.get("page").and_then(|value| value.as_u64()) == Some(1)
    {
        return StatusCode::NO_CONTENT.into_response();
    }

    invalid_read_progress_payload()
}

async fn opds_manifest(
    Extension(profile): Extension<CompatProfile>,
    headers: HeaderMap,
) -> Response {
    if let Some(response) = require_auth(&headers) {
        return response;
    }

    if profile == CompatProfile::JavaLiveLocaldb {
        return (
            StatusCode::OK,
            [(header::CONTENT_TYPE, "application/opds-publication+json")],
            Json(java_live_opds_manifest(&headers)),
        )
            .into_response();
    }

    (
        StatusCode::OK,
        [(header::CONTENT_TYPE, "application/opds-publication+json")],
        Json(snapshot_json(
            "opds-v2-manifest.json",
            CompatProfile::SnapshotAligned,
        )),
    )
        .into_response()
}

async fn opds_auth(headers: HeaderMap) -> Response {
    (
        StatusCode::OK,
        [(
            header::CONTENT_TYPE,
            HeaderValue::from_static("application/opds-authentication+json"),
        )],
        Json(opds_auth_json(&headers)),
    )
        .into_response()
}

async fn opds_catalog(headers: HeaderMap) -> Response {
    let host = request_host(&headers);
    let auth_href = absolute_url(&host, "/opds/v2/auth");
    let link = format!(
        "<{}>; rel=\"http://opds-spec.org/auth/document\"; type=\"application/opds-authentication+json\"",
        auth_href
    );

    (
        StatusCode::UNAUTHORIZED,
        [
            (
                header::WWW_AUTHENTICATE,
                HeaderValue::from_static("Basic realm=\"Realm\""),
            ),
            (header::LINK, HeaderValue::from_str(&link).unwrap()),
            (
                header::CONTENT_TYPE,
                HeaderValue::from_static("application/opds-authentication+json;charset=UTF-8"),
            ),
        ],
        Json(opds_auth_json(&headers)),
    )
        .into_response()
}

fn require_auth(headers: &HeaderMap) -> Option<Response> {
    let token = headers
        .get("X-Auth-Token")
        .and_then(|value| value.to_str().ok())
        .filter(|value| !value.trim().is_empty());

    if token.is_some() {
        None
    } else {
        Some(StatusCode::UNAUTHORIZED.into_response())
    }
}

fn invalid_read_progress_payload() -> Response {
    (
        StatusCode::BAD_REQUEST,
        [(header::CONTENT_TYPE, "application/json")],
        Json(json!({
            "error": "invalid read progress payload",
        })),
    )
        .into_response()
}

fn resolved_token(headers: &HeaderMap) -> String {
    headers
        .get("X-Auth-Token")
        .and_then(|value| value.to_str().ok())
        .filter(|value| !value.trim().is_empty())
        .unwrap_or("generated-token")
        .to_string()
}

fn empty_auth_token_supplied(headers: &HeaderMap) -> bool {
    headers
        .get("X-Auth-Token")
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value.trim().is_empty())
}

fn session_token(user: PlaceholderUser) -> String {
    format!("komga-{}-token", user.id)
}

fn remember_me_token(user: PlaceholderUser) -> String {
    format!("komga-{}-remember-me-token", user.id)
}

fn placeholder_user_json(user: PlaceholderUser) -> Value {
    if user.email == "user@example.org" {
        json!({
            "id": "0PTTX3XD04FM0",
            "email": "user@example.org",
            "roles": ["FILE_DOWNLOAD", "PAGE_STREAMING", "USER"],
            "sharedAllLibraries": true,
            "sharedLibrariesIds": [],
            "labelsAllow": [],
            "labelsExclude": [],
            "ageRestriction": null,
        })
    } else {
        json!({
            "id": user.id,
            "email": user.email
        })
    }
}

fn basic_user(headers: &HeaderMap) -> AuthOutcome {
    let Some(value) = headers
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
    else {
        return AuthOutcome::Missing;
    };

    let value = value.trim();
    if value.is_empty() {
        return AuthOutcome::Missing;
    }

    let Some(encoded) = value.strip_prefix("Basic ") else {
        return AuthOutcome::Invalid;
    };

    let decoded = match STANDARD.decode(encoded) {
        Ok(decoded) => decoded,
        Err(_) => return AuthOutcome::Invalid,
    };

    let credentials = match String::from_utf8(decoded) {
        Ok(credentials) => credentials,
        Err(_) => return AuthOutcome::Invalid,
    };

    let Some((username, password)) = credentials.split_once(':') else {
        return AuthOutcome::Invalid;
    };

    PLACEHOLDER_USERS
        .iter()
        .copied()
        .find(|user| user.email == username && user.password == password)
        .map(AuthOutcome::Valid)
        .unwrap_or(AuthOutcome::Invalid)
}

enum AuthOutcome {
    Valid(PlaceholderUser),
    Invalid,
    Missing,
}

fn compat_profile_from_env() -> CompatProfile {
    match std::env::var(COMPAT_PROFILE_ENV).as_deref() {
        Ok(JAVA_LIVE_LOCALDB_PROFILE) => CompatProfile::JavaLiveLocaldb,
        _ => CompatProfile::SnapshotAligned,
    }
}

fn remember_me_requested(uri: &Uri) -> bool {
    uri.query()
        .is_some_and(|query| query.split('&').any(|pair| pair == "remember-me=true"))
}

fn snapshot_json(path: &str, profile: CompatProfile) -> Value {
    let json = match path {
        "libraries-list-admin.json" => include_str!(
            "../../komga/src/test/resources/compatibility-snapshots/rest/libraries-list-admin.json"
        ),
        "series-list.json" => include_str!(
            "../../komga/src/test/resources/compatibility-snapshots/rest/series-list.json"
        ),
        "books-list.json" => include_str!(
            "../../komga/src/test/resources/compatibility-snapshots/rest/books-list.json"
        ),
        "opds-v2-manifest.json" => include_str!(
            "../../komga/src/test/resources/compatibility-snapshots/opds/opds-v2-manifest.json"
        ),
        other => panic!("unsupported snapshot: {other}"),
    };

    let mut value: Value = serde_json::from_str(json).expect("snapshot json should be valid");

    match (path, profile) {
        ("libraries-list-admin.json", CompatProfile::JavaLiveLocaldb) => {
            if let Some(root) = value.pointer_mut("/0/root") {
                *root = Value::String(String::new());
            }
        }
        ("series-list.json", CompatProfile::JavaLiveLocaldb) => {
            if let Some(url) = value.pointer_mut("/content/0/url") {
                *url = Value::String(String::new());
            }
        }
        ("books-list.json", CompatProfile::JavaLiveLocaldb) => {
            if let Some(url) = value.pointer_mut("/content/0/url") {
                *url = Value::String("book.cbr".to_string());
            }
            if let Some(file_last_modified) = value.pointer_mut("/content/0/fileLastModified") {
                *file_last_modified = Value::String("2024-01-02T08:04:05Z".to_string());
            }
            if let Some(status) = value.pointer_mut("/content/0/media/status") {
                *status = Value::String("READY".to_string());
            }
            if let Some(media_type) = value.pointer_mut("/content/0/media/mediaType") {
                *media_type = Value::String("application/zip".to_string());
            }
            if let Some(pages_count) = value.pointer_mut("/content/0/media/pagesCount") {
                *pages_count = Value::Number(1.into());
            }
            if let Some(media_profile) = value.pointer_mut("/content/0/media/mediaProfile") {
                *media_profile = Value::String("DIVINA".to_string());
            }
        }
        _ => {}
    }

    value
}

fn opds_auth_json(headers: &HeaderMap) -> Value {
    let host = request_host(headers);

    json!({
        "authentication": [
            {
                "type": "http://opds-spec.org/auth/basic",
                "labels": {
                    "login": "Email",
                    "password": "Password"
                }
            }
        ],
        "title": "Komga",
        "id": absolute_url(&host, "/opds/v2/auth"),
        "description": "Enter your email and password to authenticate.",
        "links": [
            {
                "rel": "help",
                "href": "https://komga.org"
            },
            {
                "rel": "logo",
                "href": absolute_url(&host, "/android-chrome-512x512.png")
            }
        ]
    })
}

fn books_latest_json(profile: CompatProfile) -> Value {
    let mut value = snapshot_json("books-list.json", profile);

    if profile == CompatProfile::JavaLiveLocaldb {
        if let Some(sort_sorted) = value.pointer_mut("/sort/sorted") {
            *sort_sorted = Value::Bool(true);
        }
        if let Some(sort_unsorted) = value.pointer_mut("/sort/unsorted") {
            *sort_unsorted = Value::Bool(false);
        }
        if let Some(sort_empty) = value.pointer_mut("/sort/empty") {
            *sort_empty = Value::Bool(false);
        }
        if let Some(pageable_sort_sorted) = value.pointer_mut("/pageable/sort/sorted") {
            *pageable_sort_sorted = Value::Bool(true);
        }
        if let Some(pageable_sort_unsorted) = value.pointer_mut("/pageable/sort/unsorted") {
            *pageable_sort_unsorted = Value::Bool(false);
        }
        if let Some(pageable_sort_empty) = value.pointer_mut("/pageable/sort/empty") {
            *pageable_sort_empty = Value::Bool(false);
        }
    }

    value
}

fn book_pages_json(_: CompatProfile) -> Value {
    json!([
        {
            "number": 1,
            "fileName": "komga.png",
            "mediaType": "image/png",
            "width": null,
            "height": null,
            "sizeBytes": 0,
            "size": "0 B",
        }
    ])
}

fn java_live_opds_manifest(headers: &HeaderMap) -> Value {
    let host = request_host(headers);

    json!({
        "context": "https://readium.org/webpub-manifest/context.jsonld",
        "metadata": {
            "title": "book.cbr",
            "modified": "2024-01-01T22:04:05-05:00",
            "conformsTo": "https://readium.org/webpub-manifest/profiles/divina",
            "numberOfPages": 1,
            "belongsTo": {
                "series": [
                    {
                        "name": "series",
                        "position": 1.0,
                        "links": [
                            {
                                "href": absolute_url(&host, "/opds/v2/series/series-1"),
                                "type": "application/opds+json",
                            }
                        ],
                    }
                ]
            }
        },
        "links": [
            {
                "rel": "self",
                "href": absolute_url(&host, "/opds/v2/books/book-1/manifest"),
                "type": "application/divina+json",
                "properties": {
                    "authenticate": {
                        "href": absolute_url(&host, "/opds/v2/auth"),
                        "type": "application/opds-authentication+json",
                    }
                }
            },
            {
                "rel": "http://opds-spec.org/acquisition",
                "href": absolute_url(&host, "/opds/v2/books/book-1/file"),
                "type": "application/vnd.comicbook+zip",
                "properties": {
                    "authenticate": {
                        "href": absolute_url(&host, "/opds/v2/auth"),
                        "type": "application/opds-authentication+json",
                    }
                }
            },
            {
                "rel": "http://www.cantook.com/api/progression",
                "href": absolute_url(&host, "/opds/v2/books/book-1/progression"),
                "type": "application/vnd.readium.progression+json",
                "properties": {
                    "authenticate": {
                        "href": absolute_url(&host, "/opds/v2/auth"),
                        "type": "application/opds-authentication+json",
                    }
                }
            }
        ],
        "images": [],
        "readingOrder": [
            {
                "href": absolute_url(&host, "/opds/v2/books/book-1/pages/1?contentNegotiation=false"),
                "type": "image/png",
            }
        ],
        "resources": [
            {
                "href": absolute_url(&host, "/opds/v2/books/book-1/thumbnail"),
                "type": "image/jpeg",
                "properties": {
                    "authenticate": {
                        "href": absolute_url(&host, "/opds/v2/auth"),
                        "type": "application/opds-authentication+json",
                    }
                }
            }
        ],
        "toc": [],
        "landmarks": [],
        "pageList": [],
    })
}

fn request_host(headers: &HeaderMap) -> String {
    headers
        .get(header::HOST)
        .and_then(|value| value.to_str().ok())
        .filter(|value| !value.trim().is_empty())
        .unwrap_or("localhost")
        .to_string()
}

fn absolute_url(host: &str, path: &str) -> String {
    format!("http://{host}{path}")
}
