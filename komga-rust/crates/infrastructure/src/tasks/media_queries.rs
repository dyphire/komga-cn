use std::collections::HashMap;
use std::path::{Path, PathBuf};

use sqlx::{Row, SqlitePool};

use crate::sqlite::connect_pool;

#[derive(Clone, Debug)]
pub struct PersistedLibraryHashingFlags {
    pub hash_files: bool,
    pub hash_pages: bool,
    pub hash_koreader: bool,
}

#[derive(Clone, Debug)]
pub struct PersistedLibraryMaintenanceFlags {
    pub repair_extensions: bool,
    pub convert_to_cbz: bool,
}

pub struct PersistedBookHashRuntimeState {
    pub library_id: String,
    pub file_hash: Option<String>,
    pub file_hash_koreader: Option<String>,
}

#[derive(Clone, Debug)]
pub struct PersistedBookArchiveSource {
    pub file_path: PathBuf,
    pub series_id: String,
    pub file_last_modified: i64,
    pub media_type: String,
    pub media_status: String,
}

#[derive(Clone, Debug)]
pub struct PersistedExtensionRepairTarget {
    pub book_id: String,
    pub series_id: String,
    pub library_id: String,
    pub book_url: String,
    pub library_root: String,
    pub media_type: String,
}

#[derive(Clone, Debug)]
pub struct PersistedConversionTarget {
    pub book_url: String,
    pub series_id: String,
    pub library_id: String,
    pub library_root: String,
    pub file_last_modified: i64,
    pub convert_to_cbz: bool,
    pub media_type: String,
    pub media_status: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PersistedBookToConvert {
    pub book_id: String,
    pub series_id: String,
}

fn expected_extension_for_extension_repair(media_type: &str) -> Option<&'static str> {
    match media_type {
        "application/x-rar-compressed; version=4" | "application/x-rar-compressed; version=5" => {
            Some("cbr")
        }
        "application/zip" => Some("cbz"),
        "application/pdf" => Some("pdf"),
        "application/epub+zip" => Some("epub"),
        _ => None,
    }
}

#[derive(Clone, Debug)]
pub struct PersistedHashedPageToDelete {
    pub file_hash: String,
    pub file_size: i64,
    pub file_name: String,
    pub media_type: String,
    pub page_number: i64,
}

pub fn load_book_hashed_pages(
    database_file: &Path,
    book_id: &str,
) -> Result<Vec<PersistedHashedPageToDelete>, String> {
    let database_file = database_file.to_path_buf();
    let book_id = book_id.to_string();

    run_database_query(database_file, move |pool| {
        let book_id = book_id.clone();
        Box::pin(async move {
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
            .bind(&book_id)
            .fetch_all(&pool)
            .await
            .map_err(|error| format!("failed to load hashed pages for '{book_id}': {error}"))?;

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
        })
    })
}

pub fn load_sidecar_url_for_parent(
    database_file: &Path,
    parent_url: &str,
    metadata_only: bool,
) -> Result<Option<String>, String> {
    let database_file = database_file.to_path_buf();
    let parent_url = parent_url.to_string();

    run_database_query(database_file, move |pool| {
        let parent_url = parent_url.clone();
        Box::pin(async move {
            let sql = if metadata_only {
                r#"
                SELECT URL
                FROM SIDECAR
                WHERE PARENT_URL = ?
                AND LOWER(URL) LIKE '%.xml'
                ORDER BY LAST_MODIFIED_TIME DESC
                LIMIT 1
                "#
            } else {
                r#"
                SELECT URL
                FROM SIDECAR
                WHERE PARENT_URL = ?
                AND (
                    LOWER(URL) LIKE '%.jpg'
                    OR LOWER(URL) LIKE '%.jpeg'
                    OR LOWER(URL) LIKE '%.png'
                    OR LOWER(URL) LIKE '%.webp'
                    OR LOWER(URL) LIKE '%.gif'
                    OR LOWER(URL) LIKE '%.avif'
                )
                ORDER BY LAST_MODIFIED_TIME DESC
                LIMIT 1
                "#
            };

            let row = sqlx::query(sql)
                .bind(&parent_url)
                .fetch_optional(&pool)
                .await
                .map_err(|error| format!("failed to load sidecar for '{parent_url}': {error}"))?;

            Ok(row.map(|row| row.get::<String, _>("URL")))
        })
    })
}

