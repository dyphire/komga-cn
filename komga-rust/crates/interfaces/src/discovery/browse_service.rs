use crate::discovery::persisted::books_queries::load_persisted_books_page;
use crate::discovery::persisted::models::{
    BooksFilterCriteria, PersistedBookTagsScope, PersistedBooksBrowseQuery, PersistedBooksSortMode,
    PersistedSeriesBrowseQuery, PersistedSeriesSortMode, PersistedSeriesSummary,
    SeriesFilterCriteria,
};
use crate::discovery::persisted::series_queries::{
    load_persisted_alphabetical_groups, load_persisted_series_page,
};
use crate::discovery_auth::context::{
    DiscoveryQueryContext as InterfacesDiscoveryQueryContext, QueryRestrictions,
};
use crate::discovery_auth::principal::AgeRestrictionKind as InterfacesAgeRestrictionKind;
use crate::state::PersistedDiscoveryListDataSource;
use async_trait::async_trait;
use komga_application::discovery::{
    BookReadModel, BookTagScope, BooksBrowseQuery, BooksBrowseRequest, BooksFeedQuery,
    DiscoveryBrowseService, DiscoveryListService, LatestBooksRequest, SeriesBrowseQuery,
    SeriesBrowseRequest, SeriesReadModel,
};
use komga_domain::discovery::PageEnvelope;
use komga_domain::discovery::{
    BookSort, DiscoveryError, DiscoveryQueryContext, SeriesFilter, SeriesSort,
};

pub fn compose_persisted_discovery_list_service(
    persisted: Box<dyn PersistedDiscoveryListDataSource>,
) -> Box<dyn DiscoveryListService> {
    Box::new(PersistedDiscoveryListFacade::new(persisted))
}

struct PersistedDiscoveryBrowseService {
    persisted: Box<dyn PersistedDiscoveryListDataSource>,
}

impl PersistedDiscoveryBrowseService {
    fn new(persisted: Box<dyn PersistedDiscoveryListDataSource>) -> Self {
        Self { persisted }
    }
}

struct PersistedDiscoveryListFacade {
    browse: PersistedDiscoveryBrowseService,
}

impl PersistedDiscoveryListFacade {
    fn new(persisted: Box<dyn PersistedDiscoveryListDataSource>) -> Self {
        Self {
            browse: PersistedDiscoveryBrowseService::new(persisted),
        }
    }
}

fn to_interfaces_context(context: &DiscoveryQueryContext) -> InterfacesDiscoveryQueryContext {
    InterfacesDiscoveryQueryContext {
        user_id: context.user_id.as_ref().map(|id| id.as_str().to_string()),
        is_admin: context.is_admin,
        authorized_library_ids: context
            .authorized_library_ids
            .as_ref()
            .map(|ids| ids.iter().map(|id| id.as_str().to_string()).collect()),
        restrictions: context.restrictions.as_ref().map(|r| QueryRestrictions {
            age: r.age,
            age_restriction: r.age_restriction.map(|kind| match kind {
                komga_domain::discovery::AgeRestrictionKind::AllowOnly => {
                    InterfacesAgeRestrictionKind::AllowOnly
                }
                komga_domain::discovery::AgeRestrictionKind::Exclude => {
                    InterfacesAgeRestrictionKind::Exclude
                }
            }),
            labels_allow: r.labels_allow.clone(),
            labels_exclude: r.labels_exclude.clone(),
        }),
    }
}

fn series_sort_to_persisted(sort: &[SeriesSort]) -> Vec<PersistedSeriesSortMode> {
    if sort.is_empty() {
        return vec![];
    }
    sort.iter()
        .map(|s| match s {
            SeriesSort::MetadataTitleSortAsc => PersistedSeriesSortMode::TitleAsc,
            SeriesSort::MetadataTitleSortDesc => PersistedSeriesSortMode::TitleDesc,
            SeriesSort::NameAsc => PersistedSeriesSortMode::NameAsc,
            SeriesSort::NameDesc => PersistedSeriesSortMode::NameDesc,
            SeriesSort::CreatedDateAsc => PersistedSeriesSortMode::CreatedAsc,
            SeriesSort::CreatedDateDesc => PersistedSeriesSortMode::CreatedDesc,
            SeriesSort::LastModifiedDateAsc => PersistedSeriesSortMode::LastModifiedAsc,
            SeriesSort::LastModifiedDateDesc => PersistedSeriesSortMode::LastModifiedDesc,
            SeriesSort::ReleaseDateAsc => PersistedSeriesSortMode::ReleaseDateAsc,
            SeriesSort::ReleaseDateDesc => PersistedSeriesSortMode::ReleaseDateDesc,
            SeriesSort::BooksCountAsc => PersistedSeriesSortMode::BooksCountAsc,
            SeriesSort::BooksCountDesc => PersistedSeriesSortMode::BooksCountDesc,
            SeriesSort::CollectionNumberAsc => PersistedSeriesSortMode::CollectionNumberAsc,
            SeriesSort::CollectionNumberDesc => PersistedSeriesSortMode::CollectionNumberDesc,
            SeriesSort::ReadDateAsc => PersistedSeriesSortMode::ReadDateAsc,
            SeriesSort::ReadDateDesc => PersistedSeriesSortMode::ReadDateDesc,
            SeriesSort::Random => PersistedSeriesSortMode::Random,
            SeriesSort::RelevanceAsc => PersistedSeriesSortMode::RelevanceAsc,
            SeriesSort::RelevanceDesc => PersistedSeriesSortMode::RelevanceDesc,
        })
        .collect()
}

