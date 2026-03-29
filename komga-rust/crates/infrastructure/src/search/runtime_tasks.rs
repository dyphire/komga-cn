use std::path::{Path, PathBuf};

use sqlx::Row;

use crate::sqlite::connect_pool;

use super::{SearchDocument, SearchEntityType, SearchEvent, SearchIndexLifecycle};

#[derive(Clone, Debug)]
pub struct BookAnalysisInput {
    pub title: String,
    pub url: String,
    pub root: String,
}

#[derive(Clone, Debug)]
pub struct AnalyzedBookPage {
    pub file_name: String,
    pub media_type: String,
    pub file_size: i64,
}

#[derive(Clone, Debug)]
pub struct AnalyzedBookMedia {
    pub status: String,
    pub media_type: String,
    pub pages: Vec<AnalyzedBookPage>,
}

pub fn rebuild_index_from_database(database_file: &Path, index_dir: &Path) -> Result<(), String> {
    let database_file = database_file.to_path_buf();
    let index_dir = index_dir.to_path_buf();
    run_database_query(database_file, move |pool| {
        let index_dir = index_dir.clone();
        Box::pin(async move {
            let mut docs = Vec::new();

            let book_rows = sqlx::query(
                "SELECT b.ID AS ID, COALESCE(bm.TITLE, b.NAME) AS TITLE\n                 FROM BOOK b\n                 LEFT JOIN BOOK_METADATA bm ON bm.BOOK_ID = b.ID",
            )
            .fetch_all(&pool)
            .await
            .map_err(|error| format!("failed to read BOOK rows for index rebuild: {error}"))?;
            for row in book_rows {
                docs.push(SearchDocument {
                    entity_type: SearchEntityType::Book,
                    id: row.get::<String, _>("ID"),
                    title: row.get::<String, _>("TITLE"),
                });
            }

            let series_rows = sqlx::query(
                "SELECT s.ID AS ID, COALESCE(sm.TITLE, s.NAME) AS TITLE\n                 FROM SERIES s\n                 LEFT JOIN SERIES_METADATA sm ON sm.SERIES_ID = s.ID",
            )
            .fetch_all(&pool)
            .await
            .map_err(|error| format!("failed to read SERIES rows for index rebuild: {error}"))?;
            for row in series_rows {
                docs.push(SearchDocument {
                    entity_type: SearchEntityType::Series,
                    id: row.get::<String, _>("ID"),
                    title: row.get::<String, _>("TITLE"),
                });
            }

            let collection_rows = sqlx::query("SELECT ID, NAME FROM COLLECTION")
                .fetch_all(&pool)
                .await
                .map_err(|error| {
                    format!("failed to read COLLECTION rows for index rebuild: {error}")
                })?;
            for row in collection_rows {
                docs.push(SearchDocument {
                    entity_type: SearchEntityType::Collection,
                    id: row.get::<String, _>("ID"),
                    title: row.get::<String, _>("NAME"),
                });
            }

            let readlist_rows = sqlx::query("SELECT ID, NAME FROM READLIST")
                .fetch_all(&pool)
                .await
                .map_err(|error| {
                    format!("failed to read READLIST rows for index rebuild: {error}")
                })?;
            for row in readlist_rows {
                docs.push(SearchDocument {
                    entity_type: SearchEntityType::ReadList,
                    id: row.get::<String, _>("ID"),
                    title: row.get::<String, _>("NAME"),
                });
            }

            let index = SearchIndexLifecycle::bootstrap(index_dir.as_path())
                .map_err(|error| format!("failed to bootstrap search index: {error}"))?;
            index
                .rebuild(&docs)
                .map_err(|error| format!("failed to rebuild search index: {error}"))
        })
    })
}

