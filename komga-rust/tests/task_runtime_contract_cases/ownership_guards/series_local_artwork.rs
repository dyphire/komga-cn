use super::*;

async fn enqueue_refresh_series_local_artwork(scheduler: &mut TaskQueueScheduler, series_id: &str) {
    scheduler
        .enqueue(
            TaskQueueRecord::new(
                format!("RefreshSeriesLocalArtwork_{series_id}"),
                1_000,
                Some(series_id.to_string()),
            )
            .with_simple_type("RefreshSeriesLocalArtwork"),
        )
        .await;
}

#[tokio::test]
async fn runtime_skips_series_local_artwork_refresh_when_library_import_local_artwork_is_disabled()
{
    let ctx = TestFixture::new("runtime-skip-series-local-artwork-when-import-disabled").await;

    let series_dir = ctx.paths().config_dir.join("series/series-1");
    std::fs::create_dir_all(&series_dir).expect("series artwork directory should exist");
    std::fs::write(series_dir.join("cover.png"), fixture_png_bytes())
        .expect("series artwork fixture should be written");

    let pool = connect_test_pool(ctx.paths().main_db.as_path(), 1)
        .await
        .expect("main db should open for series local artwork disabled fixture setup");
    sqlx::query("UPDATE LIBRARY SET IMPORT_LOCAL_ARTWORK = 0 WHERE ID = ?")
        .bind("library-1")
        .execute(&pool)
        .await
        .expect("library import-local-artwork flag should be disabled");
    sqlx::query("DELETE FROM THUMBNAIL_SERIES WHERE SERIES_ID = ?")
        .bind("series-1")
        .execute(&pool)
        .await
        .expect("existing series thumbnails should be cleared before local artwork gating test");
    pool.close().await;

    let runtime = runtime_task_context(ctx.paths()).await;
    let mut scheduler = TaskQueueScheduler::for_runtime(runtime.clone(), "rust-main").await;
    enqueue_refresh_series_local_artwork(&mut scheduler, "series-1").await;
    scheduler.process_available(&runtime.job()).await.expect(
        "series local artwork refresh should skip cleanly when library.importLocalArtwork is disabled",
    );

    let verify_pool = connect_test_pool(ctx.paths().main_db.as_path(), 1)
        .await
        .expect("main db should open for series local artwork disabled verification");
    let sidecar_thumbnail_count = sqlx::query(
        "SELECT COUNT(*) AS COUNT FROM THUMBNAIL_SERIES WHERE SERIES_ID = ? AND TYPE = 'SIDECAR'",
    )
    .bind("series-1")
    .fetch_one(&verify_pool)
    .await
    .expect("series sidecar thumbnail rows should be queryable")
    .get::<i64, _>("COUNT");
    verify_pool.close().await;

    assert_eq!(
        sidecar_thumbnail_count, 0,
        "runtime must not import series local artwork when library.importLocalArtwork is disabled",
    );
}

