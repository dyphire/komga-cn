use anyhow::Context;
use komga_application::media_assets::{
    ArchiveEntry, BookAccessRestrictions, BookMediaRecord, BookPageRecord, EpubExtensionBlob,
    ManifestBookRecord, SeriesArchiveEntries, SeriesBookNumberSort, content_type_from_filename,
};
use komga_domain::discovery::MediaStatus;
use sqlx::sqlite::SqliteRow;
use sqlx::{Row, SqlitePool};

use crate::parsing::{clamp_kotlin_int_u32, parse_sqlite_group_concat_values};
use crate::resolve_library_item_path;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct PersistedMediaFileRow {
    pub(crate) file_name: String,
    pub(crate) media_type: String,
    pub(crate) sub_type: Option<String>,
}

fn persisted_page_number_to_public(number: i64) -> u64 {
    number as u64 + 1
}

pub(crate) fn public_page_number_to_persisted(page_number: u64) -> Option<i64> {
    page_number
        .checked_sub(1)
        .and_then(|value| i64::try_from(value).ok())
}

fn map_persisted_book_page_row(row: SqliteRow) -> BookPageRecord {
    BookPageRecord {
        number: persisted_page_number_to_public(row.get::<i64, _>("NUMBER")),
        file_name: row.get::<String, _>("FILE_NAME"),
        media_type: row.get::<String, _>("MEDIA_TYPE"),
        width: row.get::<Option<i64>, _>("width"),
        height: row.get::<Option<i64>, _>("height"),
        file_size: row.get::<i64, _>("FILE_SIZE"),
    }
}

pub(crate) async fn load_persisted_book_media(
    pool: &SqlitePool,
    book_id: &str,
) -> anyhow::Result<Option<BookMediaRecord>> {
    let row = sqlx::query(
        r#"SELECT b.LIBRARY_ID AS LIBRARY_ID, b.NAME AS FILE_NAME, b.URL AS BOOK_URL,
            l.ROOT AS LIBRARY_ROOT,
            COALESCE(m.MEDIA_TYPE, 'application/octet-stream') AS MEDIA_TYPE,
            COALESCE(m.PAGE_COUNT, 0) AS PAGE_COUNT
         FROM BOOK b
         JOIN LIBRARY l ON l.ID = b.LIBRARY_ID
         LEFT JOIN MEDIA m ON m.BOOK_ID = b.ID
         WHERE b.ID = ?"#,
    )
    .bind(book_id)
    .fetch_optional(pool)
    .await
    .context("query persisted book media")?;

    Ok(row.map(|row| BookMediaRecord {
        library_id: row.get::<String, _>("LIBRARY_ID"),
        media_type: row.get::<String, _>("MEDIA_TYPE"),
        file_path: resolve_library_item_path(
            row.get::<String, _>("LIBRARY_ROOT").as_str(),
            row.get::<String, _>("BOOK_URL").as_str(),
        ),
        file_name: row.get::<String, _>("FILE_NAME"),
        page_count: row.get::<i64, _>("PAGE_COUNT").max(0) as u64,
    }))
}

pub(crate) async fn load_persisted_book_media_files(
    pool: &SqlitePool,
    book_id: &str,
) -> anyhow::Result<Vec<String>> {
    sqlx::query("SELECT FILE_NAME FROM MEDIA_FILE WHERE BOOK_ID = ? ORDER BY rowid ASC")
        .bind(book_id)
        .fetch_all(pool)
        .await
        .context("query persisted book media files")
        .map(|rows| {
            rows.into_iter()
                .map(|row| row.get::<String, _>("FILE_NAME"))
                .collect()
        })
}

pub(crate) async fn load_persisted_media_file_records(
    pool: &SqlitePool,
    book_id: &str,
) -> anyhow::Result<Vec<PersistedMediaFileRow>> {
    sqlx::query(
        r#"SELECT FILE_NAME, COALESCE(MEDIA_TYPE, '') AS MEDIA_TYPE, SUB_TYPE
         FROM MEDIA_FILE WHERE BOOK_ID = ? ORDER BY rowid ASC"#,
    )
    .bind(book_id)
    .fetch_all(pool)
    .await
    .context("query persisted media file records")
    .map(|rows| {
        rows.into_iter()
            .map(|row| {
                let file_name = row.get::<String, _>("FILE_NAME");
                let media_type = row.get::<String, _>("MEDIA_TYPE");
                PersistedMediaFileRow {
                    media_type: content_type_from_filename(&file_name, &media_type),
                    file_name,
                    sub_type: row.get::<Option<String>, _>("SUB_TYPE"),
                }
            })
            .collect()
    })
}

