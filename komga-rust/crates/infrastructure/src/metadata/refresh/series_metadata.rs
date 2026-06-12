use std::fs;
use std::io::Read;
use std::path::Path;

use komga_application::media_assets::{
    BookMediaRecord, book_media_is_epub, book_media_is_zip_archive,
};
use komga_application::runtime_sse::RuntimeSseEventSink;
use sqlx::{Row, SqlitePool};
use zip::result::ZipError;

use crate::filesystem::media_access::epub::load_epub_package_document;
use crate::resolve_rooted_path;

use super::SeriesMetadataImportPatch;
use super::epub::extract_epub_series_patch;
use super::sources::{extract_comicinfo_series_patch, load_mylar_series_patch};
use super::support::{
    canonicalize_string_set, dedupe_strings_preserve_order, generated_collection_id,
    most_frequent_owned, nonblank_string,
};

struct SeriesBookRefreshSource {
    media: BookMediaRecord,
}

struct SeriesMetadataRefreshState {
    status: String,
    status_lock: bool,
    title: String,
    title_lock: bool,
    title_sort: String,
    title_sort_lock: bool,
    summary: String,
    summary_lock: bool,
    reading_direction: Option<String>,
    reading_direction_lock: bool,
    publisher: String,
    publisher_lock: bool,
    age_rating: Option<u32>,
    age_rating_lock: bool,
    language: String,
    language_lock: bool,
    genres: Vec<String>,
    genres_lock: bool,
    total_book_count: Option<u32>,
    total_book_count_lock: bool,
}

struct PersistedCollectionMembership {
    id: String,
    name: String,
    ordered: bool,
    series_ids: Vec<String>,
}

async fn load_series_books_for_refresh(
    pool: &SqlitePool,
    series_id: &str,
    library_root: &Path,
) -> Result<Vec<SeriesBookRefreshSource>, String> {
    let rows = sqlx::query(
        r#"
        SELECT b.LIBRARY_ID AS LIBRARY_ID,
               b.NAME AS FILE_NAME,
               b.URL AS BOOK_URL,
               COALESCE(m.MEDIA_TYPE, 'application/octet-stream') AS MEDIA_TYPE,
               COALESCE(m.PAGE_COUNT, 0) AS PAGE_COUNT
        FROM BOOK b
        LEFT JOIN MEDIA m ON m.BOOK_ID = b.ID
        LEFT JOIN BOOK_METADATA bm ON bm.BOOK_ID = b.ID
        WHERE b.SERIES_ID = ?
          AND b.DELETED_DATE IS NULL
        ORDER BY COALESCE(bm.NUMBER_SORT, CAST(b.NUMBER AS REAL), 0) ASC,
                 b.ID ASC
        "#,
    )
    .bind(series_id)
    .fetch_all(pool)
    .await
    .map_err(|error| {
        format!("failed to load series books for metadata refresh '{series_id}': {error}")
    })?;

    Ok(rows
        .into_iter()
        .map(|row| SeriesBookRefreshSource {
            media: BookMediaRecord {
                library_id: row.get::<String, _>("LIBRARY_ID"),
                media_type: row.get::<String, _>("MEDIA_TYPE"),
                file_path: resolve_rooted_path(
                    library_root,
                    row.get::<String, _>("BOOK_URL").as_str(),
                ),
                file_name: row.get::<String, _>("FILE_NAME"),
                page_count: row.get::<i64, _>("PAGE_COUNT").max(0) as u64,
            },
        })
        .collect())
}

fn load_archive_entry_bytes(
    archive_path: &Path,
    entry_name: &str,
) -> Result<Option<Vec<u8>>, String> {
    let file = fs::File::open(archive_path).map_err(|error| {
        format!(
            "failed to open ComicInfo archive '{}': {error}",
            archive_path.display()
        )
    })?;
    let mut archive = zip::ZipArchive::new(file).map_err(|error| {
        format!(
            "failed to read ComicInfo archive '{}': {error}",
            archive_path.display()
        )
    })?;
    let mut entry = match archive.by_name(entry_name) {
        Ok(entry) => entry,
        Err(ZipError::FileNotFound) => return Ok(None),
        Err(error) => {
            return Err(format!(
                "failed to read ComicInfo archive entry '{entry_name}' from '{}': {error}",
                archive_path.display()
            ));
        }
    };
    let mut bytes = Vec::new();
    entry.read_to_end(&mut bytes).map_err(|error| {
        format!(
            "failed to read ComicInfo archive entry '{entry_name}' bytes from '{}': {error}",
            archive_path.display()
        )
    })?;
    Ok(Some(bytes))
}

