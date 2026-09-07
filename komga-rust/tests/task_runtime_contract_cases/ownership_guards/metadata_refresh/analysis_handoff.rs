use super::*;
use komga_application::task_processing::{BookPayload, TaskKind, TaskRequest};
use komga_domain::discovery::MediaStatus;
use komga_infrastructure_media_core::content::metadata_sources::CapturedMetadataDocument;
use komga_infrastructure_media_library::MediaLibraryJobContext;
use komga_infrastructure_media_library::analysis::{BookAnalysisPurpose, analyze_book};
use komga_infrastructure_media_metadata::{
    load_comicinfo_bytes_from_path, refresh_book_metadata_with_sources,
};
use std::collections::BTreeSet;

async fn analysis_context(paths: &RuntimeDbPaths) -> (MediaLibraryJobContext, RiirDatabase) {
    let riir = RiirDatabase::file_backed(&paths.riir_db_file)
        .await
        .unwrap();
    let context = MediaLibraryJobContext::new(
        DatabaseHandle::file_backed(paths.main_db.clone())
            .await
            .unwrap(),
        true,
        true,
        Arc::new(RuntimeSseEventStore::default()),
        Some(Arc::new(
            komga_infrastructure_media_metadata::RiirSeriesMetadataContributionCleanup::new(
                riir.clone(),
            ),
        )),
    );
    (context, riir)
}

async fn configure_analysis(pool: &sqlx::SqlitePool, relative_path: &str) {
    isolate_book_metadata_imports(pool, 1, 1, 1, 0).await;
    sqlx::query("UPDATE BOOK SET URL = ?, NAME = ?, LAST_MODIFIED_DATE = '2020-01-01 00:00:00' WHERE ID = 'book-1'")
        .bind(relative_path).bind(relative_path.rsplit('/').next().unwrap())
        .execute(pool).await.unwrap();
    sqlx::query("UPDATE LIBRARY SET ANALYZE_DIMENSIONS = 0 WHERE ID = 'library-1'")
        .execute(pool)
        .await
        .unwrap();
    sqlx::query("UPDATE BOOK_METADATA SET TITLE = 'original' WHERE BOOK_ID = 'book-1'")
        .execute(pool)
        .await
        .unwrap();
}

