use sqlx::{Row, Sqlite, SqlitePool, Transaction};

use crate::persisted_paths::resolve_stored_path;
use crate::sql::content_libraries::DELETE_LIBRARY_DEPENDENCY_SQL;

#[derive(Clone, Debug)]
pub struct PersistedLibraryWriteModel {
    pub id: String,
    pub name: String,
    pub root: String,
    pub import_comicinfo_book: bool,
    pub import_comicinfo_series: bool,
    pub import_comicinfo_collection: bool,
    pub import_comicinfo_readlist: bool,
    pub import_comicinfo_series_append_volume: bool,
    pub import_epub_book: bool,
    pub import_epub_series: bool,
    pub import_mylar_series: bool,
    pub import_local_artwork: bool,
    pub import_barcode_isbn: bool,
    pub scan_force_modified_time: bool,
    pub scan_interval: String,
    pub scan_on_startup: bool,
    pub scan_cbx: bool,
    pub scan_pdf: bool,
    pub scan_epub: bool,
    pub scan_directory_exclusions: Vec<String>,
    pub repair_extensions: bool,
    pub convert_to_cbz: bool,
    pub empty_trash_after_scan: bool,
    pub series_cover: String,
    pub hash_files: bool,
    pub hash_pages: bool,
    pub hash_koreader: bool,
    pub analyze_dimensions: bool,
    pub oneshots_directory: Option<String>,
    pub unavailable: bool,
}

pub async fn persist_library_create(
    pool: &SqlitePool,
    library: &PersistedLibraryWriteModel,
) -> Result<(), sqlx::Error> {
    let mut tx = pool.begin().await?;
    insert_library_row(&mut tx, library).await?;
    replace_library_exclusions(&mut tx, &library.id, &library.scan_directory_exclusions).await?;
    tx.commit().await?;
    Ok(())
}

pub async fn persist_library_update(
    pool: &SqlitePool,
    library: &PersistedLibraryWriteModel,
) -> Result<bool, sqlx::Error> {
    let mut tx = pool.begin().await?;
    let updated = update_library_row(&mut tx, library).await?;
    if !updated {
        tx.rollback().await?;
        return Ok(false);
    }
    replace_library_exclusions(&mut tx, &library.id, &library.scan_directory_exclusions).await?;
    tx.commit().await?;
    Ok(true)
}