fn load_comicinfo_series_patch_for_book(
    source: &SeriesBookRefreshSource,
    append_volume_to_title: bool,
) -> Result<Option<SeriesMetadataImportPatch>, String> {
    if !book_media_is_zip_archive(&source.media) {
        return Ok(None);
    }

    let Some(xml) = load_archive_entry_bytes(source.media.file_path.as_path(), "ComicInfo.xml")?
    else {
        return Ok(None);
    };
    let xml = String::from_utf8(xml).map_err(|error| {
        format!(
            "failed to decode ComicInfo.xml from '{}': {error}",
            source.media.file_path.display()
        )
    })?;
    Ok(Some(extract_comicinfo_series_patch(
        &xml,
        append_volume_to_title,
    )))
}

async fn load_epub_series_patch_for_book(
    source: &SeriesBookRefreshSource,
) -> Result<Option<SeriesMetadataImportPatch>, String> {
    if !book_media_is_epub(&source.media) {
        return Ok(None);
    }

    let Some(package_document) = load_epub_package_document(&source.media).await? else {
        return Ok(None);
    };
    Ok(Some(extract_epub_series_patch(&package_document)?))
}

fn aggregate_series_metadata_import_patches(
    patches: &[SeriesMetadataImportPatch],
) -> Option<SeriesMetadataImportPatch> {
    if patches.is_empty() {
        return None;
    }

    let genres = canonicalize_string_set(
        patches
            .iter()
            .filter_map(|patch| patch.genres.clone())
            .flatten(),
    );
    let collections =
        dedupe_strings_preserve_order(patches.iter().flat_map(|patch| patch.collections.clone()));

    let aggregated = SeriesMetadataImportPatch {
        title: most_frequent_owned(patches.iter().filter_map(|patch| patch.title.clone())),
        title_sort: most_frequent_owned(
            patches.iter().filter_map(|patch| patch.title_sort.clone()),
        ),
        status: most_frequent_owned(patches.iter().filter_map(|patch| patch.status.clone())),
        summary: None,
        reading_direction: most_frequent_owned(
            patches.iter().filter_map(|patch| patch.reading_direction),
        ),
        publisher: most_frequent_owned(patches.iter().filter_map(|patch| patch.publisher.clone())),
        age_rating: patches.iter().filter_map(|patch| patch.age_rating).max(),
        language: most_frequent_owned(patches.iter().filter_map(|patch| patch.language.clone())),
        genres: (!genres.is_empty()).then_some(genres),
        total_book_count: patches
            .iter()
            .filter_map(|patch| patch.total_book_count)
            .max(),
        collections,
    };

    (aggregated.title.is_some()
        || aggregated.title_sort.is_some()
        || aggregated.status.is_some()
        || aggregated.reading_direction.is_some()
        || aggregated.publisher.is_some()
        || aggregated.age_rating.is_some()
        || aggregated.language.is_some()
        || aggregated.genres.is_some()
        || aggregated.total_book_count.is_some()
        || !aggregated.collections.is_empty())
    .then_some(aggregated)
}