pub fn load_library_hashing_flags(
    database_file: &Path,
    library_id: &str,
) -> Result<PersistedLibraryHashingFlags, String> {
    let database_file = database_file.to_path_buf();
    let library_id = library_id.to_string();

    run_database_query(database_file, move |pool| {
        let library_id = library_id.clone();
        Box::pin(async move {
            let row = sqlx::query(
                r#"
                SELECT
                COALESCE(HASH_FILES, 0) AS HASH_FILES,
                COALESCE(HASH_PAGES, 0) AS HASH_PAGES,
                COALESCE(HASH_KOREADER, 0) AS HASH_KOREADER
                FROM LIBRARY
                WHERE ID = ?
                LIMIT 1
                "#,
            )
            .bind(&library_id)
            .fetch_optional(&pool)
            .await
            .map_err(|error| {
                format!("failed to load library hashing flags for '{library_id}': {error}")
            })?;

            Ok(row.map_or(
                PersistedLibraryHashingFlags {
                    hash_files: false,
                    hash_pages: false,
                    hash_koreader: false,
                },
                |row| PersistedLibraryHashingFlags {
                    hash_files: row.get::<i64, _>("HASH_FILES") != 0,
                    hash_pages: row.get::<i64, _>("HASH_PAGES") != 0,
                    hash_koreader: row.get::<i64, _>("HASH_KOREADER") != 0,
                },
            ))
        })
    })
}

pub fn load_library_maintenance_flags(
    database_file: &Path,
    library_id: &str,
) -> Result<PersistedLibraryMaintenanceFlags, String> {
    let database_file = database_file.to_path_buf();
    let library_id = library_id.to_string();

    run_database_query(database_file, move |pool| {
        let library_id = library_id.clone();
        Box::pin(async move {
            let row = sqlx::query(
                r#"
                SELECT
                COALESCE(REPAIR_EXTENSIONS, 0) AS REPAIR_EXTENSIONS,
                COALESCE(CONVERT_TO_CBZ, 0) AS CONVERT_TO_CBZ
                FROM LIBRARY
                WHERE ID = ?
                LIMIT 1
                "#,
            )
            .bind(&library_id)
            .fetch_optional(&pool)
            .await
            .map_err(|error| {
                format!("failed to load library maintenance flags for '{library_id}': {error}")
            })?;

            Ok(row.map_or(
                PersistedLibraryMaintenanceFlags {
                    repair_extensions: false,
                    convert_to_cbz: false,
                },
                |row| PersistedLibraryMaintenanceFlags {
                    repair_extensions: row.get::<i64, _>("REPAIR_EXTENSIONS") != 0,
                    convert_to_cbz: row.get::<i64, _>("CONVERT_TO_CBZ") != 0,
                },
            ))
        })
    })
}

pub fn load_book_library_id(database_file: &Path, book_id: &str) -> Result<Option<String>, String> {
    let database_file = database_file.to_path_buf();
    let book_id = book_id.to_string();

    run_database_query(database_file, move |pool| {
        let book_id = book_id.clone();
        Box::pin(async move {
            let row = sqlx::query(
                r#"
                SELECT LIBRARY_ID
                FROM BOOK
                WHERE ID = ?
                LIMIT 1
                "#,
            )
            .bind(&book_id)
            .fetch_optional(&pool)
            .await
            .map_err(|error| format!("failed to load book library for '{book_id}': {error}"))?;

            Ok(row.map(|row| row.get::<String, _>("LIBRARY_ID")))
        })
    })
}

pub fn load_book_hash_runtime_state(
    database_file: &Path,
    book_id: &str,
) -> Result<Option<PersistedBookHashRuntimeState>, String> {
    let database_file = database_file.to_path_buf();
    let book_id = book_id.to_string();

    run_database_query(database_file, move |pool| {
        let book_id = book_id.clone();
        Box::pin(async move {
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
            .bind(&book_id)
            .fetch_optional(&pool)
            .await
            .map_err(|error| {
                format!("failed to load book hash runtime state for '{book_id}': {error}")
            })?;

            Ok(row.map(|row| PersistedBookHashRuntimeState {
                library_id: row.get::<String, _>("LIBRARY_ID"),
                file_hash: row.get::<Option<String>, _>("FILE_HASH"),
                file_hash_koreader: row.get::<Option<String>, _>("FILE_HASH_KOREADER"),
            }))
        })
    })
}

