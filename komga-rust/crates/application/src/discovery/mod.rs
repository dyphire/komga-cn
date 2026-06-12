mod books;
mod browse;
mod browse_engine;
mod collections;
mod detail_port;
#[cfg(test)]
mod detail_port_tests;
mod persisted_sets;
mod query_ports;
mod read_models;
mod reading_direction;
mod readlists;
mod series;
mod series_metadata_update;
#[cfg(test)]
mod series_metadata_update_tests;

pub use books::{BookDetailQuery, BookReadlistsQuery, BookSiblingQuery};
pub use browse::{
    BookTagScope, BooksBrowseRequest, DiscoveryBrowseService, DiscoveryFacetService, FacetKind,
    FacetScope, LatestBooksRequest, PageRequest, SeriesAlphabeticalGroup,
    SeriesAlphabeticalGroupsRequest, SeriesBrowseRequest,
};
pub use browse_engine::{
    AuthorEntry, BookBrowseQuery, BookEvaluationContext, BookPosterRow, BookRow, BookSortMode,
    BrowseContext, PageEnvelope as BrowsePageEnvelope, ReadProgressRow, SeriesBrowseQuery,
    SeriesEvaluationContext, SeriesReadProgressCounts, SeriesRow, SeriesSortMode, WebLinkEntry,
    book_condition_needs_posters, book_condition_needs_readlist_memberships,
    collect_book_release_date_offsets, collect_series_release_date_offsets,
    filter_and_paginate_books, filter_and_paginate_series,
    series_condition_needs_collection_memberships, series_condition_needs_read_progress,
    series_condition_needs_total_book_counts,
};
pub use collections::{
    CollectionCreateResult, CollectionListQuery, CollectionMutationError, CollectionMutationInput,
    CollectionMutationService, CollectionProjectionService,
};
pub use detail_port::{
    BookDetailPort, CollectionMutationPort, CollectionPort, CollectionProjectionPort,
    CollectionSeriesPort, DiscoveryPersistedReadlistBookRecord, DiscoveryPersistedReadlistRecord,
    ExistingSeriesMetadataRecord, PersistedBookIdResolverPort, PersistedBookResourceRecord,
    PersistedBookSiblingDirectionRecord, PersistedCollectionAccessRecord,
    PersistedComicrackMatchCandidateRecord, PersistedSeriesCollectionRecord,
    PersistedSeriesDetailRecord, PersistedSeriesIdResolverPort, PersistedSeriesResourceRecord,
    PersistedSeriesRestrictionRecord, ReadlistBookPort, ReadlistComicRackMatchPort,
    ReadlistMutationPort, ReadlistProjectionPort, SeriesAlternateTitleRecord, SeriesDetailPort,
    SeriesMetadataLinkRecord, SeriesMetadataUpdateRecord, resolve_persisted_book_id,
    resolve_persisted_series_id,
};
pub use persisted_sets::{PersistedSetService, PersistedSetVisibilityService};
pub use query_ports::{
    AuthorFacetPort, BookSpecialListPort, CollectionSearchPort, LibraryIdMappingPort,
    PersistedAuthorEntry, PersistedAuthorsScope, PersistedBookBrowseEntry, ReadlistSearchPort,
    ScoredSearchHit,
};
pub use read_models::{
    BookDetailReadModel, BookMetadataAuthorReadModel, BookMetadataLinkReadModel, BookReadModel,
    BookReadProgressReadModel, BookResourceReadModel, CollectionReadModel, LibraryReadModel,
    ReadListReadModel, SeriesDetailReadModel, SeriesReadModel, SeriesResourceReadModel,
};
pub use reading_direction::SeriesReadingDirection;
pub use readlists::{
    ComicRackMatchBook, ComicRackMatchSeries, ComicRackReadListMatchError,
    ComicRackReadListMatchGroup, ComicRackReadListMatchResult, ComicRackReadListMatchService,
    ComicRackReadListParseError, ComicRackReadListRequest, ComicRackReadListRequestBook,
    ComicRackReadListRequestMatch, ReadListBooksOwnership, ReadListBooksQuery, ReadListDetailQuery,
    ReadListsQuery, ReadListsSort, ReadlistCreateResult, ReadlistMutationError,
    ReadlistMutationInput, ReadlistMutationService, ReadlistProjectionService,
    classify_readlist_books_query, parse_comicrack_readlist,
};
pub use series::{SeriesCollectionsQuery, SeriesDetailQuery};
pub use series_metadata_update::{
    SeriesEventEmitter, SeriesMetadataPatch, SeriesMetadataUpdateError, SeriesMetadataUpdateResult,
    SeriesMetadataWritePort, SeriesMetadataWriter, apply_series_metadata_patch,
};
