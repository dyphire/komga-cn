use super::*;

#[derive(Debug, Eq, PartialEq)]
pub(super) struct PersistenceSnapshot {
    pub(super) library_rows: i64,
    pub(super) series_rows: i64,
    pub(super) series_metadata_rows: i64,
    pub(super) book_metadata_aggregation_rows: i64,
    pub(super) book_rows: i64,
    pub(super) book_metadata_rows: i64,
    pub(super) media_file_rows: i64,
    pub(super) sidecar_rows: i64,
}

#[derive(Debug, Eq, PartialEq)]
pub(super) struct TaskSnapshot {
    pub(super) task_rows: i64,
}

pub(super) struct ScannerPersistenceFixture {
    pub(super) paths: persistence_contract_fixture::RuntimeDbPaths,
    pub(super) library_root: PathBuf,
    pub(super) config: RuntimeConfig,
}

impl ScannerPersistenceFixture {
    pub(super) async fn new(case_id: &str) -> anyhow::Result<Self> {
        let paths = persistence_contract_fixture::new_runtime_db_paths(case_id)?;
        persistence_contract_fixture::seed_main_db_from_flyway(&paths.main_db).await?;
        persistence_contract_fixture::seed_tasks_db_from_flyway(&paths.tasks_db).await?;

        let library_root = create_scannable_library_root(&paths.config_dir)?;
        seed_library_row(&paths.main_db, "library-1", &library_root).await?;

        let config = runtime_config_for_paths(&paths);
        Ok(Self {
            paths,
            library_root,
            config,
        })
    }

    pub(super) fn cleanup(self) {
        persistence_contract_fixture::cleanup(self.paths);
    }
}

pub(super) fn scan_library_task_id(library_id: &str, deep_scan: bool) -> String {
    format!("SCAN_LIBRARY:{library_id}:DEEP:{deep_scan}")
}

pub(super) fn scan_library_task_payload(
    library_id: &str,
    priority: i32,
    deep_scan: bool,
) -> String {
    json!({
        "libraryId": library_id,
        "scanDeep": deep_scan,
        "priority": priority,
        "groupId": Value::Null,
        "uniqueId": scan_library_task_id(library_id, deep_scan),
    })
    .to_string()
}

pub(super) fn scan_library_task(
    library_id: &str,
    priority: i32,
    deep_scan: bool,
) -> TaskQueueRecord {
    TaskQueueRecord::new(scan_library_task_id(library_id, deep_scan), priority, None)
        .with_simple_type("SCAN_LIBRARY")
        .with_payload(scan_library_task_payload(library_id, priority, deep_scan))
}

pub(super) fn create_scannable_library_root(config_dir: &Path) -> anyhow::Result<PathBuf> {
    let root = config_dir.join("library-root");
    let series_dir = root.join("Series-A");

    fs::create_dir_all(&series_dir)?;
    write_scannable_cbz_fixture(&series_dir.join("Book-001.cbz"), b"default-page")?;
    fs::write(series_dir.join("Book-001.xml"), b"<ComicInfo></ComicInfo>")?;
    fs::write(series_dir.join("ComicInfo.xml"), b"<ComicInfo></ComicInfo>")?;

    Ok(root)
}

pub(super) fn write_scannable_cbz_fixture(path: &Path, page_marker: &[u8]) -> anyhow::Result<i64> {
    let file = File::create(path)?;
    let mut zip = ZipWriter::new(file);
    let options = SimpleFileOptions::default()
        .compression_method(CompressionMethod::Stored)
        .unix_permissions(0o644);
    let page_bytes = render_scannable_png_bytes(page_marker)?;

    zip.start_file("page-1.png", options)?;
    zip.write_all(&page_bytes)?;
    zip.finish()?;

    Ok(i64::try_from(page_bytes.len()).expect("fixture page size should fit into i64"))
}