async fn load_series_metadata_refresh_state(
    pool: &SqlitePool,
    series_id: &str,
) -> Result<Option<SeriesMetadataRefreshState>, String> {
    let row = sqlx::query(
        r#"
        SELECT STATUS, STATUS_LOCK, TITLE, TITLE_LOCK, TITLE_SORT, TITLE_SORT_LOCK, SUMMARY,
               SUMMARY_LOCK, READING_DIRECTION, READING_DIRECTION_LOCK, PUBLISHER,
               PUBLISHER_LOCK, AGE_RATING, AGE_RATING_LOCK, LANGUAGE, LANGUAGE_LOCK,
               GENRES_LOCK, TOTAL_BOOK_COUNT, TOTAL_BOOK_COUNT_LOCK
        FROM SERIES_METADATA
        WHERE SERIES_ID = ?
        LIMIT 1
        "#,
    )
    .bind(series_id)
    .fetch_optional(pool)
    .await
    .map_err(|error| {
        format!("failed to load existing series metadata for '{series_id}': {error}")
    })?;

    let Some(row) = row else {
        return Ok(None);
    };

    let genres = sqlx::query(
        "SELECT GENRE FROM SERIES_METADATA_GENRE WHERE SERIES_ID = ? ORDER BY GENRE COLLATE NOCASE ASC",
    )
    .bind(series_id)
    .fetch_all(pool)
    .await
    .map_err(|error| format!("failed to load existing series genres for '{series_id}': {error}"))?
    .into_iter()
    .map(|row| row.get::<String, _>("GENRE"))
    .collect::<Vec<_>>();

    Ok(Some(SeriesMetadataRefreshState {
        status: row.get::<String, _>("STATUS"),
        status_lock: row.get::<bool, _>("STATUS_LOCK"),
        title: row.get::<String, _>("TITLE"),
        title_lock: row.get::<bool, _>("TITLE_LOCK"),
        title_sort: row.get::<String, _>("TITLE_SORT"),
        title_sort_lock: row.get::<bool, _>("TITLE_SORT_LOCK"),
        summary: row.get::<String, _>("SUMMARY"),
        summary_lock: row.get::<bool, _>("SUMMARY_LOCK"),
        reading_direction: row.get::<Option<String>, _>("READING_DIRECTION"),
        reading_direction_lock: row.get::<bool, _>("READING_DIRECTION_LOCK"),
        publisher: row.get::<String, _>("PUBLISHER"),
        publisher_lock: row.get::<bool, _>("PUBLISHER_LOCK"),
        age_rating: row
            .get::<Option<i64>, _>("AGE_RATING")
            .map(|value| value.clamp(0, i64::from(i32::MAX)) as u32),
        age_rating_lock: row.get::<bool, _>("AGE_RATING_LOCK"),
        language: row.get::<String, _>("LANGUAGE"),
        language_lock: row.get::<bool, _>("LANGUAGE_LOCK"),
        genres,
        genres_lock: row.get::<bool, _>("GENRES_LOCK"),
        total_book_count: row
            .get::<Option<i64>, _>("TOTAL_BOOK_COUNT")
            .map(|value| value.clamp(0, i64::from(i32::MAX)) as u32),
        total_book_count_lock: row.get::<bool, _>("TOTAL_BOOK_COUNT_LOCK"),
    }))
}

async fn persist_series_metadata_refresh_state(
    pool: &SqlitePool,
    series_id: &str,
    state: &SeriesMetadataRefreshState,
) -> Result<(), String> {
    let mut tx = pool.begin().await.map_err(|error| {
        format!("failed to begin series metadata refresh update tx for '{series_id}': {error}")
    })?;

    let updated = sqlx::query(
        r#"
        UPDATE SERIES_METADATA
        SET STATUS = ?,
            TITLE = ?,
            TITLE_SORT = ?,
            SUMMARY = ?,
            READING_DIRECTION = ?,
            PUBLISHER = ?,
            AGE_RATING = ?,
            LANGUAGE = ?,
            TOTAL_BOOK_COUNT = ?,
            LAST_MODIFIED_DATE = CURRENT_TIMESTAMP
        WHERE SERIES_ID = ?
        "#,
    )
    .bind(&state.status)
    .bind(&state.title)
    .bind(&state.title_sort)
    .bind(&state.summary)
    .bind(state.reading_direction.as_deref())
    .bind(&state.publisher)
    .bind(state.age_rating.map(i64::from))
    .bind(&state.language)
    .bind(state.total_book_count.map(i64::from))
    .bind(series_id)
    .execute(&mut *tx)
    .await
    .map_err(|error| format!("failed to update series metadata for '{series_id}': {error}"))?
    .rows_affected()
        > 0;

    if !updated {
        tx.rollback().await.map_err(|error| {
            format!("failed to rollback missing series metadata update for '{series_id}': {error}")
        })?;
        return Err(format!(
            "series metadata row disappeared before metadata refresh for '{series_id}'"
        ));
    }

    sqlx::query("DELETE FROM SERIES_METADATA_GENRE WHERE SERIES_ID = ?")
        .bind(series_id)
        .execute(&mut *tx)
        .await
        .map_err(|error| format!("failed to clear series genres for '{series_id}': {error}"))?;

    for genre in &state.genres {
        sqlx::query("INSERT INTO SERIES_METADATA_GENRE (SERIES_ID, GENRE) VALUES (?, ?)")
            .bind(series_id)
            .bind(genre)
            .execute(&mut *tx)
            .await
            .map_err(|error| format!("failed to insert series genre for '{series_id}': {error}"))?;
    }

    tx.commit().await.map_err(|error| {
        format!("failed to commit series metadata refresh update for '{series_id}': {error}")
    })?;
    Ok(())
}