#[tokio::test]
async fn captured_sources_match_standalone_readers_and_refresh_without_files() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    for fixture in [
        "sample/ComicInfo.zip",
        "sample/ComicInfo_duplicateInfos.rar",
        "../komga/src/test/resources/archives/rar4.rar",
        "../komga/src/test/resources/archives/rar5.rar",
        "../komga/src/test/resources/archives/epub3.epub",
        "sample/epub3.mobi",
    ] {
        let ctx = TestFixture::builder("analysis-source-formats")
            .without_runtime_workers()
            .build()
            .await;
        let name = std::path::Path::new(fixture)
            .file_name()
            .unwrap()
            .to_str()
            .unwrap();
        let relative = format!("books/{name}");
        let path = ctx.paths().config_dir.join(&relative);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::copy(root.join(fixture), &path).unwrap();
        let pool = connect_test_pool(&ctx.paths().main_db, 1).await.unwrap();
        configure_analysis(&pool, &relative).await;
        sqlx::query("UPDATE LIBRARY SET IMPORT_COMICINFO_SERIES = 1, IMPORT_EPUB_SERIES = 1 WHERE ID = 'library-1'")
            .execute(&pool).await.unwrap();
        let (context, riir) = analysis_context(ctx.paths()).await;
        let outcome = analyze_book(&context, "book-1", BookAnalysisPurpose::AnalysisAndMetadata)
            .await
            .unwrap();
        let expected_status = if fixture.starts_with("sample/ComicInfo") {
            MediaStatus::Error // These provider fixtures contain XML but no pages.
        } else {
            MediaStatus::Ready
        };
        assert_eq!(outcome.media_status, Some(expected_status), "{fixture}");
        let media_type: String =
            sqlx::query_scalar("SELECT MEDIA_TYPE FROM MEDIA WHERE BOOK_ID = 'book-1'")
                .fetch_one(&pool)
                .await
                .unwrap();
        let standalone = load_comicinfo_bytes_from_path(&path, &media_type).unwrap();
        match &outcome.metadata_sources.comicinfo {
            CapturedMetadataDocument::Present(bytes) => {
                assert_eq!(Some(bytes), standalone.as_ref(), "{fixture}")
            }
            CapturedMetadataDocument::Absent | CapturedMetadataDocument::NotApplicable => {
                assert!(standalone.is_none(), "{fixture}")
            }
            state => panic!("unexpected source for {fixture}: {state:?}"),
        }
        // Compare the observable imports from the independent and supplied entry points.
        sqlx::query("DELETE FROM READLIST_BOOK WHERE BOOK_ID = 'book-1'")
            .execute(&pool)
            .await
            .unwrap();
        let capabilities =
            komga_application::task_processing::RefreshBookMetadataPayload::default_capabilities()
                .into_iter()
                .collect::<BTreeSet<_>>();
        komga_infrastructure_media_metadata::refresh_book_metadata(
            &pool,
            Some(&riir),
            context.runtime_events(),
            "book-1",
            &capabilities,
        )
        .await
        .unwrap();
        let expected: String =
            sqlx::query_scalar("SELECT TITLE FROM BOOK_METADATA WHERE BOOK_ID = 'book-1'")
                .fetch_one(&pool)
                .await
                .unwrap();
        let readlist_count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM READLIST_BOOK WHERE BOOK_ID = 'book-1'")
                .fetch_one(&pool)
                .await
                .unwrap();
        let contribution_sql = "SELECT PROVIDER, OUTCOME, PAYLOAD, SOURCE_MEDIA_MODIFIED_SECONDS FROM SERIES_METADATA_CONTRIBUTION WHERE BOOK_ID = 'book-1' ORDER BY PROVIDER";
        let expected_contributions: Vec<(String, String, Option<String>, i64)> =
            sqlx::query_as(contribution_sql)
                .fetch_all(riir.read_pool())
                .await
                .unwrap();
        sqlx::query("UPDATE BOOK_METADATA SET TITLE = 'original' WHERE BOOK_ID = 'book-1'")
            .execute(&pool)
            .await
            .unwrap();
        // Require the handoff to create these results, not merely preserve the baseline.
        sqlx::query("DELETE FROM READLIST_BOOK WHERE BOOK_ID = 'book-1'")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("DELETE FROM SERIES_METADATA_CONTRIBUTION WHERE BOOK_ID = 'book-1'")
            .execute(riir.write_pool())
            .await
            .unwrap();
        fs::remove_file(&path).unwrap();
        for _ in 0..2 {
            refresh_book_metadata_with_sources(
                &pool,
                Some(&riir),
                context.runtime_events(),
                "book-1",
                &capabilities,
                &outcome.metadata_sources,
            )
            .await
            .unwrap();
        }
        let actual: String =
            sqlx::query_scalar("SELECT TITLE FROM BOOK_METADATA WHERE BOOK_ID = 'book-1'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(actual, expected, "{fixture}");
        let actual_count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM READLIST_BOOK WHERE BOOK_ID = 'book-1'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(actual_count, readlist_count, "{fixture}");
        let actual_contributions: Vec<(String, String, Option<String>, i64)> =
            sqlx::query_as(contribution_sql)
                .fetch_all(riir.read_pool())
                .await
                .unwrap();
        assert_eq!(actual_contributions, expected_contributions, "{fixture}");
        if expected_status == MediaStatus::Ready {
            assert!(!actual_contributions.is_empty(), "{fixture}");
        }
        pool.close().await;
    }
}