#[tokio::test]
async fn runtime_skips_series_local_artwork_refresh_for_oneshot_series() {
    let ctx = TestFixture::new("runtime-skip-series-local-artwork-for-oneshot").await;

    let series_dir = ctx.paths().config_dir.join("series/series-1");
    std::fs::create_dir_all(&series_dir).expect("series artwork directory should exist");
    std::fs::write(series_dir.join("cover.png"), fixture_png_bytes())
        .expect("series artwork fixture should be written");

    let pool = connect_test_pool(ctx.paths().main_db.as_path(), 1)
        .await
        .expect("main db should open for series oneshot artwork fixture setup");
    sqlx::query("UPDATE LIBRARY SET IMPORT_LOCAL_ARTWORK = 1 WHERE ID = ?")
        .bind("library-1")
        .execute(&pool)
        .await
        .expect("library import-local-artwork flag should be enabled");
    sqlx::query("UPDATE SERIES SET ONESHOT = 1 WHERE ID = ?")
        .bind("series-1")
        .execute(&pool)
        .await
        .expect("series oneshot flag should be enabled");
    sqlx::query("DELETE FROM THUMBNAIL_SERIES WHERE SERIES_ID = ?")
        .bind("series-1")
        .execute(&pool)
        .await
        .expect("existing series thumbnails should be cleared before oneshot skip test");
    pool.close().await;

    let runtime = runtime_task_context(ctx.paths()).await;
    let mut scheduler = TaskQueueScheduler::for_runtime(runtime.clone(), "rust-main").await;
    enqueue_refresh_series_local_artwork(&mut scheduler, "series-1").await;
    scheduler
        .process_available(&runtime.job())
        .await
        .expect("series local artwork refresh should skip oneshot series cleanly");

    let verify_pool = connect_test_pool(ctx.paths().main_db.as_path(), 1)
        .await
        .expect("main db should open for oneshot series local artwork verification");
    let sidecar_thumbnail_count = sqlx::query(
        "SELECT COUNT(*) AS COUNT FROM THUMBNAIL_SERIES WHERE SERIES_ID = ? AND TYPE = 'SIDECAR'",
    )
    .bind("series-1")
    .fetch_one(&verify_pool)
    .await
    .expect("series sidecar thumbnail rows should be queryable after oneshot skip")
    .get::<i64, _>("COUNT");
    verify_pool.close().await;

    assert_eq!(
        sidecar_thumbnail_count, 0,
        "runtime must not import series local artwork for oneshot series",
    );
}

#[tokio::test]
async fn runtime_imports_multiple_filesystem_series_local_artworks_and_selects_only_one_when_none_exists()
 {
    let ctx =
        TestFixture::new("runtime-imports-multiple-filesystem-series-local-artworks-none-selected")
            .await;

    let series_dir = ctx.paths().config_dir.join("series/series-1");
    std::fs::create_dir_all(&series_dir).expect("series artwork directory should exist");
    std::fs::write(series_dir.join("cover.png"), fixture_png_bytes())
        .expect("primary series local artwork should be written");
    std::fs::write(series_dir.join("poster.jpg"), fixture_png_bytes())
        .expect("secondary series local artwork should be written");
    std::fs::write(series_dir.join("banner.png"), fixture_png_bytes())
        .expect("non-matching series local artwork should be written");

    let pool = connect_test_pool(ctx.paths().main_db.as_path(), 1)
        .await
        .expect("main db should open for multi-series-artwork fixture setup");
    sqlx::query("UPDATE LIBRARY SET IMPORT_LOCAL_ARTWORK = 1 WHERE ID = ?")
        .bind("library-1")
        .execute(&pool)
        .await
        .expect("library import-local-artwork flag should be enabled");
    sqlx::query("UPDATE SERIES SET ONESHOT = 0 WHERE ID = ?")
        .bind("series-1")
        .execute(&pool)
        .await
        .expect("series oneshot flag should be disabled");
    sqlx::query("DELETE FROM THUMBNAIL_SERIES WHERE SERIES_ID = ?")
        .bind("series-1")
        .execute(&pool)
        .await
        .expect("existing series thumbnails should be cleared for multi-artwork import test");
    pool.close().await;

    let runtime = runtime_task_context(ctx.paths()).await;
    let mut scheduler = TaskQueueScheduler::for_runtime(runtime.clone(), "rust-main").await;
    enqueue_refresh_series_local_artwork(&mut scheduler, "series-1").await;
    scheduler.process_available(&runtime.job()).await.expect(
        "series local artwork refresh should import multiple filesystem candidates cleanly",
    );

    let verify_pool = connect_test_pool(ctx.paths().main_db.as_path(), 1)
        .await
        .expect("main db should open for multi-series-artwork verification");
    let rows = sqlx::query(
        "SELECT TYPE, URL, SELECTED FROM THUMBNAIL_SERIES WHERE SERIES_ID = ? ORDER BY URL ASC",
    )
    .bind("series-1")
    .fetch_all(&verify_pool)
    .await
    .expect("series local artwork rows should be queryable after filesystem import");
    verify_pool.close().await;

    let urls = rows
        .iter()
        .map(|row| row.get::<Option<String>, _>("URL"))
        .collect::<Vec<_>>();
    let selected_count = rows
        .iter()
        .filter(|row| row.get::<bool, _>("SELECTED"))
        .count();

    assert_eq!(
        rows.len(),
        2,
        "runtime should import every matching series local artwork file"
    );
    assert_eq!(
        urls,
        vec![
            Some("series/series-1/cover.png".to_string()),
            Some("series/series-1/poster.jpg".to_string()),
        ],
        "runtime should only import Kotlin-supported series local artwork basenames",
    );
    assert!(
        rows.iter()
            .all(|row| row.get::<String, _>("TYPE") == "SIDECAR"),
        "runtime should import filesystem series local artwork files as SIDECAR thumbnails",
    );
    assert_eq!(
        selected_count, 1,
        "runtime should select exactly one imported series local artwork when no thumbnail was previously selected",
    );
}