pub(super) fn render_scannable_png_bytes(page_marker: &[u8]) -> anyhow::Result<Vec<u8>> {
    let pixels = if page_marker.is_empty() {
        vec![0_u8]
    } else {
        page_marker.to_vec()
    };
    let width = u32::try_from(pixels.len()).expect("fixture width should fit into u32");
    let mut image = image::RgbaImage::new(width, 1);

    for (index, value) in pixels.into_iter().enumerate() {
        image.put_pixel(
            index as u32,
            0,
            image::Rgba([value, value.wrapping_add(31), value ^ 0x5A, 0xFF]),
        );
    }

    let mut output = std::io::Cursor::new(Vec::new());
    image::DynamicImage::ImageRgba8(image).write_to(&mut output, image::ImageFormat::Png)?;
    Ok(output.into_inner())
}

pub(super) async fn seed_library_row(
    main_db: &Path,
    library_id: &str,
    root: &Path,
) -> anyhow::Result<()> {
    let pool = connect_pool(main_db, 1).await?;
    sqlx::query(
        "INSERT INTO LIBRARY (ID, NAME, ROOT) \
                 VALUES (?, ?, ?)",
    )
    .bind(library_id)
    .bind("Scanner Persistence Contract Library")
    .bind(root.to_string_lossy().to_string())
    .execute(&pool)
    .await?;
    pool.close().await;
    Ok(())
}

pub(super) async fn update_series_file_last_modified(
    main_db: &Path,
    series_url: &str,
    file_last_modified: i64,
) {
    let pool = connect_pool(main_db, 1)
        .await
        .expect("sqlite pool should open for series last-modified update");
    sqlx::query("UPDATE SERIES SET FILE_LAST_MODIFIED = ? WHERE URL = ?")
        .bind(file_last_modified)
        .bind(series_url)
        .execute(&pool)
        .await
        .expect("series last-modified should be updated for deleted-books scan contract");
    pool.close().await;
}

pub(super) async fn update_library_oneshots_directory(
    main_db: &Path,
    library_id: &str,
    oneshots_directory: Option<&str>,
) {
    let pool = connect_pool(main_db, 1)
        .await
        .expect("sqlite pool should open for library oneshots-directory update");
    sqlx::query("UPDATE LIBRARY SET ONESHOTS_DIRECTORY = ? WHERE ID = ?")
        .bind(oneshots_directory)
        .bind(library_id)
        .execute(&pool)
        .await
        .expect("library oneshots-directory should be updated for scanner oneshot contract");
    pool.close().await;
}

pub(super) async fn update_active_book_url(main_db: &Path, from_book_url: &str, to_book_url: &str) {
    let pool = connect_pool(main_db, 1)
        .await
        .expect("sqlite pool should open for active book url update");
    sqlx::query(
        "UPDATE BOOK SET URL = ?, LAST_MODIFIED_DATE = CURRENT_TIMESTAMP WHERE URL = ? AND DELETED_DATE IS NULL",
    )
    .bind(to_book_url)
    .bind(from_book_url)
    .execute(&pool)
    .await
    .expect("active book url should be updated for scanner oneshot contract");
    pool.close().await;
}

pub(super) async fn load_active_series_id_for_book_url(main_db: &Path, book_url: &str) -> String {
    let pool = connect_pool(main_db, 1)
        .await
        .expect("sqlite pool should open for active series id lookup");
    let series_id =
        sqlx::query("SELECT SERIES_ID FROM BOOK WHERE URL = ? AND DELETED_DATE IS NULL LIMIT 1")
            .bind(book_url)
            .fetch_one(&pool)
            .await
            .expect("active book row should be queryable for series id lookup")
            .get::<String, _>("SERIES_ID");
    pool.close().await;
    series_id
}

pub(super) async fn load_series_url_by_id(main_db: &Path, series_id: &str) -> String {
    let pool = connect_pool(main_db, 1)
        .await
        .expect("sqlite pool should open for series url lookup");
    let series_url =
        sqlx::query("SELECT URL FROM SERIES WHERE ID = ? AND DELETED_DATE IS NULL LIMIT 1")
            .bind(series_id)
            .fetch_one(&pool)
            .await
            .expect("active series row should be queryable for url lookup")
            .get::<String, _>("URL");
    pool.close().await;
    series_url
}