#[tokio::test]
async fn source_setting_changes_are_checked_before_provider_writes() {
    let ctx = TestFixture::builder("analysis-source-settings")
        .without_runtime_workers()
        .build()
        .await;
    write_router_epub_with_comicinfo(
        ctx.paths(),
        "books/book-1.epub",
        b"<ComicInfo><Title>captured</Title></ComicInfo>",
    );
    let pool = connect_test_pool(&ctx.paths().main_db, 1).await.unwrap();
    configure_analysis(&pool, "books/book-1.epub").await;
    isolate_book_metadata_imports(&pool, 1, 0, 0, 0).await;
    let (context, riir) = analysis_context(ctx.paths()).await;
    let mut outcome = analyze_book(&context, "book-1", BookAnalysisPurpose::AnalysisAndMetadata)
        .await
        .unwrap();
    fs::remove_file(ctx.paths().config_dir.join("books/book-1.epub")).unwrap();
    isolate_book_metadata_imports(&pool, 1, 0, 1, 0).await;
    let capabilities =
        komga_application::task_processing::RefreshBookMetadataPayload::default_capabilities()
            .into_iter()
            .collect::<BTreeSet<_>>();
    let error = refresh_book_metadata_with_sources(
        &pool,
        Some(&riir),
        context.runtime_events(),
        "book-1",
        &capabilities,
        &outcome.metadata_sources,
    )
    .await
    .unwrap_err();
    assert!(format!("{error:#}").contains("not captured"));
    let title: String =
        sqlx::query_scalar("SELECT TITLE FROM BOOK_METADATA WHERE BOOK_ID = 'book-1'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(
        title, "original",
        "missing OPF must not partially apply ComicInfo"
    );
    isolate_book_metadata_imports(&pool, 1, 0, 0, 0).await;
    refresh_book_metadata_with_sources(
        &pool,
        Some(&riir),
        context.runtime_events(),
        "book-1",
        &capabilities,
        &outcome.metadata_sources,
    )
    .await
    .unwrap();
    let title: String =
        sqlx::query_scalar("SELECT TITLE FROM BOOK_METADATA WHERE BOOK_ID = 'book-1'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(title, "captured");
    outcome.metadata_sources.comicinfo = CapturedMetadataDocument::Failed("read failed".into());
    isolate_book_metadata_imports(&pool, 0, 0, 0, 0).await;
    refresh_book_metadata_with_sources(
        &pool,
        Some(&riir),
        context.runtime_events(),
        "book-1",
        &capabilities,
        &outcome.metadata_sources,
    )
    .await
    .unwrap();
    pool.close().await;
}

#[test]
fn analysis_handoff_routes_success_fallback_and_unowned_refresh() {
    for (malformed, owns_sidecar, priority) in [
        (false, true, i32::MIN),
        (false, true, i32::MAX),
        (true, true, 90),
        (false, false, 90),
    ] {
        let executor = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let ctx = executor.block_on(
            TestFixture::builder("analysis-handoff-routing")
                .without_runtime_workers()
                .build(),
        );
        let config = ctx.config().clone();
        let (logs, failed) = capture_router_logs_async_result(&config, async move {
            write_router_epub_with_comicinfo(
                ctx.paths(),
                "books/book-1.epub",
                if malformed {
                    b"<ComicInfo>"
                } else {
                    b"<ComicInfo><Title>captured</Title></ComicInfo>"
                },
            );
            let pool = connect_test_pool(&ctx.paths().main_db, 1).await.unwrap();
            configure_analysis(&pool, "books/book-1.epub").await;
            isolate_book_metadata_imports(&pool, 1, 0, 0, 0).await;
            let runtime = runtime_task_context_with_ownership(
                ctx.paths(),
                TaskRuntimeOwnership {
                    owns_sidecar_output: owns_sidecar,
                    ..TaskRuntimeOwnership::all_owned()
                },
            )
            .await;
            let scheduler =
                TaskQueueScheduler::for_runtime(runtime.clone(), "analysis-handoff").await;
            scheduler
                .enqueue(
                    TaskRequest::with_payload(TaskKind::AnalyzeBook, BookPayload::new("book-1"))
                        .priority(priority)
                        .group("series-1")
                        .into_queue_record(),
                )
                .await
                .unwrap();
            let failed = komga_infrastructure_jobs::process_available(&scheduler, &runtime)
                .await
                .is_err();
            let status: String =
                sqlx::query_scalar("SELECT STATUS FROM MEDIA WHERE BOOK_ID = 'book-1'")
                    .fetch_one(&pool)
                    .await
                    .unwrap();
            assert_eq!(status, "READY");
            let modified: String =
                sqlx::query_scalar("SELECT LAST_MODIFIED_DATE FROM BOOK WHERE ID = 'book-1'")
                    .fetch_one(&pool)
                    .await
                    .unwrap();
            assert_eq!(modified, "2020-01-01 00:00:00");
            let title: String =
                sqlx::query_scalar("SELECT TITLE FROM BOOK_METADATA WHERE BOOK_ID = 'book-1'")
                    .fetch_one(&pool)
                    .await
                    .unwrap();
            assert_eq!(
                title,
                if !malformed && owns_sidecar {
                    "captured"
                } else {
                    "original"
                }
            );
            pool.close().await;
            failed
        });
        assert_eq!(failed, malformed);
        let events = parse_json_log_lines(&logs);
        let enqueued = matching_event_fields(&events, "task_enqueue");
        let count = |kind: &str| {
            enqueued
                .iter()
                .filter(|fields| field_str(fields, "task_type") == Some(kind))
                .count()
        };
        assert_eq!(
            count("RefreshBookMetadata"),
            usize::from(malformed || !owns_sidecar),
            "{logs}"
        );
        assert_eq!(
            count("RefreshSeriesMetadata"),
            usize::from(!malformed && owns_sidecar),
            "{logs}"
        );
        assert_eq!(count("GenerateBookThumbnail"), 1, "{logs}");
        for fields in enqueued {
            match field_str(fields, "task_type") {
                Some("RefreshBookMetadata") => {
                    assert_eq!(field_str(fields, "group"), Some("series-1"));
                    assert_eq!(
                        fields["priority"].as_i64(),
                        Some(i64::from(priority.saturating_add(1)))
                    );
                }
                Some("RefreshSeriesMetadata") => {
                    assert_eq!(field_str(fields, "group"), Some("series-1"));
                    assert_eq!(
                        fields["priority"].as_i64(),
                        Some(i64::from(priority.saturating_add(1).saturating_sub(1)))
                    );
                }
                _ => {}
            }
        }
    }
}
