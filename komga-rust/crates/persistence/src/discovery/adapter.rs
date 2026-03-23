use std::mem;
use std::sync::Mutex;

use komga_application::discovery::{
    BookDetailQuery, BookReadlistsQuery, BookSiblingQuery, DiscoveryQueryRepository,
    NativeBooksLatestQuery, NativeBooksListQuery, NativeReadListBooksQuery, NativeReadListsQuery,
    NativeSeriesListQuery, ReadListDetailQuery, SeriesCollectionsQuery, SeriesDetailQuery,
};
use komga_domain::discovery::{
    BookDetailReadModel, BookReadModel, BookResourceReadModel, CollectionReadModel, DiscoveryError,
    DiscoveryQueryContext, LibraryReadModel, PageEnvelope, ReadListReadModel,
    SeriesDetailReadModel, SeriesReadModel, SeriesResourceReadModel,
};
use sqlx::{SqlitePool, sqlite::SqlitePoolOptions};

use super::queries;
use super::queries::{book_detail, books, readlists};
use super::rows::{BookRow, CollectionRow, LibraryRow, ReadListRow, ReadProgressRow, SeriesRow};
use crate::sqlite::setup;

#[derive(Default)]
struct PendingRows {
    libraries: Vec<LibraryRow>,
    series: Vec<SeriesRow>,
    collections: Vec<CollectionRow>,
    read_lists: Vec<ReadListRow>,
    books: Vec<BookRow>,
    read_progress: Vec<ReadProgressRow>,
}

pub struct SqliteDiscoveryAdapter {
    pending: Mutex<PendingRows>,
    pool: Mutex<Option<SqlitePool>>,
}

impl Default for SqliteDiscoveryAdapter {
    fn default() -> Self {
        Self::new()
    }
}

impl SqliteDiscoveryAdapter {
    pub fn new() -> Self {
        Self {
            pending: Mutex::new(PendingRows::default()),
            pool: Mutex::new(None),
        }
    }

    pub fn insert_library(&mut self, row: LibraryRow) {
        self.pending
            .lock()
            .expect("pending rows lock should not be poisoned")
            .libraries
            .push(row);
    }

    pub fn insert_series(&mut self, row: SeriesRow) {
        self.pending
            .lock()
            .expect("pending rows lock should not be poisoned")
            .series
            .push(row);
    }

    pub fn insert_collection(&mut self, row: CollectionRow) {
        self.pending
            .lock()
            .expect("pending rows lock should not be poisoned")
            .collections
            .push(row);
    }

    pub fn insert_read_list(&mut self, row: ReadListRow) {
        self.pending
            .lock()
            .expect("pending rows lock should not be poisoned")
            .read_lists
            .push(row);
    }

    pub fn insert_book(&mut self, row: BookRow) {
        self.pending
            .lock()
            .expect("pending rows lock should not be poisoned")
            .books
            .push(row);
    }

    pub fn insert_read_progress(&mut self, row: ReadProgressRow) {
        self.pending
            .lock()
            .expect("pending rows lock should not be poisoned")
            .read_progress
            .push(row);
    }

    pub async fn get_readlist_book_sibling_previous(
        &self,
        context: &DiscoveryQueryContext,
        readlist_id: &str,
        book_id: &str,
    ) -> Result<Option<BookDetailReadModel>, DiscoveryError> {
        let pool = self.ready_pool().await?;
        readlists::get_readlist_book_sibling_sqlx(pool, context, readlist_id, book_id, false).await
    }

    pub async fn get_readlist_book_sibling_next(
        &self,
        context: &DiscoveryQueryContext,
        readlist_id: &str,
        book_id: &str,
    ) -> Result<Option<BookDetailReadModel>, DiscoveryError> {
        let pool = self.ready_pool().await?;
        readlists::get_readlist_book_sibling_sqlx(pool, context, readlist_id, book_id, true).await
    }

    async fn ready_pool(&self) -> Result<SqlitePool, DiscoveryError> {
        self.ensure_pool_initialized().await?;
        self.flush_pending_rows().await?;

        let pool_guard = self
            .pool
            .lock()
            .expect("discovery pool lock should not be poisoned");
        Ok(pool_guard
            .as_ref()
            .expect("pool should be initialized")
            .clone())
    }