fn persisted_series_to_read_model(series: &PersistedSeriesSummary) -> SeriesReadModel {
    SeriesReadModel {
        id: series.id.clone(),
        name: series.name.clone(),
        title: series.title.clone(),
    }
}

fn book_sort_to_persisted(sort: &[BookSort]) -> Vec<PersistedBooksSortMode> {
    if sort.is_empty() {
        return vec![];
    }
    sort.iter()
        .filter_map(|s| match s {
            BookSort::MetadataTitleAsc => Some(PersistedBooksSortMode::TitleAsc),
            BookSort::MetadataTitleDesc => Some(PersistedBooksSortMode::TitleDesc),
            BookSort::NameAsc => Some(PersistedBooksSortMode::NameAsc),
            BookSort::NameDesc => Some(PersistedBooksSortMode::NameDesc),
            BookSort::SeriesTitleAsc => Some(PersistedBooksSortMode::SeriesTitleAsc),
            BookSort::SeriesTitleDesc => Some(PersistedBooksSortMode::SeriesTitleDesc),
            BookSort::CreatedDateAsc => Some(PersistedBooksSortMode::CreatedDateAsc),
            BookSort::CreatedDateDesc => Some(PersistedBooksSortMode::CreatedDateDesc),
            BookSort::LastModifiedDateAsc => Some(PersistedBooksSortMode::LastModifiedDateAsc),
            BookSort::LastModifiedDateDesc => Some(PersistedBooksSortMode::LastModifiedDateDesc),
            BookSort::FileSizeAsc => Some(PersistedBooksSortMode::FileSizeAsc),
            BookSort::FileSizeDesc => Some(PersistedBooksSortMode::FileSizeDesc),
            BookSort::FileHashAsc => Some(PersistedBooksSortMode::FileHashAsc),
            BookSort::FileHashDesc => Some(PersistedBooksSortMode::FileHashDesc),
            BookSort::UrlAsc => Some(PersistedBooksSortMode::UrlAsc),
            BookSort::UrlDesc => Some(PersistedBooksSortMode::UrlDesc),
            BookSort::MediaStatusAsc => Some(PersistedBooksSortMode::MediaStatusAsc),
            BookSort::MediaStatusDesc => Some(PersistedBooksSortMode::MediaStatusDesc),
            BookSort::MediaCommentAsc => Some(PersistedBooksSortMode::MediaCommentAsc),
            BookSort::MediaCommentDesc => Some(PersistedBooksSortMode::MediaCommentDesc),
            BookSort::MediaTypeAsc => Some(PersistedBooksSortMode::MediaTypeAsc),
            BookSort::MediaTypeDesc => Some(PersistedBooksSortMode::MediaTypeDesc),
            BookSort::MediaPagesCountAsc => Some(PersistedBooksSortMode::MediaPagesCountAsc),
            BookSort::MediaPagesCountDesc => Some(PersistedBooksSortMode::MediaPagesCountDesc),
            BookSort::ReadProgressLastModifiedAsc => {
                Some(PersistedBooksSortMode::ReadProgressLastModifiedDateAsc)
            }
            BookSort::ReadProgressLastModifiedDesc => {
                Some(PersistedBooksSortMode::ReadProgressLastModifiedDateDesc)
            }
            BookSort::ReadProgressReadDateAsc => {
                Some(PersistedBooksSortMode::ReadProgressReadDateAsc)
            }
            BookSort::ReadProgressReadDateDesc => {
                Some(PersistedBooksSortMode::ReadProgressReadDateDesc)
            }
            BookSort::ReleaseDateAsc => Some(PersistedBooksSortMode::ReleaseDateAsc),
            BookSort::ReleaseDateDesc => Some(PersistedBooksSortMode::ReleaseDateDesc),
            BookSort::NumberSortAsc => Some(PersistedBooksSortMode::NumberSortAsc),
            BookSort::NumberSortDesc => Some(PersistedBooksSortMode::NumberSortDesc),
            BookSort::SeriesIdAsc => Some(PersistedBooksSortMode::SeriesIdAsc),
            BookSort::ReadListNumberAsc => Some(PersistedBooksSortMode::ReadListNumberAsc),
            BookSort::ReadListNumberDesc => Some(PersistedBooksSortMode::ReadListNumberDesc),
            BookSort::RelevanceAsc => Some(PersistedBooksSortMode::RelevanceAsc),
            BookSort::RelevanceDesc => Some(PersistedBooksSortMode::RelevanceDesc),
            BookSort::Random => None,
        })
        .collect()
}

