use komga_application::discovery::{
    BookDetailQuery, BookReadlistsQuery, BookSiblingQuery, DiscoveryQueryRepository,
    NativeBooksLatestQuery, NativeBooksListQuery, NativeReadListBooksQuery, NativeSeriesListQuery,
    SeriesCollectionsQuery, SeriesDetailQuery,
};
use komga_domain::discovery::{
    BookDetailReadModel, BookReadModel, BookResourceReadModel, CollectionReadModel, DiscoveryError,
    DiscoveryQueryContext, LibraryReadModel, PageEnvelope, ReadListReadModel,
    SeriesDetailReadModel, SeriesReadModel, SeriesResourceReadModel,
};
use sqlx::SqlitePool;

use super::{BookRow, LibraryRow, SeriesRow};
use crate::discovery::queries;
use crate::discovery::queries::{book_detail, readlists};
use crate::sqlite::{SqliteTempPool, setup};

#[derive(Clone)]
pub struct SqlxRuntimeDiscoveryAdapter {
    pool: SqlitePool,
}

impl SqlxRuntimeDiscoveryAdapter {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

pub struct SqlxRuntimeDiscoveryStore {
    temp_pool: SqliteTempPool,
}

impl SqlxRuntimeDiscoveryStore {
    pub async fn new(case_id: &str) -> Result<Self, DiscoveryError> {
        let temp_pool = SqliteTempPool::new(case_id).await.map_err(map_sqlx_error)?;
        setup::bootstrap_pool(temp_pool.pool())
            .await
            .map_err(map_sqlx_error)?;
        Ok(Self { temp_pool })
    }

    pub fn adapter(&self) -> SqlxRuntimeDiscoveryAdapter {
        SqlxRuntimeDiscoveryAdapter::new(self.temp_pool.pool().clone())
    }

    pub async fn insert_library(&self, row: LibraryRow) -> Result<(), DiscoveryError> {
        sqlx::query("INSERT INTO libraries (id, name, root) VALUES (?1, ?2, ?3)")
            .bind(row.id)
            .bind(row.name)
            .bind(row.root)
            .execute(self.temp_pool.pool())
            .await
            .map_err(map_sqlx_error)?;
        Ok(())
    }

    pub async fn insert_series(&self, row: SeriesRow) -> Result<(), DiscoveryError> {
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
        .execute(self.temp_pool.pool())
        .await
        .map_err(map_sqlx_error)?;

        for label in labels {
            sqlx::query("INSERT INTO series_labels (series_id, label) VALUES (?1, ?2)")
                .bind(&id)
                .bind(label)
                .execute(self.temp_pool.pool())
                .await
                .map_err(map_sqlx_error)?;
        }

        for genre in genres {
            sqlx::query("INSERT INTO series_genres (series_id, genre) VALUES (?1, ?2)")
                .bind(&id)
                .bind(genre)
                .execute(self.temp_pool.pool())
                .await
                .map_err(map_sqlx_error)?;
        }

        for tag in tags {
            sqlx::query("INSERT INTO series_tags (series_id, tag) VALUES (?1, ?2)")
                .bind(&id)
                .bind(tag)
                .execute(self.temp_pool.pool())
                .await
                .map_err(map_sqlx_error)?;
        }

        for author in authors {
            sqlx::query("INSERT INTO series_authors (series_id, author) VALUES (?1, ?2)")
                .bind(&id)
                .bind(author)
                .execute(self.temp_pool.pool())
                .await
                .map_err(map_sqlx_error)?;
        }

        Ok(())
    }

    pub async fn insert_book(&self, row: BookRow) -> Result<(), DiscoveryError> {
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
        .execute(self.temp_pool.pool())
        .await
        .map_err(map_sqlx_error)?;

        for tag in tags {
            sqlx::query("INSERT INTO book_tags (book_id, tag) VALUES (?1, ?2)")
                .bind(&id)
                .bind(tag)
                .execute(self.temp_pool.pool())
                .await
                .map_err(map_sqlx_error)?;
        }

        for author in authors {
            sqlx::query("INSERT INTO book_authors (book_id, author) VALUES (?1, ?2)")
                .bind(&id)
                .bind(author)
                .execute(self.temp_pool.pool())
                .await
                .map_err(map_sqlx_error)?;
        }

        Ok(())
    }

