use super::*;

#[tokio::test]
async fn libraries_route_returns_json_when_authorized() {
    let app = komga_rust::app::build_router();

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/libraries")
                .header("X-Auth-Token", "dummy-token")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response.headers().get(header::CONTENT_TYPE).unwrap(),
        "application/json"
    );

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json, expected_snapshot("libraries-list-admin.json"));
}

#[tokio::test]
async fn libraries_route_returns_admin_snapshot_after_admin_login() {
    let app = komga_rust::app::build_router();

    let token = session_token_for_basic_auth(&app, "YWRtaW5AZXhhbXBsZS5vcmc6YWRtaW4=").await;
    let json = libraries_json_for_token(&app, &token).await;

    assert_eq!(json, expected_snapshot("libraries-list-admin.json"));
    assert_eq!(json[0]["root"], "/library1");
}

#[tokio::test]
async fn libraries_route_returns_user_snapshot_with_hidden_root_after_user_login() {
    let app = komga_rust::app::build_router();

    let token = session_token_for_basic_auth(&app, "dXNlckBleGFtcGxlLm9yZzp1c2Vy").await;
    let json = libraries_json_for_token(&app, &token).await;

    assert_eq!(json, expected_snapshot("libraries-list-user.json"));
    assert_eq!(json[0]["root"], "");
}

#[tokio::test]
async fn libraries_route_returns_only_authorized_libraries_for_limited_user() {
    let app = komga_rust::app::build_router();

    let token = session_token_for_basic_auth(&app, "bGltaXRlZEBleGFtcGxlLm9yZzpsaW1pdGVk").await;
    let json = libraries_json_for_token(&app, &token).await;

    assert_eq!(json, expected_snapshot("libraries-list-user.json"));
    assert_eq!(json.as_array().map(Vec::len), Some(1));
    assert_eq!(json[0]["id"], "1");
    assert_eq!(json[0]["root"], "");
}

#[tokio::test]
async fn libraries_route_uses_java_live_localdb_admin_payload_instead_of_snapshot() {
    let _java_live_base_url_env_guard = JAVA_LIVE_BASE_URL_ENV_LOCK.lock().await;

    let upstream_body = serde_json::json!([
        {
            "id": "0PV0WX931SWP1",
            "name": "default",
            "root": "/tmp/komga-live-http-fixture",
            "importComicInfoBook": true,
            "importComicInfoSeries": true,
            "importComicInfoCollection": true,
            "importComicInfoReadList": true,
            "importComicInfoSeriesAppendVolume": true,
            "importEpubBook": true,
            "importEpubSeries": true,
            "importMylarSeries": true,
            "importLocalArtwork": true,
            "importBarcodeIsbn": true,
            "scanForceModifiedTime": false,
            "scanInterval": "EVERY_6H",
            "scanOnStartup": false,
            "scanCbx": true,
            "scanPdf": true,
            "scanEpub": true,
            "scanDirectoryExclusions": [],
            "repairExtensions": false,
            "convertToCbz": false,
            "emptyTrashAfterScan": false,
            "seriesCover": "FIRST",
            "hashFiles": true,
            "hashPages": false,
            "hashKoreader": false,
            "analyzeDimensions": true,
            "oneshotsDirectory": null,
            "unavailable": false
        }
    ]);
    let upstream_response = upstream_body.to_string();
    let listener = tokio::net::TcpListener::bind(std::net::SocketAddr::from(([127, 0, 0, 1], 0)))
        .await
        .unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        for step in 0..2 {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut buffer = [0u8; 2048];
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
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nSet-Cookie: KOMGA-SESSION=java-admin-session; Path=/\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{}".to_string()
            } else {
                assert!(request.contains("GET /api/v1/libraries "));
                assert!(request_lower.contains("cookie: komga-session=java-admin-session"));
                format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
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
    let token = session_token_for_basic_auth(&app, "YWRtaW5AZXhhbXBsZS5vcmc6YWRtaW4=").await;
    let json = libraries_json_for_token(&app, &token).await;

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

    assert_eq!(json, upstream_body);
    assert_eq!(json[0]["root"], "/tmp/komga-live-http-fixture");
    assert_ne!(json[0]["id"], "1");
}

#[tokio::test]
async fn libraries_route_returns_server_error_when_java_live_localdb_admin_fetch_fails() {
    let _java_live_base_url_env_guard = JAVA_LIVE_BASE_URL_ENV_LOCK.lock().await;

    let listener = tokio::net::TcpListener::bind(std::net::SocketAddr::from(([127, 0, 0, 1], 0)))
        .await
        .unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        let mut buffer = [0u8; 2048];
        let size = tokio::io::AsyncReadExt::read(&mut socket, &mut buffer)
            .await
            .unwrap();
        let request = String::from_utf8_lossy(&buffer[..size]);

        assert!(request.contains("GET /api/v2/users/me "));

        let response = concat!(
            "HTTP/1.1 500 Internal Server Error\r\n",
            "Content-Type: application/json\r\n",
            "Content-Length: 18\r\n",
            "Connection: close\r\n",
            "\r\n",
            "{\"status\":500}"
        );
        tokio::io::AsyncWriteExt::write_all(&mut socket, response.as_bytes())
            .await
            .unwrap();
    });

    let env_key = "KOMGA_RUST_JAVA_LIVE_BASE_URL";
    let original = std::env::var_os(env_key);
    unsafe {
        std::env::set_var(env_key, format!("http://{address}"));
    }

    let app =
        komga_rust::app::build_router_with_profile(komga_rust::app::CompatProfile::JavaLiveLocaldb);
    let token = session_token_for_basic_auth(&app, "YWRtaW5AZXhhbXBsZS5vcmc6YWRtaW4=").await;
    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/libraries")
                .header("X-Auth-Token", token)
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

    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
}