#[async_trait]
impl DiscoveryBrowseService for PersistedDiscoveryBrowseService {
    async fn list_series(
        &self,
        context: &DiscoveryQueryContext,
        request: SeriesBrowseRequest,
    ) -> Result<PageEnvelope<SeriesReadModel>, DiscoveryError> {
        let interfaces_context = to_interfaces_context(context);
        let sort_modes = series_sort_to_persisted(&request.sort);

        let persisted_query = PersistedSeriesBrowseQuery::from_filters(
            SeriesFilterCriteria::default(),
            request.search,
            request.page.page,
            request.page.size,
            request.page.unpaged,
            sort_modes,
        )
        .with_condition(request.filter.condition.clone());

        let page =
            load_persisted_series_page(&*self.persisted, &interfaces_context, persisted_query)
                .await
                .map_err(DiscoveryError::Persistence)?;

        Ok(PageEnvelope {
            content: page
                .content
                .iter()
                .map(persisted_series_to_read_model)
                .collect(),
            page: page.page,
            size: page.size,
            total_elements: page.total_elements,
            total_pages: page.total_pages,
        })
    }

    async fn list_books(
        &self,
        context: &DiscoveryQueryContext,
        request: BooksBrowseRequest,
    ) -> Result<PageEnvelope<BookReadModel>, DiscoveryError> {
        let interfaces_context = to_interfaces_context(context);
        let sort_modes = book_sort_to_persisted(&request.sort);

        let persisted_query = PersistedBooksBrowseQuery::from_filters(
            BooksFilterCriteria::default(),
            request.search,
            request.page.page,
            request.page.size,
            request.page.unpaged,
            sort_modes,
        )
        .with_condition(request.filter.condition.clone());

        load_persisted_books_page(&*self.persisted, &interfaces_context, persisted_query)
            .await
            .map_err(DiscoveryError::Persistence)
    }

    async fn list_latest_books(
        &self,
        context: &DiscoveryQueryContext,
        request: LatestBooksRequest,
    ) -> Result<PageEnvelope<BookReadModel>, DiscoveryError> {
        let interfaces_context = to_interfaces_context(context);

        let persisted_query = PersistedBooksBrowseQuery::from_filters(
            BooksFilterCriteria {
                library_ids: request.library_ids,
                ..BooksFilterCriteria::default()
            },
            None,
            request.page.page,
            request.page.size,
            request.page.unpaged,
            vec![PersistedBooksSortMode::LastModifiedDateDesc],
        );

        load_persisted_books_page(&*self.persisted, &interfaces_context, persisted_query)
            .await
            .map_err(DiscoveryError::Persistence)
    }
}

#[async_trait]
impl DiscoveryListService for PersistedDiscoveryListFacade {
    async fn list_series(
        &self,
        context: &DiscoveryQueryContext,
        query: SeriesBrowseQuery,
    ) -> Result<PageEnvelope<SeriesReadModel>, DiscoveryError> {
        self.browse.list_series(context, query.into()).await
    }

    async fn list_books(
        &self,
        context: &DiscoveryQueryContext,
        query: BooksBrowseQuery,
    ) -> Result<PageEnvelope<BookReadModel>, DiscoveryError> {
        self.browse.list_books(context, query.into()).await
    }

    async fn list_books_latest(
        &self,
        context: &DiscoveryQueryContext,
        query: BooksFeedQuery,
    ) -> Result<PageEnvelope<BookReadModel>, DiscoveryError> {
        self.browse.list_latest_books(context, query.into()).await
    }

