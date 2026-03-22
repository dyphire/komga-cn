use komga_domain::discovery::{
    classify_book_sorts, classify_series_sorts, BookReadModel, BookSort, DiscoveryError,
    DiscoveryQueryContext, LibraryReadModel, PageEnvelope, SeriesReadModel, SeriesSort,
};

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct LibraryListQuery {}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SeriesListQuery {
    pub page: usize,
    pub size: usize,
    pub library_ids: Option<Vec<String>>,
    pub deleted: Option<bool>,
    pub oneshot: Option<bool>,
    pub read_statuses: Option<Vec<String>>,
    pub genres: Option<Vec<String>>,
    pub tags: Option<Vec<String>>,
    pub languages: Option<Vec<String>>,
    pub publishers: Option<Vec<String>>,
    pub age_ratings: Option<Vec<u16>>,
    pub release_dates: Option<Vec<String>>,
    pub sharing_labels: Option<Vec<String>>,
    pub series_statuses: Option<Vec<String>>,
    pub complete: Option<bool>,
    pub authors: Option<Vec<String>>,
    pub sort: Vec<String>,
    pub search: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BooksListQuery {
    pub page: usize,
    pub size: usize,
    pub library_ids: Option<Vec<String>>,
    pub series_ids: Option<Vec<String>>,
    pub deleted: Option<bool>,
    pub oneshot: Option<bool>,
    pub tags: Option<Vec<String>>,
    pub read_statuses: Option<Vec<String>>,
    pub media_profiles: Option<Vec<String>>,
    pub media_statuses: Option<Vec<String>>,
    pub authors: Option<Vec<String>>,
    pub release_dates: Option<Vec<String>>,
    pub sort: Vec<String>,
    pub search: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BooksLatestQuery {
    pub page: usize,
    pub size: usize,
    pub unpaged: bool,
    pub library_ids: Option<Vec<String>>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NativeSeriesListQuery {
    pub page: usize,
    pub size: usize,
    pub library_ids: Option<Vec<String>>,
    pub deleted: Option<bool>,
    pub oneshot: Option<bool>,
    pub read_statuses: Option<Vec<String>>,
    pub genres: Option<Vec<String>>,
    pub tags: Option<Vec<String>>,
    pub languages: Option<Vec<String>>,
    pub publishers: Option<Vec<String>>,
    pub age_ratings: Option<Vec<u16>>,
    pub release_dates: Option<Vec<String>>,
    pub sharing_labels: Option<Vec<String>>,
    pub series_statuses: Option<Vec<String>>,
    pub complete: Option<bool>,
    pub authors: Option<Vec<String>>,
    pub sort: Vec<SeriesSort>,
    pub search: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NativeBooksListQuery {
    pub page: usize,
    pub size: usize,
    pub library_ids: Option<Vec<String>>,
    pub series_ids: Option<Vec<String>>,
    pub deleted: Option<bool>,
    pub oneshot: Option<bool>,
    pub tags: Option<Vec<String>>,
    pub read_statuses: Option<Vec<String>>,
    pub media_profiles: Option<Vec<String>>,
    pub media_statuses: Option<Vec<String>>,
    pub authors: Option<Vec<String>>,
    pub release_dates: Option<Vec<String>>,
    pub sort: Vec<BookSort>,
    pub search: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NativeBooksLatestQuery {
    pub page: usize,
    pub size: usize,
    pub unpaged: bool,
    pub library_ids: Option<Vec<String>>,
}

pub trait DiscoveryQueryRepository {
    fn list_libraries(
        &self,
        context: &DiscoveryQueryContext,
    ) -> Result<Vec<LibraryReadModel>, DiscoveryError>;

    fn list_series(
        &self,
        context: &DiscoveryQueryContext,
        query: NativeSeriesListQuery,
    ) -> Result<PageEnvelope<SeriesReadModel>, DiscoveryError>;

    fn list_books(
        &self,
        context: &DiscoveryQueryContext,
        query: NativeBooksListQuery,
    ) -> Result<PageEnvelope<BookReadModel>, DiscoveryError>;

    fn list_books_latest(
        &self,
        context: &DiscoveryQueryContext,
        query: NativeBooksLatestQuery,
    ) -> Result<PageEnvelope<BookReadModel>, DiscoveryError>;
}

pub struct DiscoveryQueries<R> {
    repository: R,
}

impl<R> DiscoveryQueries<R>
where
    R: DiscoveryQueryRepository,
{
    pub fn new(repository: R) -> Self {
        Self { repository }
    }

    pub fn list_libraries(
        &self,
        context: &DiscoveryQueryContext,
        _query: LibraryListQuery,
    ) -> Result<Vec<LibraryReadModel>, DiscoveryError> {
        self.repository.list_libraries(context)
    }

    pub fn list_series(
        &self,
        context: &DiscoveryQueryContext,
        query: SeriesListQuery,
    ) -> Result<PageEnvelope<SeriesReadModel>, DiscoveryError> {
        let sort = classify_series_sorts(&query.sort)?;
        self.repository.list_series(
            context,
            NativeSeriesListQuery {
                page: query.page,
                size: query.size,
                library_ids: query.library_ids,
                deleted: query.deleted,
                oneshot: query.oneshot,
                read_statuses: query.read_statuses,
                genres: query.genres,
                tags: query.tags,
                languages: query.languages,
                publishers: query.publishers,
                age_ratings: query.age_ratings,
                release_dates: query.release_dates,
                sharing_labels: query.sharing_labels,
                series_statuses: query.series_statuses,
                complete: query.complete,
                authors: query.authors,
                sort,
                search: query.search,
            },
        )
    }

    pub fn list_books(
        &self,
        context: &DiscoveryQueryContext,
        query: BooksListQuery,
    ) -> Result<PageEnvelope<BookReadModel>, DiscoveryError> {
        let sort = classify_book_sorts(&query.sort)?;
        self.repository.list_books(
            context,
            NativeBooksListQuery {
                page: query.page,
                size: query.size,
                library_ids: query.library_ids,
                series_ids: query.series_ids,
                deleted: query.deleted,
                oneshot: query.oneshot,
                tags: query.tags,
                read_statuses: query.read_statuses,
                media_profiles: query.media_profiles,
                media_statuses: query.media_statuses,
                authors: query.authors,
                release_dates: query.release_dates,
                sort,
                search: query.search,
            },
        )
    }

    pub fn list_books_latest(
        &self,
        context: &DiscoveryQueryContext,
        query: BooksLatestQuery,
    ) -> Result<PageEnvelope<BookReadModel>, DiscoveryError> {
        self.repository.list_books_latest(
            context,
            NativeBooksLatestQuery {
                page: query.page,
                size: query.size,
                unpaged: query.unpaged,
                library_ids: query.library_ids,
            },
        )
    }
}