    pub async fn cleanup(self) {
        self.temp_pool.cleanup().await;
    }
}

impl DiscoveryQueryRepository for SqlxRuntimeDiscoveryAdapter {
    async fn list_libraries(
        &self,
        context: &DiscoveryQueryContext,
    ) -> Result<Vec<LibraryReadModel>, DiscoveryError> {
        queries::list_libraries_sqlx(self.pool.clone(), context).await
    }

    async fn list_series(
        &self,
        context: &DiscoveryQueryContext,
        query: NativeSeriesListQuery,
    ) -> Result<PageEnvelope<SeriesReadModel>, DiscoveryError> {
        queries::list_series_sqlx(self.pool.clone(), context, &query).await
    }

    async fn list_books(
        &self,
        context: &DiscoveryQueryContext,
        query: NativeBooksListQuery,
    ) -> Result<PageEnvelope<BookReadModel>, DiscoveryError> {
        queries::books::list_books_sqlx(self.pool.clone(), context, &query).await
    }

    async fn list_books_latest(
        &self,
        context: &DiscoveryQueryContext,
        query: NativeBooksLatestQuery,
    ) -> Result<PageEnvelope<BookReadModel>, DiscoveryError> {
        queries::books::list_books_latest_sqlx(self.pool.clone(), context, &query).await
    }

    async fn list_readlist_books(
        &self,
        context: &DiscoveryQueryContext,
        query: NativeReadListBooksQuery,
    ) -> Result<PageEnvelope<BookReadModel>, DiscoveryError> {
        readlists::list_readlist_books_sqlx(self.pool.clone(), context, &query).await
    }

    async fn resolve_series_resource(
        &self,
        series_id: &str,
    ) -> Result<Option<SeriesResourceReadModel>, DiscoveryError> {
        queries::resolve_series_resource_sqlx(self.pool.clone(), series_id).await
    }

    async fn get_series_detail(
        &self,
        context: &DiscoveryQueryContext,
        query: SeriesDetailQuery,
    ) -> Result<Option<SeriesDetailReadModel>, DiscoveryError> {
        queries::get_series_detail_sqlx(self.pool.clone(), context, &query).await
    }

    async fn resolve_book_resource(
        &self,
        book_id: &str,
    ) -> Result<Option<BookResourceReadModel>, DiscoveryError> {
        queries::resolve_book_resource_sqlx(self.pool.clone(), book_id).await
    }

    async fn get_book_detail(
        &self,
        context: &DiscoveryQueryContext,
        query: BookDetailQuery,
    ) -> Result<Option<BookDetailReadModel>, DiscoveryError> {
        book_detail::get_book_detail_sqlx(self.pool.clone(), context, &query).await
    }

    async fn get_book_sibling_previous(
        &self,
        context: &DiscoveryQueryContext,
        query: BookSiblingQuery,
    ) -> Result<Option<BookDetailReadModel>, DiscoveryError> {
        book_detail::get_book_sibling_previous_sqlx(self.pool.clone(), context, &query).await
    }

    async fn get_book_sibling_next(
        &self,
        context: &DiscoveryQueryContext,
        query: BookSiblingQuery,
    ) -> Result<Option<BookDetailReadModel>, DiscoveryError> {
        book_detail::get_book_sibling_next_sqlx(self.pool.clone(), context, &query).await
    }

    async fn list_book_readlists(
        &self,
        context: &DiscoveryQueryContext,
        query: BookReadlistsQuery,
    ) -> Result<Vec<ReadListReadModel>, DiscoveryError> {
        readlists::list_book_readlists_sqlx(self.pool.clone(), context, &query).await
    }

    async fn list_series_collections(
        &self,
        context: &DiscoveryQueryContext,
        query: SeriesCollectionsQuery,
    ) -> Result<Vec<CollectionReadModel>, DiscoveryError> {
        queries::list_series_collections_sqlx(self.pool.clone(), context, &query).await
    }
}

fn map_sqlx_error(error: sqlx::Error) -> DiscoveryError {
    DiscoveryError::Persistence(error.to_string())
}