pub fn analyze_book_input(
    database_file: &Path,
    book_id: &str,
) -> Result<Option<BookAnalysisInput>, String> {
    let database_file = database_file.to_path_buf();
    let book_id = book_id.to_string();
    run_database_query(database_file, move |pool| {
        let book_id = book_id.clone();
        Box::pin(async move {
            let row = sqlx::query(
                "SELECT\n                     COALESCE(bm.TITLE, b.NAME) AS TITLE,\n                     b.URL AS URL,\n                     l.ROOT AS ROOT\n                 FROM BOOK b\n                 JOIN LIBRARY l ON l.ID = b.LIBRARY_ID\n                 LEFT JOIN BOOK_METADATA bm ON bm.BOOK_ID = b.ID\n                 WHERE b.ID = ?\n                 LIMIT 1",
            )
            .bind(&book_id)
            .fetch_optional(&pool)
            .await
            .map_err(|error| format!("failed to load BOOK row for analyze: {error}"))?;

            Ok(row.map(|row| BookAnalysisInput {
                title: row.get::<String, _>("TITLE"),
                url: row.get::<String, _>("URL"),
                root: row.get::<String, _>("ROOT"),
            }))
        })
    })
}

pub fn persist_book_analysis(
    database_file: &Path,
    index_dir: &Path,
    book_id: &str,
    analysis: &AnalyzedBookMedia,
) -> Result<(), String> {
    let database_file = database_file.to_path_buf();
    let index_dir = index_dir.to_path_buf();
    let book_id = book_id.to_string();
    let analysis = analysis.clone();
    run_database_query(database_file, move |pool| {
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
                    "INSERT INTO MEDIA_PAGE (FILE_NAME, MEDIA_TYPE, NUMBER, BOOK_ID, width, height, FILE_HASH, FILE_SIZE)\n                     VALUES (?, ?, ?, ?, NULL, NULL, '', ?)",
                )
                .bind(&page.file_name)
                .bind(&page.media_type)
                .bind(index as i64)
                .bind(&book_id)
                .bind(page.file_size)
                .execute(&mut *tx)
                .await
                .map_err(|error| format!("failed to insert MEDIA_PAGE row for '{book_id}': {error}"))?;
            }

            sqlx::query(
                "INSERT INTO MEDIA (BOOK_ID, STATUS, MEDIA_TYPE, PAGE_COUNT)\n                 VALUES (?, ?, ?, ?)\n                 ON CONFLICT(BOOK_ID) DO UPDATE\n                 SET STATUS = excluded.STATUS,\n                     MEDIA_TYPE = excluded.MEDIA_TYPE,\n                     PAGE_COUNT = excluded.PAGE_COUNT,\n                     LAST_MODIFIED_DATE = CURRENT_TIMESTAMP",
            )
            .bind(&book_id)
            .bind(&analysis.status)
            .bind(&analysis.media_type)
            .bind(analysis.pages.len() as i32)
            .execute(&mut *tx)
            .await
            .map_err(|error| format!("failed to persist MEDIA analyze state: {error}"))?;

            let title = sqlx::query(
                "SELECT COALESCE(bm.TITLE, b.NAME) AS TITLE\n                 FROM BOOK b\n                 LEFT JOIN BOOK_METADATA bm ON bm.BOOK_ID = b.ID\n                 WHERE b.ID = ?\n                 LIMIT 1",
            )
            .bind(&book_id)
            .fetch_optional(&mut *tx)
            .await
            .map_err(|error| format!("failed to reload analyze-book title for '{book_id}': {error}"))?
            .map(|row| row.get::<String, _>("TITLE"));

            tx.commit().await.map_err(|error| {
                format!("failed to commit analyze-book transaction for '{book_id}': {error}")
            })?;

            if let Some(title) = title {
                let index = SearchIndexLifecycle::bootstrap(index_dir.as_path())
                    .map_err(|error| format!("failed to bootstrap search index: {error}"))?;
                index
                    .apply_event(SearchEvent::Upsert(SearchDocument {
                        entity_type: SearchEntityType::Book,
                        id: book_id,
                        title,
                    }))
                    .map_err(|error| format!("failed to upsert search document: {error}"))?;
            }

            Ok(())
        })
    })
}

type BoxFuture<T> = std::pin::Pin<Box<dyn std::future::Future<Output = Result<T, String>> + Send>>;

fn run_database_query<T>(
    database_file: PathBuf,
    operation: impl FnOnce(sqlx::SqlitePool) -> BoxFuture<T> + Send + 'static,
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
