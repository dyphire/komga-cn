use super::*;

#[tokio::test]
async fn opds_v1_series_route_matches_java_live_localdb_seeded_entry_contract() {
    let app =
        komga_rust::app::build_router_with_profile(komga_rust::app::CompatProfile::JavaLiveLocaldb);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/opds/v1.2/series")
                .header("X-Auth-Token", "dummy-token")
                .header(header::HOST, "komga.local")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response.headers().get(header::CONTENT_TYPE).unwrap(),
        "application/atom+xml"
    );

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let body = String::from_utf8(body.to_vec()).unwrap();

    assert_eq!(
        body,
        concat!(
            "<feed xmlns=\"http://www.w3.org/2005/Atom\">",
            "<id>allSeries</id>",
            "<title>All series</title>",
            "<updated>2026-01-01T00:00:00Z</updated>",
            "<author><name>Komga</name><uri>https://github.com/gotson/komga</uri></author>",
            "<link type=\"application/atom+xml;profile=opds-catalog;kind=navigation\" rel=\"self\" href=\"http://komga.local/opds/v1.2/series\"/>",
            "<link type=\"application/atom+xml;profile=opds-catalog;kind=navigation\" rel=\"start\" href=\"http://komga.local/opds/v1.2/catalog\"/>",
            "<entry>",
            "<title>series</title>",
            "<updated>2026-01-01T00:00:00Z</updated>",
            "<id>series-1</id>",
            "<content></content>",
            "<link type=\"application/atom+xml;profile=opds-catalog;kind=navigation\" rel=\"subsection\" href=\"http://komga.local/opds/v1.2/series/series-1\"/>",
            "</entry>",
            "</feed>"
        )
    );
}

#[tokio::test]
async fn opds_catalog_route_issues_auth_challenge_when_unauthenticated() {
    assert_opds_catalog_challenge_for_host("localhost").await;
    assert_opds_catalog_challenge_for_host("127.0.0.1").await;
}

#[tokio::test]
async fn opds_auth_route_returns_auth_document() {
    assert_opds_auth_for_host("localhost").await;
    assert_opds_auth_for_host("127.0.0.1").await;
}

#[tokio::test]
async fn opds_manifest_route_returns_snapshot_json() {
    let app = komga_rust::app::build_router();

    let response = app
        .oneshot(
            Request::builder()
                .uri("/opds/v2/books/book-1/manifest")
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
    assert_eq!(json, expected_opds_snapshot("opds-v2-manifest.json"));
}

#[tokio::test]
async fn opds_manifest_route_uses_request_host_in_java_live_localdb() {
    assert_java_live_manifest_for_host("localhost").await;
    assert_java_live_manifest_for_host("127.0.0.1").await;
}

#[tokio::test]
async fn opds_manifest_route_uses_java_live_upstream_manifest_when_configured() {
    let _java_live_base_url_env_guard = JAVA_LIVE_BASE_URL_ENV_LOCK.lock().await;

    let upstream_body = serde_json::json!({
        "context": "https://readium.org/webpub-manifest/context.jsonld",
        "images": [],
        "landmarks": [],
        "links": [
            {
                "href": "http://komga.local/opds/v2/books/book-1/manifest",
                "properties": {
                    "authenticate": {
                        "href": "http://komga.local/opds/v2/auth",
                        "type": "application/opds-authentication+json",
                    }
                },
                "rel": "self",
                "type": "application/divina+json",
            }
        ],
        "metadata": {
            "title": "book.cbr",
            "modified": "2026-03-21T09:08:28-04:00",
            "conformsTo": "https://readium.org/webpub-manifest/profiles/divina",
            "numberOfPages": 1,
            "published": "2024-01-01",
        },
        "pageList": [],
        "readingOrder": [
            {
                "href": "http://komga.local/opds/v2/books/book-1/pages/1?contentNegotiation=false",
                "type": "image/png",
                "width": 1,
                "height": 1,
            }
        ],
        "resources": [],
        "toc": [],
    });
    let upstream_response = upstream_body.to_string();
    let listener = tokio::net::TcpListener::bind(std::net::SocketAddr::from(([127, 0, 0, 1], 0)))
        .await
        .unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        for step in 0..2 {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut buffer = [0u8; 4096];
            let size = tokio::io::AsyncReadExt::read(&mut socket, &mut buffer)
                .await
                .unwrap();
            let request = String::from_utf8_lossy(&buffer[..size]);
            let request_lower = request.to_ascii_lowercase();

            let response = if step == 0 {
                assert!(request.contains("GET /api/v2/users/me "));
                assert!(
                    request_lower.contains("authorization: basic ywrtaw5azxhhbxbszs5vcmc6ywrtaw4=")
                );
                concat!(
                    "HTTP/1.1 200 OK\r\n",
                    "Content-Type: application/json\r\n",
                    "Set-Cookie: KOMGA-SESSION=java-admin-session; Path=/; HttpOnly; SameSite=Lax\r\n",
                    "Content-Length: 2\r\n",
                    "Connection: close\r\n",
                    "\r\n",
                    "{}"
                )
                .to_string()
            } else {
                assert!(request.contains("GET /opds/v2/books/book-1/manifest "));
                assert!(request_lower.contains("cookie: komga-session=java-admin-session"));
                format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/opds-publication+json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    upstream_response.len(),
                    upstream_response
                )
            };

            tokio::io::AsyncWriteExt::write_all(&mut socket, response.as_bytes())
                .await
                .unwrap();
        }
    });

    let env_key = "KOMGA_RUST_JAVA_LIVE_BASE_URL";
    let original = std::env::var_os(env_key);
    unsafe {
        std::env::set_var(env_key, format!("http://{address}"));
    }

    let app =
        komga_rust::app::build_router_with_profile(komga_rust::app::CompatProfile::JavaLiveLocaldb);
    let response = app
        .oneshot(
            Request::builder()
                .uri("/opds/v2/books/book-1/manifest")
                .header("X-Auth-Token", "dummy-token")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    if let Some(value) = original {
        unsafe {
            std::env::set_var(env_key, value);
        }
    } else {
        unsafe {
            std::env::remove_var(env_key);
        }
    }

    server.abort();
    let _ = server.await;

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response.headers().get(header::CONTENT_TYPE).unwrap(),
        "application/opds-publication+json"
    );

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json, upstream_body);
}

