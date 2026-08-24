use anyhow::Context;
use std::collections::HashMap;
use std::path::PathBuf;

use komga_domain::discovery::MediaStatus;
use komga_domain::media_assets::ThumbnailType;
use sqlx::{Row, SqlitePool};

use komga_infrastructure_base::resolve_library_item_path;
use komga_infrastructure_media_core::expected_extension_for_media_type;

#[derive(Clone, Debug)]
pub(crate) struct PersistedLibraryHashingFlags {
    pub(crate) hash_files: bool,
    pub(crate) hash_pages: bool,
    pub(crate) hash_koreader: bool,
}

#[derive(Clone, Debug)]
pub(crate) struct PersistedLibraryMaintenanceFlags {
    pub(crate) repair_extensions: bool,
    pub(crate) convert_to_cbz: bool,
}

pub(crate) struct PersistedBookHashRuntimeState {
    pub(crate) library_id: String,
    pub(crate) file_hash: Option<String>,
    pub(crate) file_hash_koreader: Option<String>,
}

#[derive(Clone, Debug)]
pub(crate) struct PersistedBookArchiveSource {
    pub(crate) file_path: PathBuf,
    pub(crate) series_id: String,
    pub(crate) file_last_modified: i64,
    pub(crate) media_type: String,
    pub(crate) media_status: Option<MediaStatus>,
}

#[derive(Clone, Debug)]
pub(crate) struct PersistedExtensionRepairTarget {
    pub(crate) book_id: String,
    pub(crate) series_id: String,
    pub(crate) library_id: String,
    pub(crate) book_url: String,
    pub(crate) library_root: String,
    pub(crate) media_type: String,
}

