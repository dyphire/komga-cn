mod catalog_port;
mod feed_composition;
mod feed_service;
mod persisted_port;
mod persisted_service;
mod records;

pub use catalog_port::OpdsCatalogPort;
pub use feed_composition::{
    OpdsV2FeedCompositionService, OpdsV2FeedContent, OpdsV2FeedKind, OpdsV2FeedPage,
    OpdsV2FeedPageError, OpdsV2RecommendedGroup, OpdsV2RecommendedGroupContent,
    OpdsV2RecommendedPage,
};
pub use feed_service::{OpdsFeedService, OpdsFeedUserContext, OpdsPagedBooks, OpdsPagedSeries};
pub use persisted_port::OpdsPersistedPort;
pub use persisted_service::{
    OpdsCollectionDetail, OpdsLibraryScopeError, OpdsPersistedService, OpdsReadlistDetail,
    OpdsUnifiedSearchResults,
};
pub use records::{
    BrowsePublisherEntry, BrowseSeriesNavigationEntry, OpdsBookAuthorEntry, OpdsBookFeedEntry,
    OpdsSeriesEntry, PersistedBookAuthorRecord as OpdsPersistedBookAuthorRecord,
    PersistedBookAuthorRecord, PersistedBookFeedRecord, PersistedBookSearchRecord,
    PersistedLibraryRecord, PersistedNamedRecord, PersistedReadlistBookRecord,
    PersistedReadlistRecord, PersistedSeriesBookRecord, PersistedSeriesRecord,
    PersistedSeriesSearchRecord,
};