    async fn ensure_pool_initialized(&self) -> Result<(), DiscoveryError> {
        let needs_init = self
            .pool
            .lock()
            .expect("discovery pool lock should not be poisoned")
            .is_none();
        if !needs_init {
            return Ok(());
        }

        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .map_err(map_sqlx_error)?;
        setup::bootstrap_pool(&pool).await.map_err(map_sqlx_error)?;

        let mut pool_guard = self
            .pool
            .lock()
            .expect("discovery pool lock should not be poisoned");
        if pool_guard.is_none() {
            *pool_guard = Some(pool);
        }
        Ok(())
    }

    async fn flush_pending_rows(&self) -> Result<(), DiscoveryError> {
        let pending = {
            let mut guard = self
                .pending
                .lock()
                .expect("pending rows lock should not be poisoned");
            mem::take(&mut *guard)
        };

        if pending.libraries.is_empty()
            && pending.series.is_empty()
            && pending.collections.is_empty()
            && pending.read_lists.is_empty()
            && pending.books.is_empty()
            && pending.read_progress.is_empty()
        {
            return Ok(());
        }

        let pool = {
            let pool_guard = self
                .pool
                .lock()
                .expect("discovery pool lock should not be poisoned");
            pool_guard
                .as_ref()
                .expect("pool should be initialized before flush")
                .clone()
        };

        for row in pending.libraries {
            sqlx::query("INSERT INTO libraries (id, name, root) VALUES (?1, ?2, ?3)")
                .bind(row.id)
                .bind(row.name)
                .bind(row.root)
                .execute(&pool)
                .await
                .map_err(map_sqlx_error)?;
        }

        for row in pending.series {
            let SeriesRow {
                id,
                library_id,
                title,
                labels,
                genres,
                tags,
                language,
                publisher,
                age_rating,
                release_date,
                status,
                complete,
                read_status,
                authors,
                deleted,
                oneshot,
                created,
                last_modified,
                file_last_modified,
                url,
            } = row;

            sqlx::query(
                "INSERT INTO series (id, library_id, title, age_rating, language, publisher, release_date, status, complete, read_status, deleted, oneshot, created, last_modified, file_last_modified, url) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16)",
            )
            .bind(&id)
            .bind(&library_id)
            .bind(&title)
            .bind(age_rating)
            .bind(&language)
            .bind(&publisher)
            .bind(release_date)
            .bind(&status)
            .bind(complete)
            .bind(&read_status)
            .bind(deleted)
            .bind(oneshot)
            .bind(&created)
            .bind(&last_modified)
            .bind(&file_last_modified)
            .bind(&url)
            .execute(&pool)
            .await
            .map_err(map_sqlx_error)?;

            for label in labels {
                sqlx::query("INSERT INTO series_labels (series_id, label) VALUES (?1, ?2)")
                    .bind(&id)
                    .bind(label)
                    .execute(&pool)
                    .await
                    .map_err(map_sqlx_error)?;
            }

            for genre in genres {
                sqlx::query("INSERT INTO series_genres (series_id, genre) VALUES (?1, ?2)")
                    .bind(&id)
                    .bind(genre)
                    .execute(&pool)
                    .await
                    .map_err(map_sqlx_error)?;
            }

            for tag in tags {
                sqlx::query("INSERT INTO series_tags (series_id, tag) VALUES (?1, ?2)")
                    .bind(&id)
                    .bind(tag)
                    .execute(&pool)
                    .await
                    .map_err(map_sqlx_error)?;
            }

            for author in authors {
                sqlx::query("INSERT INTO series_authors (series_id, author) VALUES (?1, ?2)")
                    .bind(&id)
                    .bind(author)
                    .execute(&pool)
                    .await
                    .map_err(map_sqlx_error)?;
            }
        }

        for row in pending.collections {
            let CollectionRow {
                id,
                name,
                ordered,
                series_ids,
                created_date,
                last_modified_date,
            } = row;

            sqlx::query(
                "INSERT INTO collections (id, name, ordered, created_date, last_modified_date) VALUES (?1, ?2, ?3, ?4, ?5)",
            )
            .bind(&id)
            .bind(name)
            .bind(ordered)
            .bind(created_date)
            .bind(last_modified_date)
            .execute(&pool)
            .await
            .map_err(map_sqlx_error)?;

            for (index, series_id) in series_ids.iter().enumerate() {
                sqlx::query(
                    "INSERT INTO collection_series (collection_id, series_id, position) VALUES (?1, ?2, ?3)",
                )
                .bind(&id)
                .bind(series_id)
                .bind(index as i64)
                .execute(&pool)
                .await
                .map_err(map_sqlx_error)?;
            }
        }

        for row in pending.read_lists {
            let ReadListRow {
                id,
                name,
                summary,
                ordered,
                book_ids,
                created_date,
                last_modified_date,
            } = row;

            sqlx::query(
                "INSERT INTO readlists (id, name, summary, ordered, created_date, last_modified_date) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            )
            .bind(&id)
            .bind(name)
            .bind(summary)
            .bind(ordered)
            .bind(created_date)
            .bind(last_modified_date)
            .execute(&pool)
            .await
            .map_err(map_sqlx_error)?;

            for (index, book_id) in book_ids.iter().enumerate() {
                sqlx::query(
                    "INSERT INTO readlist_books (readlist_id, book_id, position) VALUES (?1, ?2, ?3)",
                )
                .bind(&id)
                .bind(book_id)
                .bind(index as i64)
                .execute(&pool)
                .await
                .map_err(map_sqlx_error)?;
            }
        }

        for row in pending.books {
            let BookRow {
                id,
                series_id,
                library_id,
                title,
                url,
                created,
                last_modified,
                file_last_modified,
                size_bytes,
                media_status,
                media_profile,
                media_type,
                media_pages_count,
                metadata_release_date,
                number_sort,
                deleted,
                oneshot,
                tags,
                read_status,
                authors,
            } = row;

            sqlx::query(
                "INSERT INTO books (id, series_id, library_id, title, url, created, last_modified, file_last_modified, size_bytes, media_status, media_profile, media_type, media_pages_count, metadata_release_date, number_sort, read_status, deleted, oneshot) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18)",
            )
            .bind(&id)
            .bind(&series_id)
            .bind(&library_id)
            .bind(&title)
            .bind(&url)
            .bind(&created)
            .bind(&last_modified)
            .bind(&file_last_modified)
            .bind(size_bytes as i64)
            .bind(&media_status)
            .bind(&media_profile)
            .bind(&media_type)
            .bind(media_pages_count as i64)
            .bind(metadata_release_date)
            .bind(number_sort)
            .bind(&read_status)
            .bind(deleted)
            .bind(oneshot)
            .execute(&pool)
            .await
            .map_err(map_sqlx_error)?;

            for tag in tags {
                sqlx::query("INSERT INTO book_tags (book_id, tag) VALUES (?1, ?2)")
                    .bind(&id)
                    .bind(tag)
                    .execute(&pool)
                    .await
                    .map_err(map_sqlx_error)?;
            }

            for author in authors {
                sqlx::query("INSERT INTO book_authors (book_id, author) VALUES (?1, ?2)")
                    .bind(&id)
                    .bind(author)
                    .execute(&pool)
                    .await
                    .map_err(map_sqlx_error)?;
            }
        }

        for row in pending.read_progress {
            sqlx::query(
                "INSERT INTO read_progress (book_id, user_id, page, completed, read_date, created, last_modified, device_id, device_name) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            )
            .bind(row.book_id)
            .bind(row.user_id)
            .bind(row.page)
            .bind(row.completed)
            .bind(row.read_date)
            .bind(row.created)
            .bind(row.last_modified)
            .bind(row.device_id)
            .bind(row.device_name)
            .execute(&pool)
            .await
            .map_err(map_sqlx_error)?;
        }

        Ok(())
    }
}

impl DiscoveryQueryRepository for SqliteDiscoveryAdapter {
    async fn list_libraries(
        &self,
        context: &DiscoveryQueryContext,
    ) -> Result<Vec<LibraryReadModel>, DiscoveryError> {
        let pool = self.ready_pool().await?;
        queries::list_libraries_sqlx(pool, context).await
    }

    async fn list_series(
        &self,
        context: &DiscoveryQueryContext,
        query: NativeSeriesListQuery,
    ) -> Result<PageEnvelope<SeriesReadModel>, DiscoveryError> {
        let pool = self.ready_pool().await?;
        queries::list_series_sqlx(pool, context, &query).await
    }

    async fn list_books(
        &self,
        context: &DiscoveryQueryContext,
        query: NativeBooksListQuery,
    ) -> Result<PageEnvelope<BookReadModel>, DiscoveryError> {
        let pool = self.ready_pool().await?;
        books::list_books_sqlx(pool, context, &query).await
    }

    async fn list_books_latest(
        &self,
        context: &DiscoveryQueryContext,
        query: NativeBooksLatestQuery,
    ) -> Result<PageEnvelope<BookReadModel>, DiscoveryError> {
        let pool = self.ready_pool().await?;
        books::list_books_latest_sqlx(pool, context, &query).await
    }

    async fn list_readlist_books(
        &self,
        context: &DiscoveryQueryContext,
        query: NativeReadListBooksQuery,
    ) -> Result<PageEnvelope<BookReadModel>, DiscoveryError> {
        let pool = self.ready_pool().await?;
        readlists::list_readlist_books_sqlx(pool, context, &query).await
    }

    async fn list_readlists(
        &self,
        context: &DiscoveryQueryContext,
        query: NativeReadListsQuery,
    ) -> Result<PageEnvelope<ReadListReadModel>, DiscoveryError> {
        let pool = self.ready_pool().await?;
        readlists::list_readlists_sqlx(pool, context, &query).await
    }

    async fn resolve_series_resource(
        &self,
        series_id: &str,
    ) -> Result<Option<SeriesResourceReadModel>, DiscoveryError> {
        let pool = self.ready_pool().await?;
        queries::resolve_series_resource_sqlx(pool, series_id).await
    }

    async fn get_series_detail(
        &self,
        context: &DiscoveryQueryContext,
        query: SeriesDetailQuery,
    ) -> Result<Option<SeriesDetailReadModel>, DiscoveryError> {
        let pool = self.ready_pool().await?;
        queries::get_series_detail_sqlx(pool, context, &query).await
    }

    async fn resolve_book_resource(
        &self,
        book_id: &str,
    ) -> Result<Option<BookResourceReadModel>, DiscoveryError> {
        let pool = self.ready_pool().await?;
        queries::resolve_book_resource_sqlx(pool, book_id).await
    }

    async fn get_book_detail(
        &self,
        context: &DiscoveryQueryContext,
        query: BookDetailQuery,
    ) -> Result<Option<BookDetailReadModel>, DiscoveryError> {
        let pool = self.ready_pool().await?;
        book_detail::get_book_detail_sqlx(pool, context, &query).await
    }

    async fn get_book_sibling_previous(
        &self,
        context: &DiscoveryQueryContext,
        query: BookSiblingQuery,
    ) -> Result<Option<BookDetailReadModel>, DiscoveryError> {
        let pool = self.ready_pool().await?;
        book_detail::get_book_sibling_previous_sqlx(pool, context, &query).await
    }

    async fn get_book_sibling_next(
        &self,
        context: &DiscoveryQueryContext,
        query: BookSiblingQuery,
    ) -> Result<Option<BookDetailReadModel>, DiscoveryError> {
        let pool = self.ready_pool().await?;
        book_detail::get_book_sibling_next_sqlx(pool, context, &query).await
    }

    async fn list_book_readlists(
        &self,
        context: &DiscoveryQueryContext,
        query: BookReadlistsQuery,
    ) -> Result<Vec<ReadListReadModel>, DiscoveryError> {
        let pool = self.ready_pool().await?;
        readlists::list_book_readlists_sqlx(pool, context, &query).await
    }

    async fn get_readlist_detail(
        &self,
        context: &DiscoveryQueryContext,
        query: ReadListDetailQuery,
    ) -> Result<Option<ReadListReadModel>, DiscoveryError> {
        let pool = self.ready_pool().await?;
        readlists::get_readlist_detail_sqlx(pool, context, &query).await
    }

    async fn list_series_collections(
        &self,
        context: &DiscoveryQueryContext,
        query: SeriesCollectionsQuery,
    ) -> Result<Vec<CollectionReadModel>, DiscoveryError> {
        let pool = self.ready_pool().await?;
        queries::list_series_collections_sqlx(pool, context, &query).await
    }
}

fn map_sqlx_error(error: sqlx::Error) -> DiscoveryError {
    DiscoveryError::Persistence(error.to_string())
}
