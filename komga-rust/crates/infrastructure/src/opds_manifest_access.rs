use std::path::Path;

use crate::sqlite::connect_read_pool;
use sqlx::Row;

pub struct ManifestBookRecord {
    pub title: String,
    pub file_name: String,
    pub media_type: Option<String>,
    pub page_count: i64,
}

pub async fn load_manifest_book_record(
    database_file: &Path,
    book_id: &str,
) -> Result<Option<ManifestBookRecord>, sqlx::Error> {
    let pool = connect_read_pool(database_file).await?;
    let row = sqlx::query(
        r#"SELECT COALESCE(bm.TITLE, b.NAME) AS TITLE, b.NAME AS NAME, m.MEDIA_TYPE AS MEDIA_TYPE,
               COALESCE(m.PAGE_COUNT, 1) AS PAGE_COUNT
        FROM BOOK b
        LEFT
        JOIN BOOK_METADATA bm ON bm.BOOK_ID = b.ID
        LEFT
        JOIN MEDIA m ON m.BOOK_ID = b.ID
        WHERE b.ID = ?
        LIMIT 1"#,
    )
    .bind(book_id)
    .fetch_optional(&pool)
    .await?;

    Ok(row.map(|row| ManifestBookRecord {
        title: row.get::<String, _>("TITLE"),
        file_name: row.get::<String, _>("NAME"),
        media_type: row.try_get::<String, _>("MEDIA_TYPE").ok(),
        page_count: row.try_get::<i64, _>("PAGE_COUNT").unwrap_or(1),
    }))
}