#[tokio::test]
async fn runtime_preserves_existing_non_generated_selection_when_importing_series_local_artworks() {
    let ctx =
        TestFixture::new("runtime-preserves-non-generated-selection-for-series-local-artworks")
            .await;

    let series_dir = ctx.paths().config_dir.join("series/series-1");
    std::fs::create_dir_all(&series_dir).expect("series artwork directory should exist");
    std::fs::write(series_dir.join("cover.png"), fixture_png_bytes())
        .expect("primary series local artwork should be written");
    std::fs::write(series_dir.join("poster.jpg"), fixture_png_bytes())
        .expect("secondary series local artwork should be written");

    let pool = connect_test_pool(ctx.paths().main_db.as_path(), 1)
        .await
        .expect("main db should open for non-generated series selection fixture setup");
    sqlx::query("UPDATE LIBRARY SET IMPORT_LOCAL_ARTWORK = 1 WHERE ID = ?")
        .bind("library-1")
        .execute(&pool)
        .await
        .expect("library import-local-artwork flag should be enabled");
    sqlx::query("DELETE FROM THUMBNAIL_SERIES WHERE SERIES_ID = ? AND TYPE = 'SIDECAR'")
        .bind("series-1")
        .execute(&pool)
        .await
        .expect("existing series sidecar thumbnails should be cleared before selection preservation test");
    sqlx::query(
        "INSERT INTO THUMBNAIL_SERIES (ID, SERIES_ID, TYPE, SELECTED, THUMBNAIL, MEDIA_TYPE) VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind("thumb-series-user-selected")
    .bind("series-1")
    .bind("USER_UPLOADED")
    .bind(true)
    .bind(fixture_png_bytes())
    .bind("image/png")
    .execute(&pool)
    .await
    .expect("existing non-generated selected series thumbnail should be seeded");
    pool.close().await;

    let runtime = runtime_task_context(ctx.paths()).await;
    let mut scheduler = TaskQueueScheduler::for_runtime(runtime.clone(), "rust-main").await;
    enqueue_refresh_series_local_artwork(&mut scheduler, "series-1").await;
    scheduler.process_available(&runtime.job()).await.expect(
        "series local artwork refresh should preserve existing non-generated selections cleanly",
    );

    let verify_pool = connect_test_pool(ctx.paths().main_db.as_path(), 1)
        .await
        .expect("main db should open for non-generated series selection verification");
    let rows = sqlx::query(
        "SELECT ID, TYPE, URL, SELECTED FROM THUMBNAIL_SERIES WHERE SERIES_ID = ? ORDER BY TYPE ASC, URL ASC, ID ASC",
    )
    .bind("series-1")
    .fetch_all(&verify_pool)
    .await
    .expect("series thumbnail rows should be queryable after preserving non-generated selection");
    verify_pool.close().await;

    let selected_rows = rows
        .iter()
        .filter(|row| row.get::<bool, _>("SELECTED"))
        .collect::<Vec<_>>();
    let imported_sidecars = rows
        .iter()
        .filter(|row| row.get::<String, _>("TYPE") == "SIDECAR")
        .collect::<Vec<_>>();

    assert_eq!(
        imported_sidecars.len(),
        2,
        "runtime should still import all matching series local artworks"
    );
    assert_eq!(
        selected_rows.len(),
        1,
        "runtime should keep exactly one selected series thumbnail"
    );
    assert_eq!(
        selected_rows[0].get::<String, _>("ID"),
        "thumb-series-user-selected"
    );
    assert_eq!(selected_rows[0].get::<String, _>("TYPE"), "USER_UPLOADED");
    assert!(
        imported_sidecars
            .iter()
            .all(|row| !row.get::<bool, _>("SELECTED")),
        "runtime should not override an existing non-generated selected series thumbnail when importing local artworks",
    );
}

#[tokio::test]
async fn runtime_replaces_generated_selection_when_importing_series_local_artworks() {
    let ctx =
        TestFixture::new("runtime-replaces-generated-selection-for-series-local-artworks").await;

    let series_dir = ctx.paths().config_dir.join("series/series-1");
    std::fs::create_dir_all(&series_dir).expect("series artwork directory should exist");
    std::fs::write(series_dir.join("cover.png"), fixture_png_bytes())
        .expect("primary series local artwork should be written");
    std::fs::write(series_dir.join("poster.jpg"), fixture_png_bytes())
        .expect("secondary series local artwork should be written");

    let pool = connect_test_pool(ctx.paths().main_db.as_path(), 1)
        .await
        .expect("main db should open for generated series selection fixture setup");
    sqlx::query("UPDATE LIBRARY SET IMPORT_LOCAL_ARTWORK = 1 WHERE ID = ?")
        .bind("library-1")
        .execute(&pool)
        .await
        .expect("library import-local-artwork flag should be enabled");
    sqlx::query("DELETE FROM THUMBNAIL_SERIES WHERE SERIES_ID = ?")
        .bind("series-1")
        .execute(&pool)
        .await
        .expect("existing series thumbnails should be cleared before generated selection test");
    sqlx::query(
        "INSERT INTO THUMBNAIL_SERIES (ID, SERIES_ID, TYPE, SELECTED, THUMBNAIL, MEDIA_TYPE) VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind("thumb-generated-series-1")
    .bind("series-1")
    .bind("GENERATED")
    .bind(true)
    .bind(fixture_png_bytes())
    .bind("image/png")
    .execute(&pool)
    .await
    .expect("generated selected series thumbnail should be seeded");
    pool.close().await;

    let runtime = runtime_task_context(ctx.paths()).await;
    let mut scheduler = TaskQueueScheduler::for_runtime(runtime.clone(), "rust-main").await;
    enqueue_refresh_series_local_artwork(&mut scheduler, "series-1").await;
    scheduler
        .process_available(&runtime.job())
        .await
        .expect("series local artwork refresh should replace generated selection cleanly");

    let verify_pool = connect_test_pool(ctx.paths().main_db.as_path(), 1)
        .await
        .expect("main db should open for generated series selection verification");
    let rows = sqlx::query(
        "SELECT ID, TYPE, URL, SELECTED FROM THUMBNAIL_SERIES WHERE SERIES_ID = ? ORDER BY TYPE ASC, URL ASC, ID ASC",
    )
    .bind("series-1")
    .fetch_all(&verify_pool)
    .await
    .expect("series thumbnail rows should be queryable after generated selection replacement");
    verify_pool.close().await;

    let selected_rows = rows
        .iter()
        .filter(|row| row.get::<bool, _>("SELECTED"))
        .collect::<Vec<_>>();
    let generated_rows = rows
        .iter()
        .filter(|row| row.get::<String, _>("TYPE") == "GENERATED")
        .collect::<Vec<_>>();
    let imported_sidecars = rows
        .iter()
        .filter(|row| row.get::<String, _>("TYPE") == "SIDECAR")
        .collect::<Vec<_>>();

    assert_eq!(
        imported_sidecars.len(),
        2,
        "runtime should import all matching series local artworks"
    );
    assert_eq!(
        generated_rows.len(),
        1,
        "runtime should retain the pre-existing generated series thumbnail row"
    );
    assert_eq!(
        selected_rows.len(),
        1,
        "runtime should keep exactly one selected series thumbnail after import"
    );
    assert_eq!(selected_rows[0].get::<String, _>("TYPE"), "SIDECAR");
    assert!(
        !generated_rows[0].get::<bool, _>("SELECTED"),
        "runtime should unselect previously selected GENERATED series thumbnails when the first local artwork is imported",
    );
}

#[tokio::test]
async fn runtime_series_local_artwork_refresh_emits_thumbnail_series_added_events() {
    let _guard = runtime_sse_contract_guard().await;
    let ctx = TestFixture::new("runtime-series-local-artwork-sse-events").await;

    let series_dir = ctx.paths().config_dir.join("series/series-1");
    std::fs::create_dir_all(&series_dir).expect("series artwork directory should exist");
    std::fs::write(series_dir.join("cover.png"), fixture_png_bytes())
        .expect("primary series artwork should be written");
    std::fs::write(series_dir.join("poster.jpg"), fixture_png_bytes())
        .expect("secondary series artwork should be written");

    let pool = connect_test_pool(ctx.paths().main_db.as_path(), 1)
        .await
        .expect("main db should open for series local artwork sse fixture setup");
    sqlx::query("UPDATE LIBRARY SET IMPORT_LOCAL_ARTWORK = 1 WHERE ID = ?")
        .bind("library-1")
        .execute(&pool)
        .await
        .expect("library import-local-artwork flag should be enabled");
    sqlx::query("UPDATE SERIES SET ONESHOT = 0 WHERE ID = ?")
        .bind("series-1")
        .execute(&pool)
        .await
        .expect("series oneshot flag should be disabled");
    sqlx::query("DELETE FROM THUMBNAIL_SERIES WHERE SERIES_ID = ?")
        .bind("series-1")
        .execute(&pool)
        .await
        .expect("existing series thumbnails should be cleared before local artwork sse test");
    pool.close().await;

    let cursor = komga_application::runtime_sse::current_runtime_sse_event_cursor();
    let runtime = runtime_task_context(ctx.paths()).await;
    let mut scheduler = TaskQueueScheduler::for_runtime(runtime.clone(), "rust-main").await;
    enqueue_refresh_series_local_artwork(&mut scheduler, "series-1").await;
    scheduler
        .process_available(&runtime.job())
        .await
        .expect("series local artwork refresh should complete for sse contract");

    let (_, events) = komga_application::runtime_sse::pending_runtime_sse_events(
        cursor,
        "runtime-contract-admin",
        true,
    );
    let thumbnail_events = events
        .iter()
        .filter(|event| event.name == "ThumbnailSeriesAdded")
        .filter(|event| {
            event
                .payload
                .get("seriesId")
                .and_then(|value| value.as_str())
                == Some("series-1")
        })
        .collect::<Vec<_>>();

    assert!(
        thumbnail_events.len() >= 2,
        "series local artwork refresh should emit thumbnail events for imported sidecar artwork rows",
    );
    let selected_states = thumbnail_events
        .iter()
        .filter_map(|event| event.payload.get("selected").and_then(Value::as_bool))
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(
        selected_states,
        std::collections::BTreeSet::from([false, true]),
        "series local artwork refresh should emit both selected and unselected ThumbnailSeriesAdded event states",
    );
}