#[derive(Clone, Debug)]
pub(crate) struct PersistedConversionTarget {
    pub(crate) book_url: String,
    pub(crate) series_id: String,
    pub(crate) library_id: String,
    pub(crate) library_root: String,
    pub(crate) file_last_modified: i64,
    pub(crate) convert_to_cbz: bool,
    pub(crate) media_type: String,
    pub(crate) media_status: Option<MediaStatus>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PersistedBookToConvert {
    pub book_id: String,
    pub series_id: String,
}

#[derive(Clone, Debug)]
pub(crate) struct PersistedHashedPageToDelete {
    pub(crate) file_hash: String,
    pub(crate) file_size: i64,
    pub(crate) file_name: String,
    pub(crate) media_type: String,
    pub(crate) page_number: i64,
}

pub(crate) async fn load_book_hashed_pages(
    pool: &SqlitePool,
    book_id: &str,
) -> anyhow::Result<Vec<PersistedHashedPageToDelete>> {
    let rows = sqlx::query(
        r#"
        SELECT
        FILE_HASH AS FILE_HASH,
        COALESCE(FILE_SIZE, -1) AS FILE_SIZE,
        FILE_NAME AS FILE_NAME,
        MEDIA_TYPE AS MEDIA_TYPE,
        NUMBER AS PAGE_NUMBER
        FROM MEDIA_PAGE
        WHERE BOOK_ID = ?
        ORDER BY NUMBER ASC
        "#,
    )
    .bind(book_id)
    .fetch_all(pool)
    .await
    .map_err(|error| {
        anyhow::anyhow!(error).context(format!("failed to load hashed pages for '{book_id}'"))
    })?;

    Ok(rows
        .into_iter()
        .map(|row| PersistedHashedPageToDelete {
            file_hash: row.get::<String, _>("FILE_HASH"),
            file_size: row.get::<i64, _>("FILE_SIZE"),
            file_name: row.get::<String, _>("FILE_NAME"),
            media_type: row.get::<String, _>("MEDIA_TYPE"),
            page_number: row.get::<i64, _>("PAGE_NUMBER") + 1,
        })
        .collect())
}

pub(crate) async fn load_library_hashing_flags(
    pool: &SqlitePool,
    library_id: &str,
) -> anyhow::Result<PersistedLibraryHashingFlags> {
    let row = sqlx::query(
        r#"
        SELECT
        HASH_FILES AS HASH_FILES,
        HASH_PAGES AS HASH_PAGES,
        HASH_KOREADER AS HASH_KOREADER
        FROM LIBRARY
        WHERE ID = ?
        LIMIT 1
        "#,
    )
    .bind(library_id)
    .fetch_optional(pool)
    .await
    .map_err(|error| {
        anyhow::anyhow!(error).context(format!(
            "failed to load library hashing flags for '{library_id}': "
        ))
    })?;

    let Some(row) = row else {
        return Err(anyhow::anyhow!(format!(
            "library '{library_id}' does not exist"
        )));
    };

    Ok(PersistedLibraryHashingFlags {
        hash_files: row.get::<i64, _>("HASH_FILES") != 0,
        hash_pages: row.get::<i64, _>("HASH_PAGES") != 0,
        hash_koreader: row.get::<i64, _>("HASH_KOREADER") != 0,
    })
}

pub(crate) async fn load_library_maintenance_flags(
    pool: &SqlitePool,
    library_id: &str,
) -> anyhow::Result<PersistedLibraryMaintenanceFlags> {
    let row = sqlx::query(
        r#"
        SELECT
        REPAIR_EXTENSIONS AS REPAIR_EXTENSIONS,
        CONVERT_TO_CBZ AS CONVERT_TO_CBZ
        FROM LIBRARY
        WHERE ID = ?
        LIMIT 1
        "#,
    )
    .bind(library_id)
    .fetch_optional(pool)
    .await
    .map_err(|error| {
        anyhow::anyhow!(error).context(format!(
            "failed to load library maintenance flags for '{library_id}': "
        ))
    })?;

    let Some(row) = row else {
        return Err(anyhow::anyhow!(format!(
            "library '{library_id}' does not exist"
        )));
    };

    Ok(PersistedLibraryMaintenanceFlags {
        repair_extensions: row.get::<i64, _>("REPAIR_EXTENSIONS") != 0,
        convert_to_cbz: row.get::<i64, _>("CONVERT_TO_CBZ") != 0,
    })
}

pub(crate) async fn load_book_library_id(
    pool: &SqlitePool,
    book_id: &str,
) -> anyhow::Result<Option<String>> {
    let row = sqlx::query(
        r#"
        SELECT LIBRARY_ID
        FROM BOOK
        WHERE ID = ?
        LIMIT 1
        "#,
    )
    .bind(book_id)
    .fetch_optional(pool)
    .await
    .map_err(|error| {
        anyhow::anyhow!(error).context(format!("failed to load book library for '{book_id}'"))
    })?;

    Ok(row.map(|row| row.get::<String, _>("LIBRARY_ID")))
}

pub(crate) async fn load_book_hash_runtime_state(
    pool: &SqlitePool,
    book_id: &str,
) -> anyhow::Result<Option<PersistedBookHashRuntimeState>> {
    let row = sqlx::query(
        r#"
        SELECT LIBRARY_ID,
               FILE_HASH,
               FILE_HASH_KOREADER
        FROM BOOK
        WHERE ID = ?
        LIMIT 1
        "#,
    )
    .bind(book_id)
    .fetch_optional(pool)
    .await
    .map_err(|error| {
        anyhow::anyhow!(error).context(format!(
            "failed to load book hash runtime state for '{book_id}': "
        ))
    })?;

    Ok(row.map(|row| PersistedBookHashRuntimeState {
        library_id: row.get::<String, _>("LIBRARY_ID"),
        file_hash: row.get::<Option<String>, _>("FILE_HASH"),
        file_hash_koreader: row.get::<Option<String>, _>("FILE_HASH_KOREADER"),
    }))
}

pub(crate) async fn load_book_file_path(
    pool: &SqlitePool,
    book_id: &str,
) -> anyhow::Result<Option<PathBuf>> {
    let row = sqlx::query(
        r#"
        SELECT
        b.URL AS URL,
        l.ROOT AS ROOT
        FROM BOOK b
        JOIN LIBRARY l ON l.ID = b.LIBRARY_ID
        WHERE b.ID = ?
        LIMIT 1
        "#,
    )
    .bind(book_id)
    .fetch_optional(pool)
    .await
    .map_err(|error| {
        anyhow::anyhow!(error).context(format!(
            "failed to query book file for hash task '{book_id}': "
        ))
    })?;

    Ok(row.map(|row| {
        resolve_library_item_path(
            row.get::<String, _>("ROOT").as_str(),
            row.get::<String, _>("URL").as_str(),
        )
    }))
}

pub async fn load_non_deleted_book_ids(pool: &SqlitePool) -> anyhow::Result<Vec<String>> {
    let rows = sqlx::query(
        r#"
        SELECT b.ID
        FROM BOOK b
        WHERE b.DELETED_DATE IS NULL
        ORDER BY b.ID ASC
        "#,
    )
    .fetch_all(pool)
    .await
    .map_err(|error| {
        anyhow::anyhow!(error)
            .context("failed to query non-deleted books for thumbnail regeneration: ")
    })?;

    Ok(rows
        .into_iter()
        .map(|row| row.get::<String, _>("ID"))
        .collect())
}

pub async fn load_books_with_undersized_generated_thumbnails(
    pool: &SqlitePool,
    max_edge: i64,
) -> anyhow::Result<Vec<String>> {
    let rows = sqlx::query(
        r#"
        SELECT DISTINCT BOOK_ID
        FROM THUMBNAIL_BOOK
        WHERE TYPE = ?
        AND WIDTH < ?
        AND HEIGHT < ?
        "#,
    )
    .bind(ThumbnailType::Generated.persisted_name())
    .bind(max_edge)
    .bind(max_edge)
    .fetch_all(pool)
    .await
    .map_err(|error| {
        anyhow::anyhow!(error)
            .context("failed to query books with undersized generated thumbnails: ")
    })?;

    Ok(rows
        .into_iter()
        .map(|row| row.get::<String, _>("BOOK_ID"))
        .collect())
}

pub async fn load_books_with_missing_page_hash(
    pool: &SqlitePool,
    library_id: Option<&str>,
) -> anyhow::Result<Vec<String>> {
    let library_id = library_id.map(str::to_string);
    let rows = if let Some(library_id) = library_id.as_deref() {
        sqlx::query(
            r#"
            SELECT DISTINCT mp.BOOK_ID AS BOOK_ID
            FROM MEDIA_PAGE mp
            JOIN BOOK b ON b.ID = mp.BOOK_ID
            WHERE b.LIBRARY_ID = ?
            AND (mp.FILE_HASH = '' OR mp.FILE_HASH IS NULL)
            "#,
        )
        .bind(library_id)
        .fetch_all(pool)
        .await
    } else {
        sqlx::query(
            r#"
            SELECT DISTINCT BOOK_ID
            FROM MEDIA_PAGE
            WHERE FILE_HASH = ''
            OR FILE_HASH IS NULL
            "#,
        )
        .fetch_all(pool)
        .await
    }
    .context("failed to query books with missing page hashes")?;

    Ok(rows
        .into_iter()
        .map(|row| row.get::<String, _>("BOOK_ID"))
        .collect())
}

pub(crate) async fn load_duplicate_pages_to_delete(
    pool: &SqlitePool,
    library_id: &str,
) -> anyhow::Result<HashMap<String, Vec<PersistedHashedPageToDelete>>> {
    let library_id = library_id.to_string();
    let rows = sqlx::query(
        r#"
        SELECT
        mp.BOOK_ID AS BOOK_ID,
        mp.FILE_HASH AS FILE_HASH,
        mp.NUMBER AS PAGE_NUMBER,
        mp.FILE_NAME AS FILE_NAME,
        mp.MEDIA_TYPE AS MEDIA_TYPE,
        mp.FILE_SIZE AS FILE_SIZE
        FROM MEDIA_PAGE mp
        JOIN BOOK b ON b.ID = mp.BOOK_ID
        JOIN PAGE_HASH ph ON ph.HASH = mp.FILE_HASH
        WHERE b.LIBRARY_ID = ?
        AND b.DELETED_DATE IS NULL
        AND mp.FILE_HASH <> ''
        AND ph.ACTION = 'DELETE_AUTO'
        AND mp.FILE_HASH IN (
            SELECT mp2.FILE_HASH
            FROM MEDIA_PAGE mp2
            JOIN BOOK b2 ON b2.ID = mp2.BOOK_ID
            WHERE b2.LIBRARY_ID = ?
            AND b2.DELETED_DATE IS NULL
            AND mp2.FILE_HASH <> ''
            GROUP BY mp2.FILE_HASH
            HAVING COUNT(*) > 1
        )
        ORDER BY mp.BOOK_ID ASC, mp.NUMBER ASC
        "#,
    )
    .bind(&library_id)
    .bind(&library_id)
    .fetch_all(pool)
    .await
    .map_err(|error| {
        anyhow::anyhow!(error).context(format!(
            "failed to query duplicate pages to delete for '{library_id}': "
        ))
    })?;

    let mut by_book = HashMap::<String, Vec<PersistedHashedPageToDelete>>::new();
    for row in rows {
        let book_id = row.get::<String, _>("BOOK_ID");
        by_book
            .entry(book_id)
            .or_default()
            .push(PersistedHashedPageToDelete {
                file_hash: row.get::<String, _>("FILE_HASH"),
                file_size: row.get::<i64, _>("FILE_SIZE"),
                file_name: row.get::<String, _>("FILE_NAME"),
                media_type: row.get::<String, _>("MEDIA_TYPE"),
                page_number: row.get::<i64, _>("PAGE_NUMBER") + 1,
            });
    }

    Ok(by_book)
}

pub(crate) async fn load_books_requiring_analysis(
    pool: &SqlitePool,
    book_ids: &[String],
) -> anyhow::Result<Vec<String>> {
    if book_ids.is_empty() {
        return Ok(Vec::new());
    }

    let mut result = Vec::new();

    for book_id in book_ids {
        let status = sqlx::query(
            r#"
            SELECT STATUS
            FROM MEDIA
            WHERE BOOK_ID = ?
            LIMIT 1
            "#,
        )
        .bind(book_id)
        .fetch_optional(pool)
        .await
        .map_err(|error| {
            anyhow::anyhow!(error).context(format!("failed to query media status for '{book_id}'"))
        })?
        .map(|row| row.get::<String, _>("STATUS"));

        let needs_analysis = match status.as_deref() {
            None => true,
            Some(status) => matches!(
                MediaStatus::parse(status),
                Some(MediaStatus::Unknown | MediaStatus::Outdated)
            ),
        };

        if needs_analysis {
            result.push(book_id.clone());
        }
    }

    Ok(result)
}

pub(crate) async fn load_books_with_missing_file_hash(
    pool: &SqlitePool,
    library_id: &str,
    koreader: bool,
) -> anyhow::Result<Vec<String>> {
    let query = if koreader {
        sqlx::query(
            r#"
            SELECT ID
            FROM BOOK
            WHERE LIBRARY_ID = ?
            AND DELETED_DATE IS NULL
            AND (FILE_HASH_KOREADER = '' OR FILE_HASH_KOREADER IS NULL)
            "#,
        )
    } else {
        sqlx::query(
            r#"
            SELECT ID
            FROM BOOK
            WHERE LIBRARY_ID = ?
            AND DELETED_DATE IS NULL
            AND (FILE_HASH = '' OR FILE_HASH IS NULL)
            "#,
        )
    };

    let rows = query
        .bind(library_id)
        .fetch_all(pool)
        .await
        .map_err(|error| {
            anyhow::anyhow!(error).context(format!(
                "failed to query books with missing file hash for '{library_id}': "
            ))
        })?;

    Ok(rows
        .into_iter()
        .map(|row| row.get::<String, _>("ID"))
        .collect())
}

pub(crate) async fn load_books_to_convert(
    pool: &SqlitePool,
    library_id: &str,
) -> anyhow::Result<Vec<PersistedBookToConvert>> {
    let rows = sqlx::query(
        r#"
        SELECT ID, SERIES_ID
        FROM BOOK
        JOIN MEDIA ON MEDIA.BOOK_ID = BOOK.ID
        WHERE LIBRARY_ID = ?
        AND DELETED_DATE IS NULL
        AND LOWER(MEDIA.MEDIA_TYPE) IN (
            'application/x-rar-compressed; version=4',
            'application/x-rar-compressed; version=5'
        )
        "#,
    )
    .bind(library_id)
    .fetch_all(pool)
    .await
    .map_err(|error| {
        anyhow::anyhow!(error).context(format!(
            "failed to query books to convert for '{library_id}': "
        ))
    })?;

    Ok(rows
        .into_iter()
        .map(|row| PersistedBookToConvert {
            book_id: row.get::<String, _>("ID"),
            series_id: row.get::<String, _>("SERIES_ID"),
        })
        .collect())
}

pub(crate) async fn load_book_conversion_target(
    pool: &SqlitePool,
    book_id: &str,
) -> anyhow::Result<Option<PersistedConversionTarget>> {
    let row = sqlx::query(
        r#"
        SELECT
         b.URL AS BOOK_URL,
         b.SERIES_ID AS SERIES_ID,
         b.LIBRARY_ID AS LIBRARY_ID,
         l.ROOT AS LIBRARY_ROOT,
         unixepoch(b.FILE_LAST_MODIFIED) AS FILE_LAST_MODIFIED,
        COALESCE(l.CONVERT_TO_CBZ, 0) AS CONVERT_TO_CBZ,
        COALESCE(m.MEDIA_TYPE, '') AS MEDIA_TYPE,
        COALESCE(m.STATUS, '') AS MEDIA_STATUS
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
    .map_err(|error| {
        anyhow::anyhow!(error).context(format!(
            "failed to load convert-book source row for '{book_id}': "
        ))
    })?;

    Ok(row.map(|row| PersistedConversionTarget {
        book_url: row.get::<String, _>("BOOK_URL"),
        series_id: row.get::<String, _>("SERIES_ID"),
        library_id: row.get::<String, _>("LIBRARY_ID"),
        library_root: row.get::<String, _>("LIBRARY_ROOT"),
        file_last_modified: row.get::<i64, _>("FILE_LAST_MODIFIED"),
        convert_to_cbz: row.get::<i64, _>("CONVERT_TO_CBZ") != 0,
        media_type: row.get::<String, _>("MEDIA_TYPE"),
        media_status: MediaStatus::parse(&row.get::<String, _>("MEDIA_STATUS")),
    }))
}

pub(crate) async fn load_books_for_extension_repair(
    pool: &SqlitePool,
    library_id: &str,
) -> anyhow::Result<Vec<PersistedExtensionRepairTarget>> {
    let rows = sqlx::query(
        r#"
        SELECT
        b.ID AS BOOK_ID,
        b.SERIES_ID AS SERIES_ID,
        b.LIBRARY_ID AS LIBRARY_ID,
        b.URL AS BOOK_URL,
        l.ROOT AS LIBRARY_ROOT,
        m.MEDIA_TYPE AS MEDIA_TYPE
        FROM BOOK b
        JOIN LIBRARY l ON l.ID = b.LIBRARY_ID
        JOIN MEDIA m ON m.BOOK_ID = b.ID
        WHERE b.LIBRARY_ID = ?
        AND b.DELETED_DATE IS NULL
        "#,
    )
    .bind(library_id)
    .fetch_all(pool)
    .await
    .map_err(|error| {
        anyhow::anyhow!(error).context(format!(
            "failed to query books for extension repair in '{library_id}': "
        ))
    })?;

    Ok(rows
        .into_iter()
        .filter_map(|row| {
            let media_type = row.get::<String, _>("MEDIA_TYPE");
            let expected_extension = expected_extension_for_media_type(&media_type)?;
            let book_url = row.get::<String, _>("BOOK_URL");
            let library_root = row.get::<String, _>("LIBRARY_ROOT");
            let current_extension = resolve_library_item_path(&library_root, &book_url)
                .extension()
                .and_then(|value| value.to_str())
                .map(|value| value.to_ascii_lowercase())
                .unwrap_or_default();
            (current_extension != expected_extension).then(|| PersistedExtensionRepairTarget {
                book_id: row.get::<String, _>("BOOK_ID"),
                series_id: row.get::<String, _>("SERIES_ID"),
                library_id: row.get::<String, _>("LIBRARY_ID"),
                book_url,
                library_root,
                media_type,
            })
        })
        .collect())
}

pub(crate) async fn load_book_for_extension_repair(
    pool: &SqlitePool,
    book_id: &str,
) -> anyhow::Result<Option<PersistedExtensionRepairTarget>> {
    let row = sqlx::query(
        r#"
        SELECT
        b.ID AS BOOK_ID,
        b.SERIES_ID AS SERIES_ID,
        b.LIBRARY_ID AS LIBRARY_ID,
        b.URL AS BOOK_URL,
        l.ROOT AS LIBRARY_ROOT,
        COALESCE(m.MEDIA_TYPE, '') AS MEDIA_TYPE
        FROM BOOK b
        JOIN LIBRARY l ON l.ID = b.LIBRARY_ID
        LEFT JOIN MEDIA m ON m.BOOK_ID = b.ID
        WHERE b.ID = ?
        AND b.DELETED_DATE IS NULL
        LIMIT 1
        "#,
    )
    .bind(book_id)
    .fetch_optional(pool)
    .await
    .map_err(|error| {
        anyhow::anyhow!(error).context(format!(
            "failed to load repair-extension source row for '{book_id}': "
        ))
    })?;

    Ok(row.map(|row| PersistedExtensionRepairTarget {
        book_id: row.get::<String, _>("BOOK_ID"),
        series_id: row.get::<String, _>("SERIES_ID"),
        library_id: row.get::<String, _>("LIBRARY_ID"),
        book_url: row.get::<String, _>("BOOK_URL"),
        library_root: row.get::<String, _>("LIBRARY_ROOT"),
        media_type: row.get::<String, _>("MEDIA_TYPE"),
    }))
}

pub(crate) async fn load_book_archive_source(
    pool: &SqlitePool,
    book_id: &str,
) -> anyhow::Result<Option<PersistedBookArchiveSource>> {
    let row = sqlx::query(
        r#"
        SELECT
        b.URL AS BOOK_URL,
        b.SERIES_ID AS SERIES_ID,
        unixepoch(b.FILE_LAST_MODIFIED) AS FILE_LAST_MODIFIED,
        l.ROOT AS LIBRARY_ROOT,
        COALESCE(m.MEDIA_TYPE, '') AS MEDIA_TYPE,
        COALESCE(m.STATUS, '') AS MEDIA_STATUS
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
    .map_err(|error| {
        anyhow::anyhow!(error).context(format!("failed to load archive source for '{book_id}'"))
    })?;

    Ok(row.map(|row| PersistedBookArchiveSource {
        file_path: resolve_library_item_path(
            row.get::<String, _>("LIBRARY_ROOT").as_str(),
            row.get::<String, _>("BOOK_URL").as_str(),
        ),
        series_id: row.get::<String, _>("SERIES_ID"),
        file_last_modified: row.get::<i64, _>("FILE_LAST_MODIFIED"),
        media_type: row.get::<String, _>("MEDIA_TYPE"),
        media_status: MediaStatus::parse(&row.get::<String, _>("MEDIA_STATUS")),
    }))
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;
    use komga_infrastructure_base::sqlite::{connect_test_pool, schema};

    fn temp_db_path(case_id: &str) -> PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system time should be after unix epoch")
            .as_nanos();
        std::env::temp_dir().join(format!("komga-rust-media-queries-{case_id}-{nanos}.sqlite"))
    }

    async fn open_bootstrapped_test_pool(case_id: &str) -> (PathBuf, SqlitePool) {
        let db_path = temp_db_path(case_id);
        let pool = connect_test_pool(db_path.as_path(), 1)
            .await
            .expect("temporary sqlite db should open");
        schema::bootstrap_pool(&pool)
            .await
            .expect("temporary sqlite db should bootstrap main schema");
        (db_path, pool)
    }

    async fn close_test_pool(db_path: PathBuf, pool: SqlitePool) {
        pool.close().await;
        let _ = std::fs::remove_file(db_path);
    }

    #[tokio::test]
    async fn library_backed_queries_reject_missing_rows() {
        let (db_path, pool) = open_bootstrapped_test_pool("missing-library-backed-rows").await;
        let hashing_error = load_library_hashing_flags(&pool, "missing-library")
            .await
            .expect_err("missing library hashing flags should fail");
        let maintenance_error = load_library_maintenance_flags(&pool, "missing-library")
            .await
            .expect_err("missing library maintenance flags should fail");

        assert!(hashing_error.to_string().contains("missing-library"));
        assert!(maintenance_error.to_string().contains("missing-library"));

        close_test_pool(db_path, pool).await;
    }
}
