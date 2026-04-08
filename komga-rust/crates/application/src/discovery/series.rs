use komga_domain::discovery::{DiscoveryError, DiscoveryQueryContext, PageEnvelope};

use super::query_service::{DiscoveryQueries, DiscoveryQueryRepository};
use super::read_models::{
    CollectionReadModel, SeriesDetailReadModel, SeriesReadModel, SeriesResourceReadModel,
};

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
pub struct SeriesDetailQuery {
    pub series_id: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SeriesCollectionsQuery {
    pub series_id: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RuntimeSeriesListQuery {
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

impl<R> DiscoveryQueries<R>
where
    R: DiscoveryQueryRepository,
{
    pub async fn list_series(
        &self,
        context: &DiscoveryQueryContext,
        query: SeriesListQuery,
    ) -> Result<PageEnvelope<SeriesReadModel>, DiscoveryError> {
        self.repository
            .list_series(
                context,
                RuntimeSeriesListQuery {
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
                    sort: query.sort,
                    search: query.search,
                },
            )
            .await
    }

    pub async fn resolve_series_resource(
        &self,
        series_id: &str,
    ) -> Result<Option<SeriesResourceReadModel>, DiscoveryError> {
        self.repository.resolve_series_resource(series_id).await
    }

    pub async fn get_series_detail(
        &self,
        context: &DiscoveryQueryContext,
        query: SeriesDetailQuery,
    ) -> Result<Option<SeriesDetailReadModel>, DiscoveryError> {
        self.repository.get_series_detail(context, query).await
    }

    pub async fn list_series_collections(
        &self,
        context: &DiscoveryQueryContext,
        query: SeriesCollectionsQuery,
    ) -> Result<Vec<CollectionReadModel>, DiscoveryError> {
        self.repository
            .list_series_collections(context, query)
            .await
    }
}