pub fn load_book_file_path(database_file: &Path, book_id: &str) -> Result<Option<PathBuf>, String> {
    let database_file = database_file.to_path_buf();
    let book_id = book_id.to_string();

    run_database_query(database_file, move |pool| {
        let book_id = book_id.clone();
        Box::pin(async move {
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
            .bind(&book_id)
            .fetch_optional(&pool)
            .await
            .map_err(|error| {
                format!("failed to query book file for hash task '{book_id}': {error}")
            })?;

            Ok(row.map(|row| {
                PathBuf::from(row.get::<String, _>("ROOT")).join(row.get::<String, _>("URL"))
            }))
        })
    })
}

pub fn load_non_deleted_book_ids(database_file: &Path) -> Result<Vec<String>, String> {
    let database_file = database_file.to_path_buf();

    run_database_query(database_file, move |pool| {
        Box::pin(async move {
            let rows = sqlx::query(
                r#"
                SELECT b.ID
                FROM BOOK b
                WHERE b.DELETED_DATE IS NULL
                ORDER BY b.ID ASC
                "#,
            )
            .fetch_all(&pool)
            .await
            .map_err(|error| {
                format!("failed to query non-deleted books for thumbnail regeneration: {error}")
            })?;

            Ok(rows
                .into_iter()
                .map(|row| row.get::<String, _>("ID"))
                .collect())
        })
    })
}

pub fn load_books_with_undersized_generated_thumbnails(
    database_file: &Path,
    max_edge: i64,
) -> Result<Vec<String>, String> {
    let database_file = database_file.to_path_buf();

    run_database_query(database_file, move |pool| {
        Box::pin(async move {
            let rows = sqlx::query(
                r#"
                SELECT DISTINCT BOOK_ID
                FROM THUMBNAIL_BOOK
                WHERE TYPE = 'GENERATED'
                AND WIDTH < ?
                AND HEIGHT < ?
                "#,
            )
            .bind(max_edge)
            .bind(max_edge)
            .fetch_all(&pool)
            .await
            .map_err(|error| {
                format!("failed to query books with undersized generated thumbnails: {error}")
            })?;

            Ok(rows
                .into_iter()
                .map(|row| row.get::<String, _>("BOOK_ID"))
                .collect())
        })
    })
}

pub fn load_books_with_missing_page_hash(
    database_file: &Path,
    library_id: Option<&str>,
) -> Result<Vec<String>, String> {
    let database_file = database_file.to_path_buf();
    let library_id = library_id.map(str::to_string);

    run_database_query(database_file, move |pool| {
        Box::pin(async move {
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
                .fetch_all(&pool)
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
                .fetch_all(&pool)
                .await
            }
            .map_err(|error| format!("failed to query books with missing page hashes: {error}"))?;

            Ok(rows
                .into_iter()
                .map(|row| row.get::<String, _>("BOOK_ID"))
                .collect())
        })
    })
}