#[tokio::test]
async fn opds_manifest_route_rewrites_java_live_upstream_urls_to_request_host() {
    let _java_live_base_url_env_guard = JAVA_LIVE_BASE_URL_ENV_LOCK.lock().await;

    let listener = tokio::net::TcpListener::bind(std::net::SocketAddr::from(([127, 0, 0, 1], 0)))
        .await
        .unwrap();
    let address = listener.local_addr().unwrap();
    let upstream_base = format!("http://{address}");
    let upstream_body = serde_json::json!({
        "context": "https://readium.org/webpub-manifest/context.jsonld",
        "images": [],
        "landmarks": [],
        "links": [
            {
                "href": format!("{upstream_base}/opds/v2/books/book-1/manifest"),
                "properties": {
                    "authenticate": {
                        "href": format!("{upstream_base}/opds/v2/auth"),
                        "type": "application/opds-authentication+json",
                    }
                },
                "rel": "self",
                "type": "application/divina+json",
            },
            {
                "href": format!("{upstream_base}/opds/v2/books/book-1/file"),
                "properties": {
                    "authenticate": {
                        "href": format!("{upstream_base}/opds/v2/auth"),
                        "type": "application/opds-authentication+json",
                    }
                },
                "rel": "http://opds-spec.org/acquisition",
                "type": "application/vnd.comicbook+zip",
            },
            {
                "href": format!("{upstream_base}/opds/v2/books/book-1/progression"),
                "properties": {
                    "authenticate": {
                        "href": format!("{upstream_base}/opds/v2/auth"),
                        "type": "application/opds-authentication+json",
                    }
                },
                "rel": "http://www.cantook.com/api/progression",
                "type": "application/vnd.readium.progression+json",
            }
        ],
        "metadata": {
            "title": "book.cbr",
            "modified": "2026-03-21T09:19:18-04:00",
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
                                "href": format!("{upstream_base}/opds/v2/series/series-1"),
                                "type": "application/opds+json",
                            }
                        ],
                    }
                ]
            }
        },
        "pageList": [],
        "readingOrder": [
            {
                "href": format!("{upstream_base}/opds/v2/books/book-1/pages/1?contentNegotiation=false"),
                "type": "image/png",
                "width": 1,
                "height": 1,
            }
        ],
        "resources": [
            {
                "href": format!("{upstream_base}/opds/v2/books/book-1/thumbnail"),
                "properties": {
                    "authenticate": {
                        "href": format!("{upstream_base}/opds/v2/auth"),
                        "type": "application/opds-authentication+json",
                    }
                },
                "type": "image/jpeg",
            }
        ],
        "toc": [],
    });
    let upstream_response = upstream_body.to_string();
    let server = tokio::spawn(async move {
        for step in 0..2 {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut buffer = [0u8; 4096];
            let size = tokio::io::AsyncReadExt::read(&mut socket, &mut buffer)
                .await
                .unwrap();
            let request = String::from_utf8_lossy(&buffer[..size]);
            let request_lower = request.to_ascii_lowercase();

            let response = if step == 0 {
                assert!(request.contains("GET /api/v2/users/me "));
                assert!(
                    request_lower.contains("authorization: basic ywrtaw5azxhhbxbszs5vcmc6ywrtaw4=")
                );
                concat!(
                    "HTTP/1.1 200 OK\r\n",
                    "Content-Type: application/json\r\n",
                    "Set-Cookie: KOMGA-SESSION=java-admin-session; Path=/; HttpOnly; SameSite=Lax\r\n",
                    "Content-Length: 2\r\n",
                    "Connection: close\r\n",
                    "\r\n",
                    "{}"
                )
                .to_string()
            } else {
                assert!(request.contains("GET /opds/v2/books/book-1/manifest "));
                assert!(request_lower.contains("cookie: komga-session=java-admin-session"));
                format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/opds-publication+json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    upstream_response.len(),
                    upstream_response
                )
            };

            tokio::io::AsyncWriteExt::write_all(&mut socket, response.as_bytes())
                .await
                .unwrap();
        }
    });

    let env_key = "KOMGA_RUST_JAVA_LIVE_BASE_URL";
    let original = std::env::var_os(env_key);
    unsafe {
        std::env::set_var(env_key, format!("http://{address}"));
    }

    let app =
        komga_rust::app::build_router_with_profile(komga_rust::app::CompatProfile::JavaLiveLocaldb);
    let response = app
        .oneshot(
            Request::builder()
                .uri("/opds/v2/books/book-1/manifest")
                .header(header::HOST, "komga.local")
                .header("X-Auth-Token", "dummy-token")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    if let Some(value) = original {
        unsafe {
            std::env::set_var(env_key, value);
        }
    } else {
        unsafe {
            std::env::remove_var(env_key);
        }
    }

    server.abort();
    let _ = server.await;

    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(
        json["links"][0]["href"],
        "http://komga.local/opds/v2/books/book-1/manifest"
    );
    assert_eq!(
        json["links"][0]["properties"]["authenticate"]["href"],
        "http://komga.local/opds/v2/auth"
    );
    assert_eq!(
        json["links"][1]["href"],
        "http://komga.local/opds/v2/books/book-1/file"
    );
    assert_eq!(
        json["links"][2]["href"],
        "http://komga.local/opds/v2/books/book-1/progression"
    );
    assert_eq!(
        json["metadata"]["belongsTo"]["series"][0]["links"][0]["href"],
        "http://komga.local/opds/v2/series/series-1"
    );
    assert_eq!(
        json["readingOrder"][0]["href"],
        "http://komga.local/opds/v2/books/book-1/pages/1?contentNegotiation=false"
    );
    assert_eq!(
        json["resources"][0]["href"],
        "http://komga.local/opds/v2/books/book-1/thumbnail"
    );
}

#[tokio::test]
async fn opds_v1_series_route_returns_atom_feed_for_authorized_user() {
    let app = komga_rust::app::build_router();

    let response = app
        .oneshot(
            Request::builder()
                .uri("/opds/v1.2/series")
                .header("X-Auth-Token", "komga-user-token")
                .header(header::HOST, "localhost")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response.headers().get(header::CONTENT_TYPE).unwrap(),
        "application/atom+xml"
    );

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let xml = String::from_utf8(body.to_vec()).unwrap();

    assert!(xml.contains("<id>allSeries</id>"));
    assert!(xml.contains("<title>All series</title>"));
    assert!(xml.contains("href=\"http://localhost/opds/v1.2/series\""));
    assert!(xml.contains("href=\"http://localhost/opds/v1.2/catalog\""));
    assert!(!xml.contains("<?xml"));
    assert!(!xml.contains("<entry>"));
}