async fn apply_series_metadata_import_patch(
    pool: &SqlitePool,
    series_id: &str,
    patch: SeriesMetadataImportPatch,
) -> Result<(), String> {
    let Some(mut state) = load_series_metadata_refresh_state(pool, series_id).await? else {
        return Ok(());
    };

    let mut changed = false;

    if let Some(status) = patch.status
        && !state.status_lock
        && state.status != status
    {
        state.status = status;
        changed = true;
    }

    if let Some(title) = patch.title
        && !state.title_lock
        && state.title != title
    {
        state.title = title;
        changed = true;
    }

    if let Some(title_sort) = patch.title_sort
        && !state.title_sort_lock
        && state.title_sort != title_sort
    {
        state.title_sort = title_sort;
        changed = true;
    }

    if let Some(summary) = patch.summary
        && !state.summary_lock
        && state.summary != summary
    {
        state.summary = summary;
        changed = true;
    }

    if let Some(reading_direction) = patch.reading_direction
        && !state.reading_direction_lock
        && state.reading_direction.as_deref() != Some(reading_direction.persisted_name())
    {
        state.reading_direction = Some(reading_direction.persisted_name().to_string());
        changed = true;
    }

    if let Some(publisher) = patch.publisher
        && !state.publisher_lock
        && state.publisher != publisher
    {
        state.publisher = publisher;
        changed = true;
    }

    if let Some(age_rating) = patch.age_rating
        && !state.age_rating_lock
        && state.age_rating != Some(age_rating)
    {
        state.age_rating = Some(age_rating);
        changed = true;
    }

    if let Some(language) = patch.language
        && !state.language_lock
        && state.language != language
    {
        state.language = language;
        changed = true;
    }

    if let Some(genres) = patch.genres
        && !state.genres_lock
        && state.genres != genres
    {
        state.genres = genres;
        changed = true;
    }

    if let Some(total_book_count) = patch.total_book_count
        && !state.total_book_count_lock
        && state.total_book_count != Some(total_book_count)
    {
        state.total_book_count = Some(total_book_count);
        changed = true;
    }

    if changed {
        persist_series_metadata_refresh_state(pool, series_id, &state).await?;
    }

    Ok(())
}

async fn load_collection_membership_by_name(
    pool: &SqlitePool,
    collection_name: &str,
) -> Result<Option<PersistedCollectionMembership>, String> {
    let row = sqlx::query(
        r#"
        SELECT ID, NAME, ORDERED
        FROM COLLECTION
        WHERE NAME = ? COLLATE NOCASE
        LIMIT 1
        "#,
    )
    .bind(collection_name)
    .fetch_optional(pool)
    .await
    .map_err(|error| format!("failed to load collection by name '{collection_name}': {error}"))?;

    let Some(row) = row else {
        return Ok(None);
    };

    let collection_id = row.get::<String, _>("ID");
    let series_ids = sqlx::query(
        "SELECT SERIES_ID FROM COLLECTION_SERIES WHERE COLLECTION_ID = ? ORDER BY NUMBER ASC",
    )
    .bind(&collection_id)
    .fetch_all(pool)
    .await
    .map_err(|error| {
        format!("failed to load collection series ids for '{collection_name}': {error}")
    })?
    .into_iter()
    .map(|series_row| series_row.get::<String, _>("SERIES_ID"))
    .collect::<Vec<_>>();

    Ok(Some(PersistedCollectionMembership {
        id: collection_id,
        name: row.get::<String, _>("NAME"),
        ordered: row.get::<bool, _>("ORDERED"),
        series_ids,
    }))
}

