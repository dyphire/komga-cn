use std::collections::HashMap;
use std::path::Path;
use std::path::PathBuf;
use std::sync::{OnceLock, RwLock};

use super::index_lifecycle::{
    SearchEntityType, SearchError, SearchEvent, SearchIndexLifecycle, prepare_for_rebuild,
};

mod db;
mod loaders;
mod rebuild;

#[cfg(test)]
mod tests;

#[derive(Clone, Debug)]
pub struct BookAnalysisInput {
    pub title: String,
    pub url: String,
    pub root: String,
    pub analyze_dimensions: bool,
    pub series_id: String,
    pub previous_media_status: String,
    pub previous_page_count: i64,
}

#[derive(Clone, Debug)]
pub struct AnalyzedBookPage {
    pub file_name: String,
    pub media_type: String,
    pub width: Option<i64>,
    pub height: Option<i64>,
    pub file_size: i64,
}

#[derive(Clone, Debug)]
pub struct AnalyzedBookMedia {
    pub status: String,
    pub media_type: String,
    pub pages: Vec<AnalyzedBookPage>,
}

fn runtime_index_database_mappings() -> &'static RwLock<HashMap<PathBuf, PathBuf>> {
    static RUNTIME_INDEX_DATABASE_MAPPINGS: OnceLock<RwLock<HashMap<PathBuf, PathBuf>>> =
        OnceLock::new();
    RUNTIME_INDEX_DATABASE_MAPPINGS.get_or_init(|| RwLock::new(HashMap::new()))
}

fn register_runtime_owned_index_database(database_file: &Path, index_dir: &Path) {
    let mappings = runtime_index_database_mappings();
    let mut guard = mappings
        .write()
        .expect("runtime index/database mapping write lock should not be poisoned");
    guard.insert(index_dir.to_path_buf(), database_file.to_path_buf());
}

fn resolve_runtime_owned_index_database(index_dir: &Path) -> Option<PathBuf> {
    let mappings = runtime_index_database_mappings();
    let guard = mappings
        .read()
        .expect("runtime index/database mapping read lock should not be poisoned");
    guard.get(index_dir).cloned()
}

pub fn rebuild_index_from_database(database_file: &Path, index_dir: &Path) -> Result<(), String> {
    register_runtime_owned_index_database(database_file, index_dir);
    rebuild::rebuild_index_from_database(database_file, index_dir)
}

pub fn rebuild_index_from_database_for_entities(
    database_file: &Path,
    index_dir: &Path,
    entity_types: Option<&[SearchEntityType]>,
) -> Result<(), String> {
    register_runtime_owned_index_database(database_file, index_dir);
    rebuild::rebuild_index_from_database_for_entities(database_file, index_dir, entity_types)
}

fn bootstrap_runtime_owned_index_for_sync(
    database_file: &Path,
    index_dir: &Path,
) -> Result<SearchIndexLifecycle, String> {
    match SearchIndexLifecycle::bootstrap(index_dir) {
        Ok(index) => Ok(index),
        Err(SearchError::CorruptedIndexRequiresExplicitRebuild(_, _)) => {
            prepare_for_rebuild(index_dir)
                .map_err(|error| format!("failed to prepare search index rebuild: {error}"))?;
            rebuild::rebuild_index_from_database(database_file, index_dir)?;
            SearchIndexLifecycle::bootstrap(index_dir)
                .map_err(|error| format!("failed to bootstrap search index after rebuild: {error}"))
        }
        Err(error) => Err(format!("failed to bootstrap search index: {error}")),
    }
}