pub fn load_duplicate_pages_to_delete(
    database_file: &Path,
    library_id: &str,
) -> Result<HashMap<String, Vec<PersistedHashedPageToDelete>>, String> {
    let database_file = database_file.to_path_buf();
    let library_id = library_id.to_string();

    run_database_query(database_file, move |pool| {
        let library_id = library_id.clone();
        Box::pin(async move {
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
            .fetch_all(&pool)
            .await
            .map_err(|error| {
                format!("failed to query duplicate pages to delete for '{library_id}': {error}")
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
        })
    })
}

pub fn load_books_requiring_analysis(
    database_file: &Path,
    book_ids: &[String],
) -> Result<Vec<String>, String> {
    if book_ids.is_empty() {
        return Ok(Vec::new());
    }

    let database_file = database_file.to_path_buf();
    let book_ids = book_ids.to_vec();

    run_database_query(database_file, move |pool| {
        let book_ids = book_ids.clone();
        Box::pin(async move {
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
                .bind(&book_id)
                .fetch_optional(&pool)
                .await
                .map_err(|error| format!("failed to query media status for '{book_id}': {error}"))?
                .map(|row| row.get::<String, _>("STATUS"));

                let needs_analysis = match status.as_deref() {
                    None => true,
                    Some(status) => {
                        status.eq_ignore_ascii_case("UNKNOWN")
                            || status.eq_ignore_ascii_case("OUTDATED")
                    }
                };

                if needs_analysis {
                    result.push(book_id);
                }
            }

            Ok(result)
        })
    })
}

pub fn load_books_with_missing_file_hash(
    database_file: &Path,
    library_id: &str,
    koreader: bool,
) -> Result<Vec<String>, String> {
    let database_file = database_file.to_path_buf();
    let library_id = library_id.to_string();

    run_database_query(database_file, move |pool| {
        let library_id = library_id.clone();
        Box::pin(async move {
            let sql = if koreader {
                r#"
                SELECT ID
                FROM BOOK
                WHERE LIBRARY_ID = ?
                AND DELETED_DATE IS NULL
                AND (FILE_HASH_KOREADER = '' OR FILE_HASH_KOREADER IS NULL)
                "#
            } else {
                r#"
                SELECT ID
                FROM BOOK
                WHERE LIBRARY_ID = ?
                AND DELETED_DATE IS NULL
                AND (FILE_HASH = '' OR FILE_HASH IS NULL)
                "#
            };

            let rows = sqlx::query(sql)
                .bind(&library_id)
                .fetch_all(&pool)
                .await
                .map_err(|error| {
                    format!(
                        "failed to query books with missing file hash for '{library_id}': {error}"
                    )
                })?;

            Ok(rows
                .into_iter()
                .map(|row| row.get::<String, _>("ID"))
                .collect())
        })
    })
}

pub fn load_books_to_convert(
    database_file: &Path,
    library_id: &str,
) -> Result<Vec<PersistedBookToConvert>, String> {
    let database_file = database_file.to_path_buf();
    let library_id = library_id.to_string();

    run_database_query(database_file, move |pool| {
        let library_id = library_id.clone();
        Box::pin(async move {
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
            .bind(&library_id)
            .fetch_all(&pool)
            .await
            .map_err(|error| {
                format!("failed to query books to convert for '{library_id}': {error}")
            })?;

            Ok(rows
                .into_iter()
                .map(|row| PersistedBookToConvert {
                    book_id: row.get::<String, _>("ID"),
                    series_id: row.get::<String, _>("SERIES_ID"),
                })
                .collect())
        })
    })
}

pub fn load_book_conversion_target(
    database_file: &Path,
    book_id: &str,
) -> Result<Option<PersistedConversionTarget>, String> {
    let database_file = database_file.to_path_buf();
    let book_id = book_id.to_string();

    run_database_query(database_file, move |pool| {
        let book_id = book_id.clone();
        Box::pin(async move {
            let row = sqlx::query(
                r#"
                SELECT
                 b.URL AS BOOK_URL,
                 b.SERIES_ID AS SERIES_ID,
                 b.LIBRARY_ID AS LIBRARY_ID,
                 l.ROOT AS LIBRARY_ROOT,
                 b.FILE_LAST_MODIFIED AS FILE_LAST_MODIFIED,
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
            .bind(&book_id)
            .fetch_optional(&pool)
            .await
            .map_err(|error| {
                format!("failed to load convert-book source row for '{book_id}': {error}")
            })?;

            Ok(row.map(|row| PersistedConversionTarget {
                book_url: row.get::<String, _>("BOOK_URL"),
                series_id: row.get::<String, _>("SERIES_ID"),
                library_id: row.get::<String, _>("LIBRARY_ID"),
                library_root: row.get::<String, _>("LIBRARY_ROOT"),
                file_last_modified: row.get::<i64, _>("FILE_LAST_MODIFIED"),
                convert_to_cbz: row.get::<i64, _>("CONVERT_TO_CBZ") != 0,
                media_type: row.get::<String, _>("MEDIA_TYPE"),
                media_status: row.get::<String, _>("MEDIA_STATUS"),
            }))
        })
    })
}

pub fn load_books_for_extension_repair(
    database_file: &Path,
    library_id: &str,
) -> Result<Vec<PersistedExtensionRepairTarget>, String> {
    let database_file = database_file.to_path_buf();
    let library_id = library_id.to_string();

    run_database_query(database_file, move |pool| {
        let library_id = library_id.clone();
        Box::pin(async move {
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
            .bind(&library_id)
            .fetch_all(&pool)
            .await
            .map_err(|error| {
                format!("failed to query books for extension repair in '{library_id}': {error}")
            })?;

            Ok(rows
                .into_iter()
                .filter_map(|row| {
                    let media_type = row.get::<String, _>("MEDIA_TYPE");
                    let expected_extension = expected_extension_for_extension_repair(&media_type)?;
                    let book_url = row.get::<String, _>("BOOK_URL");
                    let current_extension = PathBuf::from(&book_url)
                        .extension()
                        .and_then(|value| value.to_str())
                        .map(|value| value.to_ascii_lowercase())
                        .unwrap_or_default();
                    (current_extension != expected_extension).then(|| {
                        PersistedExtensionRepairTarget {
                            book_id: row.get::<String, _>("BOOK_ID"),
                            series_id: row.get::<String, _>("SERIES_ID"),
                            library_id: row.get::<String, _>("LIBRARY_ID"),
                            book_url,
                            library_root: row.get::<String, _>("LIBRARY_ROOT"),
                            media_type,
                        }
                    })
                })
                .collect())
        })
    })
}

pub fn load_book_for_extension_repair(
    database_file: &Path,
    book_id: &str,
) -> Result<Option<PersistedExtensionRepairTarget>, String> {
    let database_file = database_file.to_path_buf();
    let book_id = book_id.to_string();

    run_database_query(database_file, move |pool| {
        let book_id = book_id.clone();
        Box::pin(async move {
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
            .bind(&book_id)
            .fetch_optional(&pool)
            .await
            .map_err(|error| {
                format!("failed to load repair-extension source row for '{book_id}': {error}")
            })?;

            Ok(row.map(|row| PersistedExtensionRepairTarget {
                book_id: row.get::<String, _>("BOOK_ID"),
                series_id: row.get::<String, _>("SERIES_ID"),
                library_id: row.get::<String, _>("LIBRARY_ID"),
                book_url: row.get::<String, _>("BOOK_URL"),
                library_root: row.get::<String, _>("LIBRARY_ROOT"),
                media_type: row.get::<String, _>("MEDIA_TYPE"),
            }))
        })
    })
}

pub fn load_book_archive_source(
    database_file: &Path,
    book_id: &str,
) -> Result<Option<PersistedBookArchiveSource>, String> {
    let database_file = database_file.to_path_buf();
    let book_id = book_id.to_string();

    run_database_query(database_file, move |pool| {
        let book_id = book_id.clone();
        Box::pin(async move {
            let row = sqlx::query(
                r#"
                SELECT
                b.URL AS BOOK_URL,
                b.SERIES_ID AS SERIES_ID,
                b.FILE_LAST_MODIFIED AS FILE_LAST_MODIFIED,
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
            .bind(&book_id)
            .fetch_optional(&pool)
            .await
            .map_err(|error| format!("failed to load archive source for '{book_id}': {error}"))?;

            Ok(row.map(|row| PersistedBookArchiveSource {
                file_path: PathBuf::from(row.get::<String, _>("LIBRARY_ROOT"))
                    .join(row.get::<String, _>("BOOK_URL")),
                series_id: row.get::<String, _>("SERIES_ID"),
                file_last_modified: row.get::<i64, _>("FILE_LAST_MODIFIED"),
                media_type: row.get::<String, _>("MEDIA_TYPE"),
                media_status: row.get::<String, _>("MEDIA_STATUS"),
            }))
        })
    })
}

type BoxFuture<T> = std::pin::Pin<Box<dyn std::future::Future<Output = Result<T, String>> + Send>>;

fn run_database_query<T>(
    database_file: PathBuf,
    operation: impl FnOnce(SqlitePool) -> BoxFuture<T> + Send + 'static,
) -> Result<T, String>
where
    T: Send + 'static,
{
    std::thread::spawn(move || {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|error| format!("failed to build task runtime: {error}"))?;

        runtime.block_on(async move {
            let pool = connect_pool(&database_file, 1)
                .await
                .map_err(|error| format!("failed to open sqlite pool: {error}"))?;
            operation(pool).await
        })
    })
    .join()
    .map_err(|_| "database operation worker thread panicked".to_string())?
}
