use super::*;

pub(super) static JAVA_LIVE_BASE_URL_ENV_LOCK: LazyLock<tokio::sync::Mutex<()>> =
    LazyLock::new(|| tokio::sync::Mutex::new(()));

pub(super) fn expected_snapshot(name: &str) -> Value {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../komga/src/test/resources/compatibility-snapshots/rest")
        .join(name);
    serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap()
}

pub(super) async fn libraries_json_for_token<S>(app: &S, token: &str) -> Value
where
    S: tower::Service<Request<Body>, Response = axum::response::Response> + Clone,
    S::Error: std::fmt::Debug,
    S::Future: Send,
{
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v1/libraries")
                .header("X-Auth-Token", token)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    serde_json::from_slice(&body).unwrap()
}

pub(super) fn expected_opds_snapshot(name: &str) -> Value {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../komga/src/test/resources/compatibility-snapshots/opds")
        .join(name);
    serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap()
}

pub(super) async fn assert_opds_auth_for_host(host: &str) {
    let app = komga_rust::app::build_router();

    let response = app
        .oneshot(
            Request::builder()
                .uri("/opds/v2/auth")
                .header(header::HOST, host)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response.headers().get(header::CONTENT_TYPE).unwrap(),
        "application/opds-authentication+json"
    );

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json, expected_opds_auth(host));
}

pub(super) async fn assert_opds_catalog_challenge_for_host(host: &str) {
    let app = komga_rust::app::build_router();

    let response = app
        .oneshot(
            Request::builder()
                .uri("/opds/v2/catalog")
                .header(header::HOST, host)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(
        response.headers().get(header::WWW_AUTHENTICATE).unwrap(),
        "Basic realm=\"Realm\""
    );
    assert_eq!(
        response
            .headers()
            .get(header::LINK)
            .unwrap()
            .to_str()
            .unwrap(),
        format!(
            "<http://{host}/opds/v2/auth>; rel=\"http://opds-spec.org/auth/document\"; type=\"application/opds-authentication+json\""
        )
    );
    assert_eq!(
        response.headers().get(header::CONTENT_TYPE).unwrap(),
        "application/opds-authentication+json;charset=UTF-8"
    );

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json, expected_opds_auth(host));
}

fn expected_opds_auth(host: &str) -> Value {
    let base = format!("http://{host}");

    serde_json::json!({
        "authentication": [
            {
                "type": "http://opds-spec.org/auth/basic",
                "labels": {
                    "login": "Email",
                    "password": "Password",
                },
            }
        ],
        "title": "Komga",
        "id": format!("{base}/opds/v2/auth"),
        "description": "Enter your email and password to authenticate.",
        "links": [
            {
                "rel": "help",
                "href": "https://komga.org",
            },
            {
                "rel": "logo",
                "href": format!("{base}/android-chrome-512x512.png"),
            }
        ]
    })
}

fn expected_java_live_manifest(host: &str) -> Value {
    let base = format!("http://{host}");

    serde_json::json!({
        "context": "https://readium.org/webpub-manifest/context.jsonld",
        "metadata": {
            "title": "book.cbr",
            "modified": "2026-03-21T09:08:28-04:00",
            "conformsTo": "https://readium.org/webpub-manifest/profiles/divina",
            "numberOfPages": 1,
            "published": "2024-01-01",
            "belongsTo": {
                "series": [
                    {
                        "name": "series",
                        "position": 1.0,
                        "links": [
                            {
                                "href": format!("{base}/opds/v2/series/series-1"),
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
                "href": format!("{base}/opds/v2/books/book-1/manifest"),
                "type": "application/divina+json",
                "properties": {
                    "authenticate": {
                        "href": format!("{base}/opds/v2/auth"),
                        "type": "application/opds-authentication+json",
                    }
                }
            },
            {
                "rel": "http://opds-spec.org/acquisition",
                "href": format!("{base}/opds/v2/books/book-1/file"),
                "type": "application/vnd.comicbook+zip",
                "properties": {
                    "authenticate": {
                        "href": format!("{base}/opds/v2/auth"),
                        "type": "application/opds-authentication+json",
                    }
                }
            },
            {
                "rel": "http://www.cantook.com/api/progression",
                "href": format!("{base}/opds/v2/books/book-1/progression"),
                "type": "application/vnd.readium.progression+json",
                "properties": {
                    "authenticate": {
                        "href": format!("{base}/opds/v2/auth"),
                        "type": "application/opds-authentication+json",
                    }
                }
            }
        ],
        "images": [],
        "readingOrder": [
            {
                "href": format!("{base}/opds/v2/books/book-1/pages/1?contentNegotiation=false"),
                "type": "image/png",
                "width": 1,
                "height": 1,
            }
        ],
        "resources": [
            {
                "href": format!("{base}/opds/v2/books/book-1/thumbnail"),
                "type": "image/jpeg",
                "properties": {
                    "authenticate": {
                        "href": format!("{base}/opds/v2/auth"),
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

pub(super) async fn assert_java_live_manifest_for_host(host: &str) {
    let _java_live_base_url_env_guard = JAVA_LIVE_BASE_URL_ENV_LOCK.lock().await;

    let app =
        komga_rust::app::build_router_with_profile(komga_rust::app::CompatProfile::JavaLiveLocaldb);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/opds/v2/books/book-1/manifest")
                .header("Host", host)
                .header("X-Auth-Token", "dummy-token")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response.headers().get(header::CONTENT_TYPE).unwrap(),
        "application/opds-publication+json"
    );

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json, expected_java_live_manifest(host));
}

pub(super) async fn assert_series_and_books_urls(
    app: axum::Router,
    expected_series_url: &str,
    expected_book_url: &str,
) {
    let series_response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v1/series")
                .header("X-Auth-Token", "dummy-token")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    let series_body = axum::body::to_bytes(series_response.into_body(), usize::MAX)
        .await
        .unwrap();
    let series_json: Value = serde_json::from_slice(&series_body).unwrap();
    assert_eq!(series_json["content"][0]["url"], expected_series_url);

    let books_response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/books")
                .header("X-Auth-Token", "dummy-token")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    let books_body = axum::body::to_bytes(books_response.into_body(), usize::MAX)
        .await
        .unwrap();
    let books_json: Value = serde_json::from_slice(&books_body).unwrap();
    assert_eq!(books_json["content"][0]["url"], expected_book_url);
}