async fn add_series_to_collection_for_refresh(
    pool: &SqlitePool,
    runtime_events: &dyn RuntimeSseEventSink,
    series_id: &str,
    collection_name: &str,
) -> Result<(), String> {
    let Some(collection_name) = nonblank_string(collection_name.to_string()) else {
        return Ok(());
    };

    if let Some(existing) = load_collection_membership_by_name(pool, &collection_name).await? {
        if existing
            .series_ids
            .iter()
            .any(|existing_series_id| existing_series_id == series_id)
        {
            return Ok(());
        }

        let mut tx = pool.begin().await.map_err(|error| {
            format!("failed to begin collection update tx for '{collection_name}': {error}")
        })?;

        sqlx::query(
            "UPDATE COLLECTION SET NAME = ?, ORDERED = ?, SERIES_COUNT = ?, LAST_MODIFIED_DATE = CURRENT_TIMESTAMP WHERE ID = ?",
        )
        .bind(&existing.name)
        .bind(existing.ordered)
        .bind((existing.series_ids.len() + 1) as i64)
        .bind(&existing.id)
        .execute(&mut *tx)
        .await
        .map_err(|error| format!("failed to update collection '{collection_name}': {error}"))?;

        sqlx::query(
            "INSERT INTO COLLECTION_SERIES (COLLECTION_ID, SERIES_ID, NUMBER) VALUES (?, ?, ?)",
        )
        .bind(&existing.id)
        .bind(series_id)
        .bind(existing.series_ids.len() as i64)
        .execute(&mut *tx)
        .await
        .map_err(|error| {
            format!("failed to append series to collection '{collection_name}': {error}")
        })?;

        tx.commit().await.map_err(|error| {
            format!("failed to commit collection update for '{collection_name}': {error}")
        })?;

        let mut series_ids = existing.series_ids;
        series_ids.push(series_id.to_string());
        super::events::emit_collection(runtime_events, &existing.id, &series_ids, false);
        return Ok(());
    }

    let collection_id = generated_collection_id(&collection_name);
    let mut tx = pool.begin().await.map_err(|error| {
        format!("failed to begin collection create tx for '{collection_name}': {error}")
    })?;

    sqlx::query(
        r#"
        INSERT INTO COLLECTION (ID, NAME, ORDERED, SERIES_COUNT, CREATED_DATE, LAST_MODIFIED_DATE)
        VALUES (?, ?, ?, ?, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP)
        "#,
    )
    .bind(&collection_id)
    .bind(&collection_name)
    .bind(false)
    .bind(1_i64)
    .execute(&mut *tx)
    .await
    .map_err(|error| format!("failed to create collection '{collection_name}': {error}"))?;

    sqlx::query(
        "INSERT INTO COLLECTION_SERIES (COLLECTION_ID, SERIES_ID, NUMBER) VALUES (?, ?, ?)",
    )
    .bind(&collection_id)
    .bind(series_id)
    .bind(0_i64)
    .execute(&mut *tx)
    .await
    .map_err(|error| {
        format!("failed to seed collection '{collection_name}' membership: {error}")
    })?;

    tx.commit().await.map_err(|error| {
        format!("failed to commit collection create for '{collection_name}': {error}")
    })?;
    super::events::emit_collection(
        runtime_events,
        &collection_id,
        &[series_id.to_string()],
        true,
    );
    Ok(())
}

async fn apply_series_collection_imports(
    pool: &SqlitePool,
    runtime_events: &dyn RuntimeSseEventSink,
    series_id: &str,
    collection_names: impl IntoIterator<Item = String>,
) -> Result<(), String> {
    for collection_name in dedupe_strings_preserve_order(collection_names) {
        add_series_to_collection_for_refresh(pool, runtime_events, series_id, &collection_name)
            .await?;
    }

    Ok(())
}