pub(crate) async fn book_media_is_ready_status(
    pool: &SqlitePool,
    book_id: &str,
) -> anyhow::Result<bool> {
    let row = sqlx::query("SELECT STATUS FROM MEDIA WHERE BOOK_ID = ? LIMIT 1")
        .bind(book_id)
        .fetch_optional(pool)
        .await
        .context("query media status")?;

    Ok(row
        .map(|row| row.get::<String, _>("STATUS"))
        .and_then(|status| MediaStatus::parse(&status))
        == Some(MediaStatus::Ready))
}

pub(crate) async fn load_persisted_book_pages(
    pool: &SqlitePool,
    book_id: &str,
) -> anyhow::Result<Vec<BookPageRecord>> {
    let rows = sqlx::query(
        r#"SELECT NUMBER, FILE_NAME, MEDIA_TYPE, width, height,
            CASE WHEN FILE_SIZE IS NULL THEN -1 ELSE FILE_SIZE END AS FILE_SIZE
         FROM MEDIA_PAGE WHERE BOOK_ID = ? ORDER BY NUMBER ASC"#,
    )
    .bind(book_id)
    .fetch_all(pool)
    .await
    .context("query persisted book pages")?;
    Ok(rows.into_iter().map(map_persisted_book_page_row).collect())
}

pub(crate) async fn load_persisted_book_page_row(
    pool: &SqlitePool,
    book_id: &str,
    page_number: u64,
) -> anyhow::Result<Option<BookPageRecord>> {
    let Some(persisted_page_number) = public_page_number_to_persisted(page_number) else {
        return Ok(None);
    };

    let row = sqlx::query(
        r#"SELECT NUMBER, FILE_NAME, MEDIA_TYPE, width, height,
            CASE WHEN FILE_SIZE IS NULL THEN -1 ELSE FILE_SIZE END AS FILE_SIZE
         FROM MEDIA_PAGE WHERE BOOK_ID = ? AND NUMBER = ? LIMIT 1"#,
    )
    .bind(book_id)
    .bind(persisted_page_number)
    .fetch_optional(pool)
    .await
    .context("query single persisted book page")?;
    Ok(row.map(map_persisted_book_page_row))
}

pub(crate) async fn persisted_book_exists(
    pool: &SqlitePool,
    book_id: &str,
) -> anyhow::Result<bool> {
    Ok(
        sqlx::query("SELECT 1 AS FOUND FROM BOOK WHERE ID = ? LIMIT 1")
            .bind(book_id)
            .fetch_optional(pool)
            .await
            .context("query persisted book existence")?
            .is_some(),
    )
}

pub(crate) async fn persisted_series_exists(
    pool: &SqlitePool,
    series_id: &str,
) -> anyhow::Result<bool> {
    Ok(
        sqlx::query("SELECT 1 AS FOUND FROM SERIES WHERE ID = ? LIMIT 1")
            .bind(series_id)
            .fetch_optional(pool)
            .await
            .context("query persisted series existence")?
            .is_some(),
    )
}

pub(crate) async fn load_persisted_series_oneshot(
    pool: &SqlitePool,
    series_id: &str,
) -> anyhow::Result<Option<bool>> {
    let row =
        sqlx::query("SELECT COALESCE(ONESHOT, 0) AS ONESHOT FROM SERIES WHERE ID = ? LIMIT 1")
            .bind(series_id)
            .fetch_optional(pool)
            .await
            .context("query persisted series oneshot")?;
    Ok(row.map(|row| row.get::<i64, _>("ONESHOT") != 0))
}

pub(crate) async fn load_series_book_ids(
    pool: &SqlitePool,
    series_id: &str,
) -> anyhow::Result<Vec<String>> {
    let rows = sqlx::query(
        r#"SELECT b.ID AS ID
         FROM BOOK b
         LEFT JOIN BOOK_METADATA bm ON bm.BOOK_ID = b.ID
         WHERE b.SERIES_ID = ? AND b.DELETED_DATE IS NULL
         ORDER BY COALESCE(bm.NUMBER_SORT, CAST(0 AS REAL)) ASC, b.ID ASC"#,
    )
    .bind(series_id)
    .fetch_all(pool)
    .await
    .context("query series book ids")?;
    Ok(rows
        .into_iter()
        .map(|row| row.get::<String, _>("ID"))
        .collect())
}

