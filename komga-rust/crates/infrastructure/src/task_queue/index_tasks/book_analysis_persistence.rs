use komga_domain::discovery::MediaStatus;
use sqlx::SqlitePool;

#[derive(Clone, Debug)]
pub(super) struct BookAnalysisInput {
    pub(super) url: String,
    pub(super) root: String,
    pub(super) analyze_dimensions: bool,
    pub(super) series_id: String,
    pub(super) previous_media_status: Option<MediaStatus>,
    pub(super) previous_page_count: i64,
}

#[derive(Clone, Debug)]
pub(super) struct AnalyzedBookPage {
    pub(super) file_name: String,
    pub(super) media_type: String,
    pub(super) width: Option<i64>,
    pub(super) height: Option<i64>,
    pub(super) file_size: i64,
}

#[derive(Clone, Debug)]
pub(super) struct AnalyzedBookMedia {
    pub(super) status: MediaStatus,
    pub(super) media_type: String,
    pub(super) pages: Vec<AnalyzedBookPage>,
}

pub(super) async fn analyze_book_input(
    pool: &SqlitePool,
    book_id: &str,
) -> Result<Option<BookAnalysisInput>, String> {
    let row = sqlx::query(
        r#"SELECT
             b.URL AS URL,
             b.SERIES_ID AS SERIES_ID,
             l.ANALYZE_DIMENSIONS AS ANALYZE_DIMENSIONS,
             COALESCE(m.STATUS, '') AS PREVIOUS_MEDIA_STATUS,
             COALESCE(m.PAGE_COUNT, 0) AS PREVIOUS_PAGE_COUNT,
             l.ROOT AS ROOT
            FROM BOOK b
            JOIN LIBRARY l ON l.ID = b.LIBRARY_ID
           LEFT JOIN MEDIA m ON m.BOOK_ID = b.ID
              WHERE b.ID = ?
              LIMIT 1
             "#,
    )
    .bind(book_id)
    .fetch_optional(pool)
    .await
    .map_err(|error| format!("failed to load BOOK row for analyze: {error}"))?;

    Ok(row.map(|row| BookAnalysisInput {
        url: sqlx::Row::get::<String, _>(&row, "URL"),
        root: sqlx::Row::get::<String, _>(&row, "ROOT"),
        analyze_dimensions: sqlx::Row::get::<bool, _>(&row, "ANALYZE_DIMENSIONS"),
        series_id: sqlx::Row::get::<String, _>(&row, "SERIES_ID"),
        previous_media_status: MediaStatus::parse(
            sqlx::Row::get::<String, _>(&row, "PREVIOUS_MEDIA_STATUS").as_str(),
        ),
        previous_page_count: sqlx::Row::get::<i64, _>(&row, "PREVIOUS_PAGE_COUNT"),
    }))
}

pub(super) async fn persist_book_analysis(
    pool: &SqlitePool,
    book_id: &str,
    analysis: &AnalyzedBookMedia,
) -> Result<(), String> {
    let mut tx = pool.begin().await.map_err(|error| {
        format!("failed to start analyze-book transaction for '{book_id}': {error}")
    })?;

    sqlx::query("DELETE FROM MEDIA_PAGE WHERE BOOK_ID = ?")
        .bind(book_id)
        .execute(&mut *tx)
        .await
        .map_err(|error| format!("failed to clear MEDIA_PAGE rows for '{book_id}': {error}"))?;

    for (index, page) in analysis.pages.iter().enumerate() {
        sqlx::query(
            r#"INSERT INTO MEDIA_PAGE (
            FILE_NAME,
            MEDIA_TYPE,
            NUMBER,
            BOOK_ID,
            width,
            height,
            FILE_HASH,
            FILE_SIZE
        ) VALUES (?, ?, ?, ?, ?, ?, '', ?)"#,
        )
        .bind(&page.file_name)
        .bind(&page.media_type)
        .bind(index as i64)
        .bind(book_id)
        .bind(page.width)
        .bind(page.height)
        .bind(page.file_size)
        .execute(&mut *tx)
        .await
        .map_err(|error| format!("failed to insert MEDIA_PAGE row for '{book_id}': {error}"))?;
    }

    sqlx::query(
        r#"INSERT INTO MEDIA (
            BOOK_ID,
            STATUS,
            MEDIA_TYPE,
            PAGE_COUNT
        ) VALUES (?, ?, ?, ?)
        ON CONFLICT(BOOK_ID) DO UPDATE
        SET STATUS = excluded.STATUS,
            MEDIA_TYPE = excluded.MEDIA_TYPE,
            PAGE_COUNT = excluded.PAGE_COUNT,
            LAST_MODIFIED_DATE = CURRENT_TIMESTAMP"#,
    )
    .bind(book_id)
    .bind(analysis.status.persisted_name())
    .bind(&analysis.media_type)
    .bind(analysis.pages.len() as i32)
    .execute(&mut *tx)
    .await
    .map_err(|error| format!("failed to persist MEDIA analyze state: {error}"))?;

    sqlx::query("UPDATE BOOK SET LAST_MODIFIED_DATE = CURRENT_TIMESTAMP WHERE ID = ?")
        .bind(book_id)
        .execute(&mut *tx)
        .await
        .map_err(|error| {
            format!("failed to refresh BOOK last-modified during analyze for '{book_id}': {error}")
        })?;

    tx.commit().await.map_err(|error| {
        format!("failed to commit analyze-book transaction for '{book_id}': {error}")
    })?;

    Ok(())
}