pub(super) async fn apply_mylar_series_import(
    pool: &SqlitePool,
    series_id: &str,
    library_root: &Path,
    series_url: &str,
    import_mylar_series: bool,
    oneshot: bool,
) -> Result<(), String> {
    if !import_mylar_series || oneshot {
        return Ok(());
    }

    let series_dir = resolve_rooted_path(library_root, series_url);
    let Some(patch) = load_mylar_series_patch(series_dir.as_path())? else {
        return Ok(());
    };

    apply_series_metadata_import_patch(pool, series_id, patch).await
}

#[expect(
    clippy::too_many_arguments,
    reason = "This import boundary mirrors persisted library metadata flags one-to-one."
)]
pub(super) async fn apply_series_metadata_from_book_imports(
    pool: &SqlitePool,
    runtime_events: &dyn RuntimeSseEventSink,
    series_id: &str,
    library_root: &Path,
    import_comicinfo_series: bool,
    import_comicinfo_collection: bool,
    import_comicinfo_series_append_volume: bool,
    import_epub_series: bool,
) -> Result<(), String> {
    if !(import_comicinfo_series || import_comicinfo_collection || import_epub_series) {
        return Ok(());
    }

    let books = load_series_books_for_refresh(pool, series_id, library_root).await?;

    if import_comicinfo_series || import_comicinfo_collection {
        let mut patches = Vec::new();
        for source in &books {
            if let Some(patch) =
                load_comicinfo_series_patch_for_book(source, import_comicinfo_series_append_volume)?
            {
                patches.push(patch);
            }
        }

        if import_comicinfo_series
            && let Some(aggregated) = aggregate_series_metadata_import_patches(&patches)
        {
            apply_series_metadata_import_patch(pool, series_id, aggregated).await?;
        }

        if import_comicinfo_collection {
            apply_series_collection_imports(
                pool,
                runtime_events,
                series_id,
                patches.iter().flat_map(|patch| patch.collections.clone()),
            )
            .await?;
        }
    }

    if import_epub_series {
        let mut patches = Vec::new();
        for source in &books {
            if let Some(patch) = load_epub_series_patch_for_book(source).await? {
                patches.push(patch);
            }
        }

        if let Some(aggregated) = aggregate_series_metadata_import_patches(&patches) {
            apply_series_metadata_import_patch(pool, series_id, aggregated).await?;
        }
    }

    Ok(())
}

pub(super) async fn apply_oneshot_series_metadata_import(
    pool: &SqlitePool,
    series_id: &str,
) -> Result<(), String> {
    let book_row = sqlx::query(
        r#"
        SELECT bm.TITLE AS TITLE,
               bm.SUMMARY AS SUMMARY
        FROM BOOK b
        JOIN BOOK_METADATA bm ON bm.BOOK_ID = b.ID
        WHERE b.SERIES_ID = ?
        LIMIT 1
        "#,
    )
    .bind(series_id)
    .fetch_optional(pool)
    .await
    .map_err(|error| {
        format!("failed to load oneshot series source book metadata for '{series_id}': {error}")
    })?;

    let Some(book_row) = book_row else {
        return Ok(());
    };

    let title = book_row.get::<String, _>("TITLE");
    let summary = book_row.get::<String, _>("SUMMARY");

    apply_series_metadata_import_patch(
        pool,
        series_id,
        SeriesMetadataImportPatch {
            title: Some(title.clone()),
            title_sort: Some(title),
            status: Some("ENDED".to_string()),
            summary: Some(summary),
            reading_direction: None,
            publisher: None,
            age_rating: None,
            language: None,
            genres: None,
            total_book_count: Some(1),
            collections: Vec::new(),
        },
    )
    .await
}
#[cfg(test)]
mod tests {
    use std::fs;

    use komga_application::runtime_sse::RuntimeSseEventStore;

    use super::{apply_mylar_series_import, apply_series_metadata_from_book_imports};
    use crate::test_support::BootstrappedBookFixture;

