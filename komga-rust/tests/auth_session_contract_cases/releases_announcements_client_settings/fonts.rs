use super::*;

fn expected_open_dyslexic_css() -> &'static str {
    "@font-face {\n    font-family: 'OpenDyslexic';\n    src: url('OpenDyslexic-Bold-Italic.woff') format('woff'),url('OpenDyslexic-Bold-Italic.woff2') format('woff2');\n    font-weight: bold;\n    font-style: italic;\n}\n\n@font-face {\n    font-family: 'OpenDyslexic';\n    src: url('OpenDyslexic-Bold.woff') format('woff'),url('OpenDyslexic-Bold.woff2') format('woff2');\n    font-weight: bold;\n    font-style: normal;\n}\n\n@font-face {\n    font-family: 'OpenDyslexic';\n    src: url('OpenDyslexic-Italic.woff') format('woff'),url('OpenDyslexic-Italic.woff2') format('woff2');\n    font-weight: normal;\n    font-style: italic;\n}\n\n@font-face {\n    font-family: 'OpenDyslexic';\n    src: url('OpenDyslexic-Regular.woff') format('woff'),url('OpenDyslexic-Regular.woff2') format('woff2');\n    font-weight: normal;\n    font-style: normal;\n}\n"
}

#[tokio::test]
async fn router_get_font_file_downloads_embedded_font_without_auth_like_kotlin() {
    let paths = new_router_fixture("router-get-font-file-embedded-anonymous").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths)).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/fonts/resource/OpenDyslexic/OpenDyslexic-Bold.woff")
                .body(Body::empty())
                .expect("get embedded font file request should build"),
        )
        .await
        .expect("get embedded font file request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response.headers().get(header::CONTENT_TYPE),
        Some(&header::HeaderValue::from_static("font/woff"))
    );
    assert_eq!(
        response.headers().get(header::CONTENT_DISPOSITION),
        Some(&header::HeaderValue::from_static(
            "attachment; filename=\"OpenDyslexic-Bold.woff\"",
        ))
    );

    let bytes = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("embedded font file response body should read");
    assert!(!bytes.is_empty());

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_get_font_family_css_downloads_embedded_css_without_auth_like_kotlin() {
    let paths = new_router_fixture("router-get-font-css-embedded-anonymous").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths)).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/fonts/resource/OpenDyslexic/css")
                .body(Body::empty())
                .expect("get embedded font css request should build"),
        )
        .await
        .expect("get embedded font css request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response.headers().get(header::CONTENT_TYPE),
        Some(&header::HeaderValue::from_static("text/css"))
    );
    assert_eq!(
        response.headers().get(header::CONTENT_DISPOSITION),
        Some(&header::HeaderValue::from_static(
            "attachment; filename=\"OpenDyslexic.css\"",
        ))
    );

    let bytes = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("embedded font css response body should read");
    let css = String::from_utf8(bytes.to_vec()).expect("embedded font css should be utf-8");
    assert_eq!(css, expected_open_dyslexic_css());

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_get_font_family_css_downloads_filesystem_css_without_auth_like_kotlin() {
    let paths = new_router_fixture("router-get-font-css-filesystem-anonymous").await;
    seed_router_contract_data(&paths).await;

    let family_dir = paths.config_dir.join("fonts").join("Custom Family");
    std::fs::create_dir_all(&family_dir).expect("custom css family dir should be created");
    std::fs::write(family_dir.join("Custom-BoldItalic.woff"), b"font-bytes")
        .expect("custom bold italic woff should be written");
    std::fs::write(family_dir.join("Custom-BoldItalic.woff2"), b"font-bytes")
        .expect("custom bold italic woff2 should be written");
    std::fs::write(family_dir.join("Custom-Regular.ttf"), b"font-bytes")
        .expect("custom regular ttf should be written");

    let app = build_router_with_config(&runtime_config_for_paths(&paths)).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/fonts/resource/Custom%20Family/css")
                .body(Body::empty())
                .expect("get filesystem font css request should build"),
        )
        .await
        .expect("get filesystem font css request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response.headers().get(header::CONTENT_TYPE),
        Some(&header::HeaderValue::from_static("text/css"))
    );
    assert_eq!(
        response.headers().get(header::CONTENT_DISPOSITION),
        Some(&header::HeaderValue::from_static(
            "attachment; filename=\"Custom Family.css\"",
        ))
    );

    let bytes = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("filesystem font css response body should read");
    let css = String::from_utf8(bytes.to_vec()).expect("filesystem font css should be utf-8");
    assert_eq!(
        css,
        "@font-face {\n    font-family: 'Custom Family';\n    src: url('Custom-BoldItalic.woff') format('woff'),url('Custom-BoldItalic.woff2') format('woff2');\n    font-weight: bold;\n    font-style: italic;\n}\n\n@font-face {\n    font-family: 'Custom Family';\n    src: url('Custom-Regular.ttf') format('truetype');\n    font-weight: normal;\n    font-style: normal;\n}\n"
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_get_font_file_downloads_filesystem_font_without_auth_like_kotlin() {
    let paths = new_router_fixture("router-get-font-file-filesystem-anonymous").await;
    seed_router_contract_data(&paths).await;

    let family_dir = paths.config_dir.join("fonts").join("Custom Family");
    std::fs::create_dir_all(&family_dir).expect("custom font family dir should be created");
    std::fs::write(family_dir.join("Custom-Regular.ttf"), b"font-bytes")
        .expect("custom font file should be written");

    let app = build_router_with_config(&runtime_config_for_paths(&paths)).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/fonts/resource/Custom%20Family/Custom-Regular.ttf")
                .body(Body::empty())
                .expect("get filesystem font file request should build"),
        )
        .await
        .expect("get filesystem font file request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response.headers().get(header::CONTENT_TYPE),
        Some(&header::HeaderValue::from_static("font/ttf"))
    );
    assert_eq!(
        response.headers().get(header::CONTENT_DISPOSITION),
        Some(&header::HeaderValue::from_static(
            "attachment; filename=\"Custom-Regular.ttf\"",
        ))
    );

    let bytes = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("filesystem font file response body should read");
    assert_eq!(bytes.as_ref(), b"font-bytes");

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_get_fonts_families_requires_auth_like_kotlin() {
    let paths = new_router_fixture("router-get-fonts-families-requires-auth").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths)).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/fonts/families")
                .body(Body::empty())
                .expect("get fonts families anonymous request should build"),
        )
        .await
        .expect("get fonts families anonymous request should complete");

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_get_fonts_families_returns_embedded_and_filesystem_families_like_kotlin() {
    let paths = new_router_fixture("router-get-fonts-families-embedded-and-filesystem").await;
    seed_router_contract_data(&paths).await;
    seed_router_library_restricted_user(
        &paths,
        "fonts-user",
        "fonts@example.org",
        "router-contract-fonts-123",
        &["library-1"],
    )
    .await;

    let family_dir = paths.config_dir.join("fonts").join("Custom Family");
    std::fs::create_dir_all(&family_dir).expect("custom font family dir should be created");
    std::fs::write(family_dir.join("Custom-Regular.ttf"), b"font-bytes")
        .expect("custom font file should be written");

    let app = build_router_with_config(&runtime_config_for_paths(&paths)).await;
    let auth_token = login_with_basic_credentials_and_get_token(
        app.clone(),
        "fonts@example.org",
        "router-contract-fonts-123",
    )
    .await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/fonts/families")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("get fonts families authenticated request should build"),
        )
        .await
        .expect("get fonts families authenticated request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let payload = response_json(response).await;
    let families = payload
        .as_array()
        .expect("fonts families payload should be an array")
        .iter()
        .map(|value| {
            value
                .as_str()
                .expect("font family entry should be a string")
                .to_string()
        })
        .collect::<Vec<_>>();
    assert!(families.contains(&"OpenDyslexic".to_string()));
    assert!(families.contains(&"Custom Family".to_string()));

    cleanup_router_fixture(paths);
}