pub async fn delete_persisted_library(
    pool: &SqlitePool,
    library_id: &str,
) -> Result<bool, sqlx::Error> {
    let mut tx = pool.begin().await?;
    let exists = sqlx::query(
        r#"SELECT 1
           FROM LIBRARY
           WHERE ID = ?
           LIMIT 1"#,
    )
    .bind(library_id)
    .fetch_optional(&mut *tx)
    .await?
    .is_some();
    if !exists {
        tx.rollback().await?;
        return Ok(false);
    }

    for sql in DELETE_LIBRARY_DEPENDENCY_SQL {
        sqlx::query(sql).bind(library_id).execute(&mut *tx).await?;
    }

    let deleted = sqlx::query(r#"DELETE FROM LIBRARY WHERE ID = ?"#)
        .bind(library_id)
        .execute(&mut *tx)
        .await?
        .rows_affected()
        > 0;
    if !deleted {
        tx.rollback().await?;
        return Ok(false);
    }
    tx.commit().await?;
    Ok(true)
}

pub async fn load_persisted_library_write_model(
    pool: &SqlitePool,
    library_id: &str,
) -> Result<Option<PersistedLibraryWriteModel>, sqlx::Error> {
    let row = sqlx::query(
        r#"SELECT ID,
               NAME,
               ROOT,
               IMPORT_COMICINFO_BOOK,
               IMPORT_COMICINFO_SERIES,
               IMPORT_COMICINFO_COLLECTION,
               IMPORT_COMICINFO_READLIST,
               IMPORT_COMICINFO_SERIES_APPEND_VOLUME,
               IMPORT_EPUB_BOOK,
               IMPORT_EPUB_SERIES,
               IMPORT_MYLAR_SERIES,
               IMPORT_LOCAL_ARTWORK,
               IMPORT_BARCODE_ISBN,
               SCAN_FORCE_MODIFIED_TIME,
               SCAN_INTERVAL,
               SCAN_STARTUP,
               SCAN_CBX,
               SCAN_PDF,
               SCAN_EPUB,
               REPAIR_EXTENSIONS,
               CONVERT_TO_CBZ,
               EMPTY_TRASH_AFTER_SCAN,
               SERIES_COVER,
               HASH_FILES,
               HASH_PAGES,
               HASH_KOREADER,
               ANALYZE_DIMENSIONS,
               ONESHOTS_DIRECTORY,
               UNAVAILABLE_DATE
           FROM LIBRARY
           WHERE ID = ?"#,
    )
    .bind(library_id)
    .fetch_optional(pool)
    .await?;

    let Some(row) = row else {
        return Ok(None);
    };

    let mut library = map_persisted_library_row(row);
    let exclusions = sqlx::query(
        r#"SELECT EXCLUSION
           FROM LIBRARY_EXCLUSIONS
           WHERE LIBRARY_ID = ?
           ORDER BY EXCLUSION COLLATE NOCASE ASC"#,
    )
    .bind(library_id)
    .fetch_all(pool)
    .await?;
    library.scan_directory_exclusions = exclusions
        .into_iter()
        .map(|row| row.get::<String, _>("EXCLUSION"))
        .collect();

    Ok(Some(library))
}

pub async fn validate_library_before_persist(
    pool: &SqlitePool,
    library: &PersistedLibraryWriteModel,
) -> Result<(), String> {
    let root_path = resolve_stored_path(&library.root);
    if !root_path.exists() {
        return Err("library root does not exist".to_string());
    }
    if !root_path.is_dir() {
        return Err("library root must be a directory".to_string());
    }

    let rows = sqlx::query(r#"SELECT ID, NAME, ROOT FROM LIBRARY"#)
        .fetch_all(pool)
        .await
        .map_err(|error| format!("query library validation rows: {error}"))?;

    let normalized_root = normalize_library_root(&library.root);
    for row in rows {
        let existing_id = row.get::<String, _>("ID");
        if existing_id == library.id {
            continue;
        }

        let existing_name = row.get::<String, _>("NAME");
        if existing_name == library.name {
            return Err("library name must be unique".to_string());
        }

        let normalized_existing = normalize_library_root(&row.get::<String, _>("ROOT"));
        if normalized_root == normalized_existing
            || normalized_root.starts_with(&(normalized_existing.clone() + "/"))
            || normalized_existing.starts_with(&(normalized_root.clone() + "/"))
        {
            return Err("library root cannot overlap another library root".to_string());
        }
    }

    Ok(())
}

pub async fn library_book_ids_with_empty_hash(
    pool: &SqlitePool,
    library_id: &str,
    koreader: bool,
) -> Result<Vec<String>, String> {
    let sql = if koreader {
        r#"SELECT ID
           FROM BOOK
           WHERE LIBRARY_ID = ?
           AND DELETED_DATE IS NULL
           AND (FILE_HASH_KOREADER = '' OR FILE_HASH_KOREADER IS NULL)"#
    } else {
        r#"SELECT ID
           FROM BOOK
           WHERE LIBRARY_ID = ?
           AND DELETED_DATE IS NULL
           AND (FILE_HASH = '' OR FILE_HASH IS NULL)"#
    };

    let rows = sqlx::query(sql)
        .bind(library_id)
        .fetch_all(pool)
        .await
        .map_err(|error| format!("query books with empty hash: {error}"))?;

    Ok(rows
        .into_iter()
        .map(|row| row.get::<String, _>("ID"))
        .collect::<Vec<_>>())
}

pub async fn library_book_ids(
    pool: &SqlitePool,
    library_id: &str,
) -> Result<Option<Vec<String>>, sqlx::Error> {
    let Some(_) = load_persisted_library_write_model(pool, library_id).await? else {
        return Ok(None);
    };

    let rows = sqlx::query(
        r#"SELECT ID
           FROM BOOK
           WHERE LIBRARY_ID = ?
           ORDER BY ID ASC"#,
    )
    .bind(library_id)
    .fetch_all(pool)
    .await?;

    Ok(Some(
        rows.into_iter()
            .map(|row| row.get::<String, _>("ID"))
            .collect(),
    ))
}

pub async fn library_series_and_book_ids(
    pool: &SqlitePool,
    library_id: &str,
) -> Result<Option<(Vec<String>, Vec<(String, String)>)>, sqlx::Error> {
    let Some(_) = load_persisted_library_write_model(pool, library_id).await? else {
        return Ok(None);
    };

    let series_rows = sqlx::query(
        r#"SELECT ID
           FROM SERIES
           WHERE LIBRARY_ID = ?
           ORDER BY ID ASC"#,
    )
    .bind(library_id)
    .fetch_all(pool)
    .await?;
    let book_rows = sqlx::query(
        r#"SELECT ID, SERIES_ID
           FROM BOOK
           WHERE LIBRARY_ID = ?
           ORDER BY ID ASC"#,
    )
    .bind(library_id)
    .fetch_all(pool)
    .await?;

    Ok(Some((
        series_rows
            .into_iter()
            .map(|row| row.get::<String, _>("ID"))
            .collect(),
        book_rows
            .into_iter()
            .map(|row| {
                (
                    row.get::<String, _>("ID"),
                    row.get::<String, _>("SERIES_ID"),
                )
            })
            .collect(),
    )))
}

async fn insert_library_row(
    tx: &mut Transaction<'_, Sqlite>,
    library: &PersistedLibraryWriteModel,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"INSERT INTO LIBRARY (
            ID,
            NAME,
            ROOT,
            IMPORT_COMICINFO_BOOK,
            IMPORT_COMICINFO_SERIES,
            IMPORT_COMICINFO_COLLECTION,
            IMPORT_COMICINFO_READLIST,
            IMPORT_COMICINFO_SERIES_APPEND_VOLUME,
            IMPORT_EPUB_BOOK,
            IMPORT_EPUB_SERIES,
            IMPORT_MYLAR_SERIES,
            IMPORT_LOCAL_ARTWORK,
            IMPORT_BARCODE_ISBN,
            SCAN_FORCE_MODIFIED_TIME,
            SCAN_INTERVAL,
            SCAN_STARTUP,
            SCAN_CBX,
            SCAN_PDF,
            SCAN_EPUB,
            REPAIR_EXTENSIONS,
            CONVERT_TO_CBZ,
            EMPTY_TRASH_AFTER_SCAN,
            SERIES_COVER,
            HASH_FILES,
            HASH_PAGES,
            HASH_KOREADER,
            ANALYZE_DIMENSIONS,
            ONESHOTS_DIRECTORY
        )
        VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"#,
    )
    .bind(&library.id)
    .bind(&library.name)
    .bind(&library.root)
    .bind(library.import_comicinfo_book)
    .bind(library.import_comicinfo_series)
    .bind(library.import_comicinfo_collection)
    .bind(library.import_comicinfo_readlist)
    .bind(library.import_comicinfo_series_append_volume)
    .bind(library.import_epub_book)
    .bind(library.import_epub_series)
    .bind(library.import_mylar_series)
    .bind(library.import_local_artwork)
    .bind(library.import_barcode_isbn)
    .bind(library.scan_force_modified_time)
    .bind(&library.scan_interval)
    .bind(library.scan_on_startup)
    .bind(library.scan_cbx)
    .bind(library.scan_pdf)
    .bind(library.scan_epub)
    .bind(library.repair_extensions)
    .bind(library.convert_to_cbz)
    .bind(library.empty_trash_after_scan)
    .bind(&library.series_cover)
    .bind(library.hash_files)
    .bind(library.hash_pages)
    .bind(library.hash_koreader)
    .bind(library.analyze_dimensions)
    .bind(&library.oneshots_directory)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

async fn update_library_row(
    tx: &mut Transaction<'_, Sqlite>,
    library: &PersistedLibraryWriteModel,
) -> Result<bool, sqlx::Error> {
    let updated = sqlx::query(
        r#"UPDATE LIBRARY
           SET LAST_MODIFIED_DATE = CURRENT_TIMESTAMP,
               NAME = ?,
               ROOT = ?,
               IMPORT_COMICINFO_BOOK = ?,
               IMPORT_COMICINFO_SERIES = ?,
               IMPORT_COMICINFO_COLLECTION = ?,
               IMPORT_COMICINFO_READLIST = ?,
               IMPORT_COMICINFO_SERIES_APPEND_VOLUME = ?,
               IMPORT_EPUB_BOOK = ?,
               IMPORT_EPUB_SERIES = ?,
               IMPORT_MYLAR_SERIES = ?,
               IMPORT_LOCAL_ARTWORK = ?,
               IMPORT_BARCODE_ISBN = ?,
               SCAN_FORCE_MODIFIED_TIME = ?,
               SCAN_INTERVAL = ?,
               SCAN_STARTUP = ?,
               SCAN_CBX = ?,
               SCAN_PDF = ?,
               SCAN_EPUB = ?,
               REPAIR_EXTENSIONS = ?,
               CONVERT_TO_CBZ = ?,
               EMPTY_TRASH_AFTER_SCAN = ?,
               SERIES_COVER = ?,
               HASH_FILES = ?,
               HASH_PAGES = ?,
               HASH_KOREADER = ?,
               ANALYZE_DIMENSIONS = ?,
               ONESHOTS_DIRECTORY = ?
           WHERE ID = ?"#,
    )
    .bind(&library.name)
    .bind(&library.root)
    .bind(library.import_comicinfo_book)
    .bind(library.import_comicinfo_series)
    .bind(library.import_comicinfo_collection)
    .bind(library.import_comicinfo_readlist)
    .bind(library.import_comicinfo_series_append_volume)
    .bind(library.import_epub_book)
    .bind(library.import_epub_series)
    .bind(library.import_mylar_series)
    .bind(library.import_local_artwork)
    .bind(library.import_barcode_isbn)
    .bind(library.scan_force_modified_time)
    .bind(&library.scan_interval)
    .bind(library.scan_on_startup)
    .bind(library.scan_cbx)
    .bind(library.scan_pdf)
    .bind(library.scan_epub)
    .bind(library.repair_extensions)
    .bind(library.convert_to_cbz)
    .bind(library.empty_trash_after_scan)
    .bind(&library.series_cover)
    .bind(library.hash_files)
    .bind(library.hash_pages)
    .bind(library.hash_koreader)
    .bind(library.analyze_dimensions)
    .bind(&library.oneshots_directory)
    .bind(&library.id)
    .execute(&mut **tx)
    .await?
    .rows_affected()
        > 0;
    Ok(updated)
}

async fn replace_library_exclusions(
    tx: &mut Transaction<'_, Sqlite>,
    library_id: &str,
    exclusions: &[String],
) -> Result<(), sqlx::Error> {
    sqlx::query(r#"DELETE FROM LIBRARY_EXCLUSIONS WHERE LIBRARY_ID = ?"#)
        .bind(library_id)
        .execute(&mut **tx)
        .await?;

    for exclusion in exclusions {
        sqlx::query(r#"INSERT INTO LIBRARY_EXCLUSIONS (LIBRARY_ID, EXCLUSION) VALUES (?, ?)"#)
            .bind(library_id)
            .bind(exclusion)
            .execute(&mut **tx)
            .await?;
    }

    Ok(())
}

fn map_persisted_library_row(row: sqlx::sqlite::SqliteRow) -> PersistedLibraryWriteModel {
    PersistedLibraryWriteModel {
        id: row.get::<String, _>("ID"),
        name: row.get::<String, _>("NAME"),
        root: row.get::<String, _>("ROOT"),
        import_comicinfo_book: row.get::<bool, _>("IMPORT_COMICINFO_BOOK"),
        import_comicinfo_series: row.get::<bool, _>("IMPORT_COMICINFO_SERIES"),
        import_comicinfo_collection: row.get::<bool, _>("IMPORT_COMICINFO_COLLECTION"),
        import_comicinfo_readlist: row.get::<bool, _>("IMPORT_COMICINFO_READLIST"),
        import_comicinfo_series_append_volume: row
            .get::<bool, _>("IMPORT_COMICINFO_SERIES_APPEND_VOLUME"),
        import_epub_book: row.get::<bool, _>("IMPORT_EPUB_BOOK"),
        import_epub_series: row.get::<bool, _>("IMPORT_EPUB_SERIES"),
        import_mylar_series: row.get::<bool, _>("IMPORT_MYLAR_SERIES"),
        import_local_artwork: row.get::<bool, _>("IMPORT_LOCAL_ARTWORK"),
        import_barcode_isbn: row.get::<bool, _>("IMPORT_BARCODE_ISBN"),
        scan_force_modified_time: row.get::<bool, _>("SCAN_FORCE_MODIFIED_TIME"),
        scan_interval: row.get::<String, _>("SCAN_INTERVAL"),
        scan_on_startup: row.get::<bool, _>("SCAN_STARTUP"),
        scan_cbx: row.get::<bool, _>("SCAN_CBX"),
        scan_pdf: row.get::<bool, _>("SCAN_PDF"),
        scan_epub: row.get::<bool, _>("SCAN_EPUB"),
        scan_directory_exclusions: vec![],
        repair_extensions: row.get::<bool, _>("REPAIR_EXTENSIONS"),
        convert_to_cbz: row.get::<bool, _>("CONVERT_TO_CBZ"),
        empty_trash_after_scan: row.get::<bool, _>("EMPTY_TRASH_AFTER_SCAN"),
        series_cover: row.get::<String, _>("SERIES_COVER"),
        hash_files: row.get::<bool, _>("HASH_FILES"),
        hash_pages: row.get::<bool, _>("HASH_PAGES"),
        hash_koreader: row.get::<bool, _>("HASH_KOREADER"),
        analyze_dimensions: row.get::<bool, _>("ANALYZE_DIMENSIONS"),
        oneshots_directory: row.get::<Option<String>, _>("ONESHOTS_DIRECTORY"),
        unavailable: row.get::<Option<String>, _>("UNAVAILABLE_DATE").is_some(),
    }
}

fn normalize_library_root(root: &str) -> String {
    resolve_stored_path(root.trim())
        .to_string_lossy()
        .replace('\\', "/")
        .trim_end_matches('/')
        .to_string()
}