    #[tokio::test]
    async fn apply_series_metadata_from_book_imports_propagates_corrupt_comicinfo_archive_error() {
        let fixture = BootstrappedBookFixture::open("series-refresh-corrupt-comicinfo").await;
        let library_root =
            std::env::temp_dir().join(format!("komga-corrupt-comicinfo-{}", std::process::id()));
        let series_dir = library_root.join("series");
        fs::create_dir_all(&series_dir).expect("corrupt ComicInfo fixture dir should be created");
        fs::write(series_dir.join("book-1.cbz"), b"not a zip archive")
            .expect("corrupt ComicInfo archive fixture should be written");
        fixture.insert_library_series().await;
        fixture.insert_series_metadata().await;
        fixture.insert_book("book-1").await;
        fixture
            .insert_media("book-1", Some("application/zip"))
            .await;
        sqlx::query("UPDATE LIBRARY SET ROOT = ? WHERE ID = ?")
            .bind(library_root.to_string_lossy().to_string())
            .bind("library-1")
            .execute(&fixture.pool)
            .await
            .expect("library root should point at corrupt ComicInfo fixture");
        let runtime_events = RuntimeSseEventStore::default();

        let error = apply_series_metadata_from_book_imports(
            &fixture.pool,
            &runtime_events,
            "series-1",
            library_root.as_path(),
            true,
            false,
            false,
            false,
        )
        .await
        .expect_err("corrupt ComicInfo archive should fail series metadata import");

        assert!(error.contains("ComicInfo archive"), "{error}");
        let _ = fs::remove_dir_all(library_root);
        fixture.close().await;
    }

    #[tokio::test]
    async fn apply_series_metadata_from_book_imports_propagates_corrupt_epub_package_error() {
        let fixture = BootstrappedBookFixture::open("series-refresh-corrupt-epub").await;
        let library_root =
            std::env::temp_dir().join(format!("komga-corrupt-series-epub-{}", std::process::id()));
        let series_dir = library_root.join("series");
        fs::create_dir_all(&series_dir).expect("corrupt EPUB fixture dir should be created");
        fs::write(series_dir.join("book-1.epub"), b"not a zip archive")
            .expect("corrupt EPUB fixture should be written");
        fixture.insert_library_series().await;
        fixture.insert_series_metadata().await;
        fixture.insert_book("book-1").await;
        fixture
            .insert_media("book-1", Some("application/epub+zip"))
            .await;
        sqlx::query("UPDATE BOOK SET NAME = ?, URL = ? WHERE ID = ?")
            .bind("book-1.epub")
            .bind("series/book-1.epub")
            .bind("book-1")
            .execute(&fixture.pool)
            .await
            .expect("book should point at corrupt EPUB fixture");
        let runtime_events = RuntimeSseEventStore::default();

        let error = apply_series_metadata_from_book_imports(
            &fixture.pool,
            &runtime_events,
            "series-1",
            library_root.as_path(),
            false,
            false,
            false,
            true,
        )
        .await
        .expect_err("corrupt EPUB package should fail series metadata import");

        assert!(error.contains("EPUB package document"), "{error}");
        let _ = fs::remove_dir_all(library_root);
        fixture.close().await;
    }

    #[tokio::test]
    async fn apply_mylar_series_import_propagates_malformed_series_json_error() {
        let fixture = BootstrappedBookFixture::open("series-refresh-bad-mylar-json").await;
        let library_root =
            std::env::temp_dir().join(format!("komga-bad-mylar-json-{}", std::process::id()));
        let series_dir = library_root.join("series");
        fs::create_dir_all(&series_dir).expect("bad Mylar fixture dir should be created");
        fs::write(series_dir.join("series.json"), b"{not valid json")
            .expect("bad Mylar series.json fixture should be written");
        fixture.insert_library_series().await;
        fixture.insert_series_metadata().await;

        let error = apply_mylar_series_import(
            &fixture.pool,
            "series-1",
            library_root.as_path(),
            "series",
            true,
            false,
        )
        .await
        .expect_err("malformed Mylar series.json should fail series metadata import");

        assert!(error.contains("Mylar series.json"), "{error}");
        let _ = fs::remove_dir_all(library_root);
        fixture.close().await;
    }
}