pub(crate) async fn load_series_book_number_sorts(
    pool: &SqlitePool,
    series_id: &str,
) -> anyhow::Result<Vec<SeriesBookNumberSort>> {
    let rows = sqlx::query(
        r#"SELECT b.ID AS ID, COALESCE(bm.NUMBER_SORT, CAST(0 AS REAL)) AS NUMBER_SORT
         FROM BOOK b
         LEFT JOIN BOOK_METADATA bm ON bm.BOOK_ID = b.ID
         WHERE b.SERIES_ID = ? AND b.DELETED_DATE IS NULL
         ORDER BY COALESCE(bm.NUMBER_SORT, CAST(0 AS REAL)) ASC, b.ID ASC"#,
    )
    .bind(series_id)
    .fetch_all(pool)
    .await
    .context("query series number sort rows")?;
    Ok(rows
        .into_iter()
        .map(|row| SeriesBookNumberSort {
            book_id: row.get::<String, _>("ID"),
            number_sort: row.get::<f64, _>("NUMBER_SORT"),
        })
        .collect())
}

pub(crate) async fn load_book_restrictions(
    pool: &SqlitePool,
    book_id: &str,
) -> anyhow::Result<Option<BookAccessRestrictions>> {
    let row = sqlx::query(
        r#"SELECT sm.AGE_RATING AS AGE_RATING,
                  COALESCE((SELECT GROUP_CONCAT(LABEL, char(30))
                            FROM (SELECT DISTINCT sms.LABEL AS LABEL
                                  FROM SERIES_METADATA_SHARING sms
                                  WHERE sms.SERIES_ID = s.ID)), '') AS LABELS
         FROM BOOK b
         JOIN SERIES s ON s.ID = b.SERIES_ID
         LEFT JOIN SERIES_METADATA sm ON sm.SERIES_ID = s.ID
         WHERE b.ID = ?
         GROUP BY sm.AGE_RATING"#,
    )
    .bind(book_id)
    .fetch_optional(pool)
    .await
    .context("query book restrictions")?;

    Ok(row.map(|row| {
        let age_rating = row
            .get::<Option<i64>, _>("AGE_RATING")
            .map(clamp_kotlin_int_u32);
        let labels = parse_sqlite_group_concat_values(&row.get::<String, _>("LABELS"));
        BookAccessRestrictions { age_rating, labels }
    }))
}

pub(crate) async fn load_persisted_manifest_book(
    pool: &SqlitePool,
    book_id: &str,
) -> anyhow::Result<Option<ManifestBookRecord>> {
    let row = sqlx::query(
        r#"SELECT b.LIBRARY_ID AS LIBRARY_ID, COALESCE(bm.TITLE, b.NAME) AS TITLE,
            b.NAME AS FILE_NAME,
            COALESCE(m.MEDIA_TYPE, 'application/octet-stream') AS MEDIA_TYPE
         FROM BOOK b
         LEFT JOIN BOOK_METADATA bm ON bm.BOOK_ID = b.ID
         LEFT JOIN MEDIA m ON m.BOOK_ID = b.ID
         WHERE b.ID = ?"#,
    )
    .bind(book_id)
    .fetch_optional(pool)
    .await
    .context("query persisted manifest book")?;

    Ok(row.map(|row| {
        let file_name = row.get::<String, _>("FILE_NAME");
        let media_type = row.get::<String, _>("MEDIA_TYPE");
        ManifestBookRecord {
            library_id: row.get::<String, _>("LIBRARY_ID"),
            title: row.get::<String, _>("TITLE"),
            media_type: content_type_from_filename(&file_name, &media_type),
        }
    }))
}

