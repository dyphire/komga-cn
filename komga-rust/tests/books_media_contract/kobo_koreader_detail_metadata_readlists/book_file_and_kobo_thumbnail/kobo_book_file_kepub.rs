use super::*;

#[tokio::test]
async fn router_kobo_book_file_epub_convert_kepub_uses_kepub_attachment_name() {
    let paths = new_router_fixture("router-kobo-book-file-convert-kepub").await;
    seed_router_contract_data(&paths).await;
    write_router_epub_with_cover(&paths, "books/book-1.epub");

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/kobo/any-token/v1/books/book-1/file/epub?convert_kepub=true")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("convert kepub kobo file request should build"),
        )
        .await
        .expect("convert kepub kobo file request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response
            .headers()
            .get(header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok()),
        Some("application/epub+zip")
    );
    let content_disposition = response
        .headers()
        .get(header::CONTENT_DISPOSITION)
        .and_then(|value| value.to_str().ok())
        .expect("convert kepub response should include content disposition");
    assert!(content_disposition.contains("book-1.kepub.epub"));
    let body = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("convert kepub response body should be readable");
    assert!(!body.is_empty());
    assert_eq!(&body.as_ref()[..2], b"PK");

    cleanup_router_fixture(paths);
}
