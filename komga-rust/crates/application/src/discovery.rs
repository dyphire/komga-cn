use komga_domain::discovery::{
    classify_book_sorts, classify_direct_browse_books_list_sort, classify_series_sorts,
    BookDetailReadModel, BookReadModel, BookResourceReadModel, CollectionReadModel,
    DirectBrowseBooksListFamily, DiscoveryError, DiscoveryQueryContext, LibraryReadModel,
    PageEnvelope, ReadListReadModel, SeriesDetailReadModel, SeriesReadModel,
    SeriesResourceReadModel, SeriesSort,
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
    pub unpaged: bool,
    pub direct_browse_family: Option<DirectBrowseBooksListFamily>,
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
pub struct SeriesDetailQuery {
    pub series_id: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SeriesCollectionsQuery {
    pub series_id: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BookDetailQuery {
    pub book_id: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BookSiblingQuery {
    pub book_id: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BookReadlistsQuery {
    pub book_id: String,
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
    pub unpaged: bool,
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

    fn resolve_series_resource(
        &self,
        series_id: &str,
    ) -> Result<Option<SeriesResourceReadModel>, DiscoveryError>;

    fn get_series_detail(
        &self,
        context: &DiscoveryQueryContext,
        query: SeriesDetailQuery,
    ) -> Result<Option<SeriesDetailReadModel>, DiscoveryError>;

    fn resolve_book_resource(
        &self,
        book_id: &str,
    ) -> Result<Option<BookResourceReadModel>, DiscoveryError>;

    fn get_book_detail(
        &self,
        context: &DiscoveryQueryContext,
        query: BookDetailQuery,
    ) -> Result<Option<BookDetailReadModel>, DiscoveryError>;

    fn get_book_sibling_previous(
        &self,
        context: &DiscoveryQueryContext,
        query: BookSiblingQuery,
    ) -> Result<Option<BookDetailReadModel>, DiscoveryError>;

    fn get_book_sibling_next(
        &self,
        context: &DiscoveryQueryContext,
        query: BookSiblingQuery,
    ) -> Result<Option<BookDetailReadModel>, DiscoveryError>;

    fn list_book_readlists(
        &self,
        context: &DiscoveryQueryContext,
        query: BookReadlistsQuery,
    ) -> Result<Vec<ReadListReadModel>, DiscoveryError>;

    fn list_series_collections(
        &self,
        context: &DiscoveryQueryContext,
        query: SeriesCollectionsQuery,
    ) -> Result<Vec<CollectionReadModel>, DiscoveryError>;
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
        let _ = classify_book_sorts(&query.sort)?;
        self.repository
            .list_books(context, native_books_list_query(query))
    }

    pub fn list_books_direct_browse(
        &self,
        context: &DiscoveryQueryContext,
        query: BooksListQuery,
    ) -> Result<PageEnvelope<BookReadModel>, DiscoveryError> {
        classify_direct_browse_books_list_query(&query)?;
        self.repository
            .list_books(context, native_books_list_query(query))
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

    pub fn resolve_series_resource(
        &self,
        series_id: &str,
    ) -> Result<Option<SeriesResourceReadModel>, DiscoveryError> {
        self.repository.resolve_series_resource(series_id)
    }

    pub fn get_series_detail(
        &self,
        context: &DiscoveryQueryContext,
        query: SeriesDetailQuery,
    ) -> Result<Option<SeriesDetailReadModel>, DiscoveryError> {
        self.repository.get_series_detail(context, query)
    }

    pub fn list_series_collections(
        &self,
        context: &DiscoveryQueryContext,
        query: SeriesCollectionsQuery,
    ) -> Result<Vec<CollectionReadModel>, DiscoveryError> {
        self.repository.list_series_collections(context, query)
    }

    pub fn resolve_book_resource(
        &self,
        book_id: &str,
    ) -> Result<Option<BookResourceReadModel>, DiscoveryError> {
        self.repository.resolve_book_resource(book_id)
    }

    pub fn get_book_detail(
        &self,
        context: &DiscoveryQueryContext,
        query: BookDetailQuery,
    ) -> Result<Option<BookDetailReadModel>, DiscoveryError> {
        self.repository.get_book_detail(context, query)
    }

    pub fn get_book_sibling_previous(
        &self,
        context: &DiscoveryQueryContext,
        query: BookSiblingQuery,
    ) -> Result<Option<BookDetailReadModel>, DiscoveryError> {
        self.repository.get_book_sibling_previous(context, query)
    }

    pub fn get_book_sibling_next(
        &self,
        context: &DiscoveryQueryContext,
        query: BookSiblingQuery,
    ) -> Result<Option<BookDetailReadModel>, DiscoveryError> {
        self.repository.get_book_sibling_next(context, query)
    }

    pub fn list_book_readlists(
        &self,
        context: &DiscoveryQueryContext,
        query: BookReadlistsQuery,
    ) -> Result<Vec<ReadListReadModel>, DiscoveryError> {
        self.repository.list_book_readlists(context, query)
    }
}

fn native_books_list_query(query: BooksListQuery) -> NativeBooksListQuery {
    NativeBooksListQuery {
        page: query.page,
        size: query.size,
        unpaged: query.unpaged,
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
        sort: query.sort,
        search: query.search,
    }
}

fn classify_direct_browse_books_list_query(query: &BooksListQuery) -> Result<(), DiscoveryError> {
    classify_direct_browse_books_list_sort(&query.sort)?;

    let Some(family) = query.direct_browse_family else {
        return Err(DiscoveryError::NonNativeRequestShape(
            komga_domain::discovery::NonNativeRequestShape::UnsupportedBookFilter(
                "direct-browse-family".to_string(),
            ),
        ));
    };

    let Some(series_ids) = query.series_ids.as_ref() else {
        return Err(DiscoveryError::NonNativeRequestShape(
            komga_domain::discovery::NonNativeRequestShape::UnsupportedBookFilter(
                "SeriesId".to_string(),
            ),
        ));
    };
    if series_ids.len() != 1 {
        return Err(DiscoveryError::NonNativeRequestShape(
            komga_domain::discovery::NonNativeRequestShape::UnsupportedBookFilter(
                "SeriesId".to_string(),
            ),
        ));
    }

    let has_extra_filters = query.library_ids.is_some()
        || query.deleted.is_some()
        || query.oneshot.is_some()
        || query.tags.is_some()
        || query.read_statuses.is_some()
        || query.media_profiles.is_some()
        || query.media_statuses.is_some()
        || query.authors.is_some()
        || query.release_dates.is_some()
        || query.search.is_some();
    if has_extra_filters {
        return Err(DiscoveryError::NonNativeRequestShape(
            komga_domain::discovery::NonNativeRequestShape::UnsupportedBookFilter(
                "extra-filters".to_string(),
            ),
        ));
    }

    match family {
        DirectBrowseBooksListFamily::BrowseSeriesPaged if query.unpaged => {
            Err(DiscoveryError::NonNativeRequestShape(
                komga_domain::discovery::NonNativeRequestShape::UnsupportedBookFilter(
                    "unpaged".to_string(),
                ),
            ))
        }
        DirectBrowseBooksListFamily::BrowseBookSiblingsUnpaged if !query.unpaged => {
            Err(DiscoveryError::NonNativeRequestShape(
                komga_domain::discovery::NonNativeRequestShape::UnsupportedBookFilter(
                    "paged".to_string(),
                ),
            ))
        }
        _ => Ok(()),
    }
}