pub(super) async fn load_active_series_count(main_db: &Path, library_id: &str) -> i64 {
    let pool = connect_pool(main_db, 1)
        .await
        .expect("sqlite pool should open for active series count lookup");
    let series_count = sqlx::query(
        "SELECT COUNT(*) AS COUNT FROM SERIES WHERE LIBRARY_ID = ? AND DELETED_DATE IS NULL",
    )
    .bind(library_id)
    .fetch_one(&pool)
    .await
    .expect("active series count should be queryable")
    .get::<i64, _>("COUNT");
    pool.close().await;
    series_count
}

pub(super) async fn assert_persisted_task_shape(
    tasks_db: &Path,
    id: &str,
    class: &str,
    simple_type: &str,
    group_id: Option<&str>,
    payload: Value,
) {
    let pool = connect_pool(tasks_db, 1)
        .await
        .expect("tasks db should open for persisted task verification");
    let row =
        sqlx::query("SELECT CLASS, SIMPLE_TYPE, GROUP_ID, PAYLOAD FROM TASK WHERE ID = ? LIMIT 1")
            .bind(id)
            .fetch_one(&pool)
            .await
            .expect("persisted task row should be queryable");
    pool.close().await;

    let stored_group_id = row.get::<Option<String>, _>("GROUP_ID");
    let stored_payload = serde_json::from_str::<Value>(&row.get::<String, _>("PAYLOAD"))
        .expect("persisted task payload should be valid json");

    assert_eq!(row.get::<String, _>("CLASS"), class);
    assert_eq!(row.get::<String, _>("SIMPLE_TYPE"), simple_type);
    assert_eq!(stored_group_id.as_deref(), group_id);
    assert_eq!(stored_payload, payload);
}

pub(super) fn runtime_config_for_paths(
    paths: &persistence_contract_fixture::RuntimeDbPaths,
) -> RuntimeConfig {
    let mut env = BTreeMap::new();
    env.insert(
        "KOMGA_CONFIG_DIR".to_string(),
        paths.config_dir.to_string_lossy().to_string(),
    );
    env.insert(
        "KOMGA_DATABASE_FILE".to_string(),
        paths.main_db.to_string_lossy().to_string(),
    );
    env.insert(
        "KOMGA_TASKS_DB_FILE".to_string(),
        paths.tasks_db.to_string_lossy().to_string(),
    );

    RuntimeConfig::resolve_with_env(&RuntimeCli::default(), &env)
        .expect("runtime config should resolve scanner persistence fixture paths")
}

pub(super) async fn load_persistence_snapshot(
    main_db: &Path,
    library_id: &str,
) -> PersistenceSnapshot {
    let pool = connect_pool(main_db, 1)
        .await
        .expect("sqlite pool should open for scanner persistence inspection");

    let library_rows = sqlx::query(
        "SELECT COUNT(*) AS COUNT \
                                    FROM LIBRARY \
                                    WHERE ID = ?",
    )
    .bind(library_id)
    .fetch_one(&pool)
    .await
    .expect("library row count should be queryable")
    .get::<i64, _>("COUNT");

    let series_rows = sqlx::query(
        "SELECT COUNT(*) AS COUNT \
                                   FROM SERIES \
                                   WHERE LIBRARY_ID = ?",
    )
    .bind(library_id)
    .fetch_one(&pool)
    .await
    .expect("series row count should be queryable")
    .get::<i64, _>("COUNT");

    let series_metadata_rows = sqlx::query(
        "SELECT COUNT(*) AS COUNT \
         FROM SERIES_METADATA sm \
         JOIN SERIES s ON s.ID = sm.SERIES_ID \
         WHERE s.LIBRARY_ID = ?",
    )
    .bind(library_id)
    .fetch_one(&pool)
    .await
    .expect("series metadata row count should be queryable")
    .get::<i64, _>("COUNT");

    let book_metadata_aggregation_rows = sqlx::query(
        "SELECT COUNT(*) AS COUNT \
         FROM BOOK_METADATA_AGGREGATION bma \
         JOIN SERIES s ON s.ID = bma.SERIES_ID \
         WHERE s.LIBRARY_ID = ?",
    )
    .bind(library_id)
    .fetch_one(&pool)
    .await
    .expect("book metadata aggregation row count should be queryable")
    .get::<i64, _>("COUNT");

    let book_rows = sqlx::query(
        "SELECT COUNT(*) AS COUNT \
                                 FROM BOOK \
                                 WHERE LIBRARY_ID = ?",
    )
    .bind(library_id)
    .fetch_one(&pool)
    .await
    .expect("book row count should be queryable")
    .get::<i64, _>("COUNT");

    let book_metadata_rows = sqlx::query(
        "SELECT COUNT(*) AS COUNT \
         FROM BOOK_METADATA bm \
         JOIN BOOK b ON b.ID = bm.BOOK_ID \
         WHERE b.LIBRARY_ID = ?",
    )
    .bind(library_id)
    .fetch_one(&pool)
    .await
    .expect("book metadata row count should be queryable")
    .get::<i64, _>("COUNT");

    let media_file_rows = sqlx::query(
        "SELECT COUNT(*) AS COUNT \
         FROM MEDIA_FILE \
         WHERE BOOK_ID IN (SELECT ID \
         FROM BOOK \
         WHERE LIBRARY_ID = ?)",
    )
    .bind(library_id)
    .fetch_one(&pool)
    .await
    .expect("media_file row count should be queryable")
    .get::<i64, _>("COUNT");

    let sidecar_rows = sqlx::query(
        "SELECT COUNT(*) AS COUNT \
                                    FROM SIDECAR \
                                    WHERE LIBRARY_ID = ?",
    )
    .bind(library_id)
    .fetch_one(&pool)
    .await
    .expect("sidecar row count should be queryable")
    .get::<i64, _>("COUNT");

    pool.close().await;

    PersistenceSnapshot {
        library_rows,
        series_rows,
        series_metadata_rows,
        book_metadata_aggregation_rows,
        book_rows,
        book_metadata_rows,
        media_file_rows,
        sidecar_rows,
    }
}