    async fn list_series_alphabetical_groups(
        &self,
        context: &DiscoveryQueryContext,
        filter: SeriesFilter,
        search: Option<String>,
    ) -> Result<Vec<serde_json::Value>, DiscoveryError> {
        let interfaces_context = to_interfaces_context(context);

        load_persisted_alphabetical_groups(
            &*self.browse.persisted,
            &interfaces_context,
            filter.condition,
            search,
        )
        .await
        .map_err(DiscoveryError::Persistence)
    }

    async fn list_genres(
        &self,
        _context: &DiscoveryQueryContext,
        library_ids: Option<Vec<String>>,
        collection_id: Option<String>,
    ) -> Result<Vec<String>, DiscoveryError> {
        self.browse
            .persisted
            .load_persisted_genres(library_ids.as_deref(), collection_id.as_deref())
            .await
            .map_err(DiscoveryError::Persistence)
    }

    async fn list_tags(
        &self,
        _context: &DiscoveryQueryContext,
        library_ids: Option<Vec<String>>,
        collection_id: Option<String>,
    ) -> Result<Vec<String>, DiscoveryError> {
        self.browse
            .persisted
            .load_persisted_tags(library_ids.as_deref(), collection_id.as_deref())
            .await
            .map_err(DiscoveryError::Persistence)
    }

    async fn list_languages(
        &self,
        _context: &DiscoveryQueryContext,
        library_ids: Option<Vec<String>>,
        collection_id: Option<String>,
    ) -> Result<Vec<String>, DiscoveryError> {
        self.browse
            .persisted
            .load_persisted_languages(library_ids.as_deref(), collection_id.as_deref())
            .await
            .map_err(DiscoveryError::Persistence)
    }

    async fn list_publishers(
        &self,
        _context: &DiscoveryQueryContext,
        library_ids: Option<Vec<String>>,
        collection_id: Option<String>,
    ) -> Result<Vec<String>, DiscoveryError> {
        self.browse
            .persisted
            .load_persisted_publishers(library_ids.as_deref(), collection_id.as_deref())
            .await
            .map_err(DiscoveryError::Persistence)
    }

    async fn list_age_ratings(
        &self,
        _context: &DiscoveryQueryContext,
        library_ids: Option<Vec<String>>,
        collection_id: Option<String>,
    ) -> Result<Vec<String>, DiscoveryError> {
        self.browse
            .persisted
            .load_persisted_age_ratings(library_ids.as_deref(), collection_id.as_deref())
            .await
            .map_err(DiscoveryError::Persistence)
    }

    async fn list_sharing_labels(
        &self,
        _context: &DiscoveryQueryContext,
        library_ids: Option<Vec<String>>,
        collection_id: Option<String>,
    ) -> Result<Vec<String>, DiscoveryError> {
        self.browse
            .persisted
            .load_persisted_sharing_labels(library_ids.as_deref(), collection_id.as_deref())
            .await
            .map_err(DiscoveryError::Persistence)
    }

    async fn list_series_tags(
        &self,
        _context: &DiscoveryQueryContext,
        library_ids: Option<Vec<String>>,
        collection_id: Option<String>,
    ) -> Result<Vec<String>, DiscoveryError> {
        self.browse
            .persisted
            .load_persisted_series_tags(library_ids.as_deref(), collection_id.as_deref())
            .await
            .map_err(DiscoveryError::Persistence)
    }

    async fn list_series_release_dates(
        &self,
        _context: &DiscoveryQueryContext,
        library_ids: Option<Vec<String>>,
        collection_id: Option<String>,
    ) -> Result<Vec<String>, DiscoveryError> {
        self.browse
            .persisted
            .load_persisted_series_release_dates(library_ids.as_deref(), collection_id.as_deref())
            .await
            .map_err(DiscoveryError::Persistence)
    }

    async fn list_book_tags(
        &self,
        _context: &DiscoveryQueryContext,
        scope: Option<BookTagScope>,
        library_ids: Option<Vec<String>>,
    ) -> Result<Vec<String>, DiscoveryError> {
        let persisted_scope = scope.map(|s| match s {
            BookTagScope::All => PersistedBookTagsScope::All,
            BookTagScope::Series(id) => PersistedBookTagsScope::Series(id),
            BookTagScope::Libraries(ids) => PersistedBookTagsScope::Libraries(ids),
            BookTagScope::ReadList(id) => PersistedBookTagsScope::ReadList(id),
        });
        self.browse
            .persisted
            .load_persisted_book_tags(persisted_scope, library_ids.as_deref())
            .await
            .map_err(DiscoveryError::Persistence)
    }
}