pub fn analyze_book_input(
    database_file: &Path,
    book_id: &str,
) -> Result<Option<BookAnalysisInput>, String> {
    let database_file = database_file.to_path_buf();
    let book_id = book_id.to_string();
    db::run_database_query(database_file, move |pool| {
        let book_id = book_id.clone();
        Box::pin(async move {
            let row = sqlx::query(
                r#"SELECT
                     COALESCE(bm.TITLE, b.NAME) AS TITLE,
                     b.URL AS URL,
                     b.SERIES_ID AS SERIES_ID,
                     l.ANALYZE_DIMENSIONS AS ANALYZE_DIMENSIONS,
                     COALESCE(m.STATUS, '') AS PREVIOUS_MEDIA_STATUS,
                     COALESCE(m.PAGE_COUNT, 0) AS PREVIOUS_PAGE_COUNT,
                     l.ROOT AS ROOT
                    FROM BOOK b
                    JOIN LIBRARY l ON l.ID = b.LIBRARY_ID
                   LEFT JOIN BOOK_METADATA bm ON bm.BOOK_ID = b.ID
                   LEFT JOIN MEDIA m ON m.BOOK_ID = b.ID
                      WHERE b.ID = ?
                      LIMIT 1
                     "#,
            )
            .bind(&book_id)
            .fetch_optional(&pool)
            .await
            .map_err(|error| format!("failed to load BOOK row for analyze: {error}"))?;

            Ok(row.map(|row| BookAnalysisInput {
                title: sqlx::Row::get::<String, _>(&row, "TITLE"),
                url: sqlx::Row::get::<String, _>(&row, "URL"),
                root: sqlx::Row::get::<String, _>(&row, "ROOT"),
                analyze_dimensions: sqlx::Row::get::<bool, _>(&row, "ANALYZE_DIMENSIONS"),
                series_id: sqlx::Row::get::<String, _>(&row, "SERIES_ID"),
                previous_media_status: sqlx::Row::get::<String, _>(&row, "PREVIOUS_MEDIA_STATUS"),
                previous_page_count: sqlx::Row::get::<i64, _>(&row, "PREVIOUS_PAGE_COUNT"),
            }))
        })
    })
}

pub fn persist_book_analysis(
    database_file: &Path,
    index_dir: &Path,
    book_id: &str,
    analysis: &AnalyzedBookMedia,
    update_search_index: bool,
) -> Result<(), String> {
    register_runtime_owned_index_database(database_file, index_dir);
    let database_file = database_file.to_path_buf();
    let database_file_for_sync = database_file.clone();
    let index_dir = index_dir.to_path_buf();
    let book_id = book_id.to_string();
    let analysis = analysis.clone();
    db::run_database_query(database_file, move |pool| {
        let book_id = book_id.clone();
        let analysis = analysis.clone();
        let index_dir = index_dir.clone();
        Box::pin(async move {
            let mut tx = pool.begin().await.map_err(|error| {
                format!("failed to start analyze-book transaction for '{book_id}': {error}")
            })?;

            sqlx::query("DELETE FROM MEDIA_PAGE WHERE BOOK_ID = ?")
                .bind(&book_id)
                .execute(&mut *tx)
                .await
                .map_err(|error| {
                    format!("failed to clear MEDIA_PAGE rows for '{book_id}': {error}")
                })?;

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
                .bind(&book_id)
                .bind(page.width)
                .bind(page.height)
                .bind(page.file_size)
                .execute(&mut *tx)
                .await
                .map_err(|error| {
                    format!("failed to insert MEDIA_PAGE row for '{book_id}': {error}")
                })?;
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
            .bind(&book_id)
            .bind(&analysis.status)
            .bind(&analysis.media_type)
            .bind(analysis.pages.len() as i32)
            .execute(&mut *tx)
            .await
            .map_err(|error| format!("failed to persist MEDIA analyze state: {error}"))?;

            sqlx::query(
                "UPDATE BOOK SET LAST_MODIFIED_DATE = CURRENT_TIMESTAMP WHERE ID = ?",
            )
            .bind(&book_id)
            .execute(&mut *tx)
            .await
            .map_err(|error| {
                format!("failed to refresh BOOK last-modified during analyze for '{book_id}': {error}")
            })?;

            tx.commit().await.map_err(|error| {
                format!("failed to commit analyze-book transaction for '{book_id}': {error}")
            })?;

            if update_search_index {
                let document = loaders::load_book_search_document(pool.clone(), &book_id).await?;
                let index = bootstrap_runtime_owned_index_for_sync(
                    database_file_for_sync.as_path(),
                    index_dir.as_path(),
                )?;
                if let Some(document) = document {
                    index
                        .apply_event(SearchEvent::Upsert(document))
                        .map_err(|error| format!("failed to upsert search document: {error}"))?;
                }
            }

            Ok(())
        })
    })
}