pub(super) async fn load_task_snapshot(tasks_db: &Path) -> TaskSnapshot {
    let pool = connect_pool(tasks_db, 1)
        .await
        .expect("sqlite pool should open for scanner task inspection");

    let task_rows = sqlx::query(
        "SELECT COUNT(*) AS COUNT \
                                 FROM TASK",
    )
    .fetch_one(&pool)
    .await
    .expect("task row count should be queryable")
    .get::<i64, _>("COUNT");

    pool.close().await;

    TaskSnapshot { task_rows }
}

pub(super) async fn load_media_ready_count(main_db: &Path) -> i64 {
    let pool = connect_pool(main_db, 1)
        .await
        .expect("sqlite pool should open for media status inspection");
    let count = sqlx::query(
        "SELECT COUNT(*) AS COUNT \
                             FROM MEDIA \
                             WHERE STATUS = 'READY'",
    )
    .fetch_one(&pool)
    .await
    .expect("media READY count should be queryable")
    .get::<i64, _>("COUNT");
    pool.close().await;
    count
}

pub(super) async fn load_book_file_size(main_db: &Path, book_url: &str) -> i64 {
    let pool = connect_pool(main_db, 1)
        .await
        .expect("sqlite pool should open for book file size inspection");
    let file_size = sqlx::query(
        "SELECT FILE_SIZE \
                                 FROM BOOK \
                                 WHERE URL = ? \
                                 LIMIT 1",
    )
    .bind(book_url)
    .fetch_one(&pool)
    .await
    .expect("book row should be queryable by URL for rescan contract")
    .get::<i64, _>("FILE_SIZE");
    pool.close().await;
    file_size
}

pub(super) async fn load_media_page_file_size(main_db: &Path, book_url: &str) -> i64 {
    let pool = connect_pool(main_db, 1)
        .await
        .expect("sqlite pool should open for media page size inspection");
    let file_size = sqlx::query(
        "SELECT mp.FILE_SIZE \
         FROM MEDIA_PAGE mp \
         JOIN BOOK b ON b.ID = mp.BOOK_ID \
         WHERE b.URL = ? \
         ORDER BY mp.NUMBER ASC \
         LIMIT 1",
    )
    .bind(book_url)
    .fetch_one(&pool)
    .await
    .expect("media page row should be queryable by book url for deep-scan contract")
    .get::<i64, _>("FILE_SIZE");
    pool.close().await;
    file_size
}