pub(crate) async fn load_persisted_epub_extension_blob(
    pool: &SqlitePool,
    book_id: &str,
) -> anyhow::Result<Option<EpubExtensionBlob>> {
    let row = sqlx::query(
        "SELECT EXTENSION_CLASS, EXTENSION_VALUE_BLOB FROM MEDIA WHERE BOOK_ID = ? LIMIT 1",
    )
    .bind(book_id)
    .fetch_optional(pool)
    .await
    .context("query epub extension blob")?;

    let Some(row) = row else {
        return Ok(None);
    };
    let extension_class = row
        .get::<Option<String>, _>("EXTENSION_CLASS")
        .unwrap_or_default();
    let Some(blob) = row.get::<Option<Vec<u8>>, _>("EXTENSION_VALUE_BLOB") else {
        return Ok(None);
    };
    Ok(Some(EpubExtensionBlob {
        extension_class,
        bytes: blob,
    }))
}

pub(crate) async fn load_series_archive_entries(
    pool: &SqlitePool,
    series_id: &str,
) -> anyhow::Result<Option<SeriesArchiveEntries>> {
    let series_row = sqlx::query(
        r#"SELECT COALESCE(sm.TITLE, s.NAME) AS SERIES_TITLE
         FROM SERIES s
         LEFT JOIN SERIES_METADATA sm ON sm.SERIES_ID = s.ID
         WHERE s.ID = ?
         LIMIT 1"#,
    )
    .bind(series_id)
    .fetch_optional(pool)
    .await
    .context("query series archive metadata")?;
    let Some(series_row) = series_row else {
        return Ok(None);
    };

    let series_title = series_row.get::<String, _>("SERIES_TITLE");
    let rows = sqlx::query(
        r#"SELECT b.NAME AS FILE_NAME, b.URL AS BOOK_URL, l.ROOT AS LIBRARY_ROOT
         FROM BOOK b
         JOIN LIBRARY l ON l.ID = b.LIBRARY_ID
         WHERE b.SERIES_ID = ? AND b.DELETED_DATE IS NULL
         ORDER BY b.NUMBER ASC, b.ID ASC"#,
    )
    .bind(series_id)
    .fetch_all(pool)
    .await
    .context("query series archive entries")?;

    let entries = rows
        .into_iter()
        .map(|row| {
            let file_name = row.get::<String, _>("FILE_NAME");
            let book_url = row.get::<String, _>("BOOK_URL");
            let library_root = row.get::<String, _>("LIBRARY_ROOT");
            ArchiveEntry {
                file_name,
                file_path: resolve_library_item_path(library_root.as_str(), book_url.as_str()),
            }
        })
        .collect::<Vec<_>>();
    Ok(Some(SeriesArchiveEntries {
        series_title,
        entries,
    }))
}

#[cfg(test)]
mod tests {
    use super::{
        load_persisted_book_media, load_persisted_book_pages, load_series_book_number_sorts,
    };
    use crate::test_support::BootstrappedBookFixture;

    #[tokio::test]
    async fn media_access_queries_preserve_runtime_defaults_and_page_dimensions() {
        let fixture = BootstrappedBookFixture::open("media-access-defaults").await;
        fixture.insert_library_series().await;

        fixture.insert_book("book-without-media").await;
        let media = load_persisted_book_media(&fixture.pool, "book-without-media")
            .await
            .expect("book without media row should load")
            .expect("book should exist");
        assert_eq!(media.media_type, "application/octet-stream");
        assert_eq!(media.page_count, 0);

        fixture.insert_book("book-valid-page").await;
        fixture
            .insert_media_page("book-valid-page", 0, "page.jpg", "image/jpeg", Some(42))
            .await;
        let pages = load_persisted_book_pages(&fixture.pool, "book-valid-page")
            .await
            .expect("load persisted book pages");
        assert_eq!(pages.len(), 1);
        assert_eq!(pages[0].number, 1);
        assert_eq!(pages[0].width, Some(640));
        assert_eq!(pages[0].height, Some(480));
        assert_eq!(pages[0].file_size, 42);

        fixture.insert_book("book-null-page-file-size").await;
        fixture
            .insert_media_page(
                "book-null-page-file-size",
                0,
                "page.jpg",
                "image/jpeg",
                None,
            )
            .await;
        let pages = load_persisted_book_pages(&fixture.pool, "book-null-page-file-size")
            .await
            .expect("null persisted page file size should load");
        assert_eq!(pages[0].number, 1);
        assert_eq!(pages[0].file_size, -1);

        let sorts = load_series_book_number_sorts(&fixture.pool, "series-1")
            .await
            .expect("number sort rows should load");
        assert_eq!(sorts.len(), 3);
        assert!(sorts.iter().all(|row| row.number_sort == 0.0));

        fixture.close().await;
    }
}