pub fn sync_entity_upsert_from_database(
    database_file: &Path,
    index_dir: &Path,
    entity_type: SearchEntityType,
    entity_id: &str,
) -> Result<bool, String> {
    register_runtime_owned_index_database(database_file, index_dir);
    let database_file = database_file.to_path_buf();
    let database_file_for_sync = database_file.clone();
    let index_dir = index_dir.to_path_buf();
    let entity_id = entity_id.to_string();
    db::run_database_query_with_max_connections(database_file, 2, move |pool| {
        let index_dir = index_dir.clone();
        let entity_id = entity_id.clone();
        Box::pin(async move {
            let document = match entity_type {
                SearchEntityType::Book => {
                    loaders::load_book_search_document(pool.clone(), &entity_id).await?
                }
                SearchEntityType::Series => {
                    loaders::load_series_search_document(pool.clone(), &entity_id).await?
                }
                SearchEntityType::Collection => {
                    loaders::load_collection_search_document(pool.clone(), &entity_id).await?
                }
                SearchEntityType::ReadList => {
                    loaders::load_readlist_search_document(pool.clone(), &entity_id).await?
                }
            };

            let Some(document) = document else {
                return Ok(false);
            };

            let index = bootstrap_runtime_owned_index_for_sync(
                database_file_for_sync.as_path(),
                index_dir.as_path(),
            )?;
            index
                .apply_event(SearchEvent::Upsert(document))
                .map_err(|error| format!("failed to upsert search document: {error}"))?;
            Ok(true)
        })
    })
}

pub fn sync_series_and_oneshot_books_after_metadata_update(
    database_file: &Path,
    index_dir: &Path,
    series_id: &str,
) -> Result<(), String> {
    register_runtime_owned_index_database(database_file, index_dir);
    let database_file = database_file.to_path_buf();
    let database_file_for_sync = database_file.clone();
    let index_dir = index_dir.to_path_buf();
    let series_id = series_id.to_string();
    db::run_database_query_with_max_connections(database_file, 2, move |pool| {
        let index_dir = index_dir.clone();
        let series_id = series_id.clone();
        Box::pin(async move {
            let series_document =
                loaders::load_series_search_document(pool.clone(), &series_id).await?;
            let oneshot_documents =
                loaders::load_oneshot_book_search_documents(pool.clone(), &series_id).await?;

            let index = bootstrap_runtime_owned_index_for_sync(
                database_file_for_sync.as_path(),
                index_dir.as_path(),
            )?;

            if let Some(document) = series_document {
                index
                    .apply_event(SearchEvent::Upsert(document))
                    .map_err(|error| format!("failed to upsert series search document: {error}"))?;
            }

            for document in oneshot_documents {
                index
                    .apply_event(SearchEvent::Upsert(document))
                    .map_err(|error| {
                        format!("failed to upsert oneshot book search document: {error}")
                    })?;
            }

            Ok(())
        })
    })
}

pub fn sync_entity_delete_from_index(
    index_dir: &Path,
    entity_type: SearchEntityType,
    entity_id: &str,
) -> Result<(), String> {
    let index = match SearchIndexLifecycle::bootstrap(index_dir) {
        Ok(index) => index,
        Err(SearchError::CorruptedIndexRequiresExplicitRebuild(_, _)) => {
            let Some(database_file) = resolve_runtime_owned_index_database(index_dir) else {
                return Err(
                    "failed to bootstrap search index: runtime-owned delete sync has no registered database file for rebuild recovery"
                        .to_string(),
                );
            };
            bootstrap_runtime_owned_index_for_sync(database_file.as_path(), index_dir)?
        }
        Err(error) => return Err(format!("failed to bootstrap search index: {error}")),
    };
    index
        .apply_event(SearchEvent::Delete {
            entity_type,
            id: entity_id.to_string(),
        })
        .map_err(|error| format!("failed to delete search document: {error}"))
}