#[tokio::test]
async fn libraries_route_accepts_java_live_localdb_admin_token_bootstrap_without_cookie() {
    let _java_live_base_url_env_guard = JAVA_LIVE_BASE_URL_ENV_LOCK.lock().await;

    let upstream_body = serde_json::json!([]);
    let upstream_response = upstream_body.to_string();
    let listener = tokio::net::TcpListener::bind(std::net::SocketAddr::from(([127, 0, 0, 1], 0)))
        .await
        .unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        for step in 0..2 {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut buffer = [0u8; 2048];
            let size = tokio::io::AsyncReadExt::read(&mut socket, &mut buffer)
                .await
                .unwrap();
            let request = String::from_utf8_lossy(&buffer[..size]);
            let request_lower = request.to_ascii_lowercase();

            let response = if step == 0 {
                assert!(request.contains("GET /api/v2/users/me "));
                assert!(request_lower.contains("authorization: basic "));
                concat!(
                    "HTTP/1.1 200 OK\r\n",
                    "Content-Type: application/json\r\n",
                    "X-Auth-Token: java-admin-token\r\n",
                    "Content-Length: 2\r\n",
                    "Connection: close\r\n",
                    "\r\n",
                    "{}"
                )
                .to_string()
            } else {
                assert!(request.contains("GET /api/v1/libraries "));
                assert!(request_lower.contains("x-auth-token: java-admin-token"));
                format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
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
    let token = session_token_for_basic_auth(&app, "YWRtaW5AZXhhbXBsZS5vcmc6YWRtaW4=").await;
    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/libraries")
                .header("X-Auth-Token", token)
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
    assert_eq!(json, upstream_body);
}

#[tokio::test]
async fn libraries_route_uses_java_live_localdb_user_payload_instead_of_snapshot() {
    let _java_live_base_url_env_guard = JAVA_LIVE_BASE_URL_ENV_LOCK.lock().await;

    let upstream_body = serde_json::json!([]);
    let upstream_response = upstream_body.to_string();
    let listener = tokio::net::TcpListener::bind(std::net::SocketAddr::from(([127, 0, 0, 1], 0)))
        .await
        .unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        for step in 0..2 {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut buffer = [0u8; 2048];
            let size = tokio::io::AsyncReadExt::read(&mut socket, &mut buffer)
                .await
                .unwrap();
            let request = String::from_utf8_lossy(&buffer[..size]);
            let request_lower = request.to_ascii_lowercase();

            let response = if step == 0 {
                assert!(request.contains("GET /api/v2/users/me "));
                assert!(
                    request_lower.contains("authorization: basic dxnlckblegftcgxllm9yzzp1c2vy"),
                    "unexpected bootstrap request: {request_lower}"
                );
                concat!(
                    "HTTP/1.1 200 OK\r\n",
                    "Content-Type: application/json\r\n",
                    "Set-Cookie: KOMGA-SESSION=java-user-session; Path=/; HttpOnly; SameSite=Lax\r\n",
                    "Content-Length: 2\r\n",
                    "Connection: close\r\n",
                    "\r\n",
                    "{}"
                )
                .to_string()
            } else {
                assert!(request.contains("GET /api/v1/libraries "));
                assert!(request_lower.contains("cookie: komga-session=java-user-session"));
                format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
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
    let token = session_token_for_basic_auth(&app, "dXNlckBleGFtcGxlLm9yZzp1c2Vy").await;
    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/libraries")
                .header("X-Auth-Token", token)
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
    assert_eq!(json, upstream_body);
}
