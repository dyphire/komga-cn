use crate::opds::{
    OpdsBookFeedEntry, OpdsCatalogPort, OpdsFeedService, OpdsFeedUserContext,
    OpdsLibraryScopeError, OpdsPersistedPort, OpdsPersistedService, OpdsSeriesEntry,
    PersistedLibraryRecord,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OpdsV2FeedKind {
    KeepReading,
    OnDeck,
    LatestBooks,
    LatestSeries,
}

pub enum OpdsV2FeedContent {
    Publications(Vec<OpdsBookFeedEntry>),
    Navigation(Vec<OpdsSeriesEntry>),
}

pub struct OpdsV2FeedPage {
    pub title: String,
    pub self_path: String,
    pub modified: Option<String>,
    pub page: usize,
    pub size: usize,
    pub total: usize,
    pub content: OpdsV2FeedContent,
}

pub enum OpdsV2RecommendedGroupContent {
    Libraries(Vec<PersistedLibraryRecord>),
    Publications(Vec<OpdsBookFeedEntry>),
    Navigation(Vec<OpdsSeriesEntry>),
}

pub struct OpdsV2RecommendedGroup {
    pub title: String,
    pub self_path: String,
    pub total: usize,
    pub content: OpdsV2RecommendedGroupContent,
}

pub struct OpdsV2RecommendedPage {
    pub title: String,
    pub self_path: String,
    pub modified: Option<String>,
    pub recommended_path: String,
    pub browse_path: String,
    pub collections_path: String,
    pub readlists_path: String,
    pub has_visible_collections: bool,
    pub has_visible_readlists: bool,
    pub groups: Vec<OpdsV2RecommendedGroup>,
}

#[derive(Debug, Eq, PartialEq)]
pub enum OpdsV2FeedPageError {
    LibraryScope(OpdsLibraryScopeError),
    Load(String),
}

pub struct OpdsV2FeedCompositionService<'a> {
    catalog: &'a dyn OpdsCatalogPort,
    persisted: &'a dyn OpdsPersistedPort,
}

impl<'a> OpdsV2FeedCompositionService<'a> {
    pub fn new(catalog: &'a dyn OpdsCatalogPort, persisted: &'a dyn OpdsPersistedPort) -> Self {
        Self { catalog, persisted }
    }

    pub async fn feed_page(
        &self,
        user: &OpdsFeedUserContext,
        kind: OpdsV2FeedKind,
        library_id: Option<&str>,
        page: usize,
        size: usize,
    ) -> Result<OpdsV2FeedPage, OpdsV2FeedPageError> {
        let selected_library = self.visible_library_scope(user, library_id).await?;
        let query_library_id = kind.subfeed_query_library_id(library_id);
        let feed_service = OpdsFeedService::new(self.catalog);

        let (total, content) = match kind {
            OpdsV2FeedKind::KeepReading => {
                let page = feed_service
                    .keep_reading_page(user, query_library_id, page, size)
                    .await
                    .map_err(OpdsV2FeedPageError::Load)?;
                (
                    page.total_visible_books,
                    OpdsV2FeedContent::Publications(page.books),
                )
            }
            OpdsV2FeedKind::OnDeck => {
                let page = feed_service
                    .on_deck_page(user, query_library_id, page, size)
                    .await
                    .map_err(OpdsV2FeedPageError::Load)?;
                (
                    page.total_visible_books,
                    OpdsV2FeedContent::Publications(page.books),
                )
            }
            OpdsV2FeedKind::LatestBooks => {
                let page = feed_service
                    .latest_books_page(user, query_library_id, page, size)
                    .await
                    .map_err(OpdsV2FeedPageError::Load)?;
                (
                    page.total_visible_books,
                    OpdsV2FeedContent::Publications(page.books),
                )
            }
            OpdsV2FeedKind::LatestSeries => {
                let page = feed_service
                    .latest_series_page(user, query_library_id, page, size)
                    .await
                    .map_err(OpdsV2FeedPageError::Load)?;
                (
                    page.total_visible_series,
                    OpdsV2FeedContent::Navigation(page.series),
                )
            }
        };

        Ok(OpdsV2FeedPage {
            title: feed_title(kind, selected_library.as_ref()),
            self_path: feed_self_path(kind, library_id),
            modified: selected_library.map(|library| library.last_modified),
            page,
            size,
            total,
            content,
        })
    }

    pub async fn recommended_page(
        &self,
        user: &OpdsFeedUserContext,
        library_id: Option<&str>,
    ) -> Result<OpdsV2RecommendedPage, OpdsV2FeedPageError> {
        let persisted_service = OpdsPersistedService::new(self.persisted);
        let libraries = persisted_service
            .libraries()
            .await
            .map_err(OpdsV2FeedPageError::Load)?;
        let selected_library = visible_library_from_loaded(user, &libraries, library_id)?;
        let library_segment = library_id.map(|id| format!("/{id}")).unwrap_or_default();
        let recommended_path = format!("/opds/v2/libraries{library_segment}");
        let browse_path = format!("/opds/v2/libraries{library_segment}/browse");
        let collections_path = format!("/opds/v2/libraries{library_segment}/collections");
        let readlists_path = format!("/opds/v2/libraries{library_segment}/readlists");
        let has_visible_collections = persisted_service
            .has_visible_collections_for_scope(user, library_id)
            .await;
        let has_visible_readlists = persisted_service
            .has_visible_readlists_for_scope(user, library_id)
            .await;

        let mut groups = Vec::new();
        if selected_library.is_none() {
            let visible_libraries = libraries
                .into_iter()
                .filter(|library| user.can_access_library(&library.id))
                .collect::<Vec<_>>();
            if !visible_libraries.is_empty() {
                groups.push(OpdsV2RecommendedGroup {
                    title: "Libraries".to_string(),
                    self_path: "/opds/v2/libraries".to_string(),
                    total: visible_libraries.len(),
                    content: OpdsV2RecommendedGroupContent::Libraries(visible_libraries),
                });
            }
        }

        let feed_service = OpdsFeedService::new(self.catalog);
        push_books_group(
            &mut groups,
            OpdsV2FeedKind::KeepReading,
            feed_service
                .keep_reading_page(user, library_id, 0, 5)
                .await
                .ok()
                .map(|page| (page.books, page.total_visible_books)),
            library_id,
        );
        push_books_group(
            &mut groups,
            OpdsV2FeedKind::OnDeck,
            feed_service
                .on_deck_page(user, library_id, 0, 5)
                .await
                .ok()
                .map(|page| (page.books, page.total_visible_books)),
            library_id,
        );
        push_books_group(
            &mut groups,
            OpdsV2FeedKind::LatestBooks,
            feed_service
                .latest_books_page_with_read_progress(user, library_id, 0, 5)
                .await
                .ok()
                .map(|page| (page.books, page.total_visible_books)),
            library_id,
        );
        match feed_service
            .latest_series_page(user, library_id, 0, 5)
            .await
        {
            Ok(page) if !page.series.is_empty() => {
                groups.push(OpdsV2RecommendedGroup {
                    title: OpdsV2FeedKind::LatestSeries.title().to_string(),
                    self_path: feed_self_path(OpdsV2FeedKind::LatestSeries, library_id),
                    total: page.total_visible_series,
                    content: OpdsV2RecommendedGroupContent::Navigation(page.series),
                });
            }
            _ => {}
        }

        Ok(OpdsV2RecommendedPage {
            title: selected_library
                .as_ref()
                .map(|library| format!("{} - Recommended", library.name))
                .unwrap_or_else(|| "All libraries - Recommended".to_string()),
            self_path: recommended_path.clone(),
            modified: selected_library.map(|library| library.last_modified),
            recommended_path,
            browse_path,
            collections_path,
            readlists_path,
            has_visible_collections,
            has_visible_readlists,
            groups,
        })
    }

    async fn visible_library_scope(
        &self,
        user: &OpdsFeedUserContext,
        library_id: Option<&str>,
    ) -> Result<Option<PersistedLibraryRecord>, OpdsV2FeedPageError> {
        OpdsPersistedService::new(self.persisted)
            .visible_library_scope(user, library_id)
            .await
            .map_err(OpdsV2FeedPageError::LibraryScope)
    }
}

impl OpdsV2FeedKind {
    pub fn title(self) -> &'static str {
        match self {
            Self::KeepReading => "Keep Reading",
            Self::OnDeck => "On Deck",
            Self::LatestBooks => "Latest Books",
            Self::LatestSeries => "Latest Series",
        }
    }

    fn path_suffix(self) -> &'static str {
        match self {
            Self::KeepReading => "keep-reading",
            Self::OnDeck => "on-deck",
            Self::LatestBooks => "books/latest",
            Self::LatestSeries => "series/latest",
        }
    }

    fn subfeed_query_library_id(self, library_id: Option<&str>) -> Option<&str> {
        match self {
            Self::KeepReading | Self::LatestBooks => None,
            Self::OnDeck | Self::LatestSeries => library_id,
        }
    }
}

fn push_books_group(
    groups: &mut Vec<OpdsV2RecommendedGroup>,
    kind: OpdsV2FeedKind,
    page: Option<(Vec<OpdsBookFeedEntry>, usize)>,
    library_id: Option<&str>,
) {
    let Some((books, total)) = page else {
        return;
    };
    if books.is_empty() {
        return;
    }

    groups.push(OpdsV2RecommendedGroup {
        title: kind.title().to_string(),
        self_path: feed_self_path(kind, library_id),
        total,
        content: OpdsV2RecommendedGroupContent::Publications(books),
    });
}

fn visible_library_from_loaded(
    user: &OpdsFeedUserContext,
    libraries: &[PersistedLibraryRecord],
    library_id: Option<&str>,
) -> Result<Option<PersistedLibraryRecord>, OpdsV2FeedPageError> {
    let Some(library_id) = library_id else {
        return Ok(None);
    };
    let library = libraries
        .iter()
        .find(|library| library.id == library_id)
        .cloned()
        .ok_or(OpdsV2FeedPageError::LibraryScope(
            OpdsLibraryScopeError::NotFound,
        ))?;
    if !user.can_access_library(library_id) {
        return Err(OpdsV2FeedPageError::LibraryScope(
            OpdsLibraryScopeError::Forbidden,
        ));
    }
    Ok(Some(library))
}

fn feed_title(kind: OpdsV2FeedKind, library: Option<&PersistedLibraryRecord>) -> String {
    library
        .map(|library| format!("{} - {}", library.name, kind.title()))
        .unwrap_or_else(|| format!("All libraries - {}", kind.title()))
}

fn feed_self_path(kind: OpdsV2FeedKind, library_id: Option<&str>) -> String {
    let library_segment = library_id.map(|id| format!("/{id}")).unwrap_or_default();
    format!("/opds/v2/libraries{library_segment}/{}", kind.path_suffix())
}

#[cfg(test)]
mod tests {
    use std::collections::{HashMap, HashSet};
    use std::sync::Mutex;

    use async_trait::async_trait;

    use super::*;
    use crate::opds::{
        BrowsePublisherEntry, BrowseSeriesNavigationEntry, OpdsBookFeedKind, OpdsBookFeedQuery,
        OpdsLatestSeriesFeedQuery, OpdsLibrarySeriesQuery, OpdsPagedBooks, OpdsPagedSeries,
        PersistedBookFeedRecord, PersistedBookSearchRecord, PersistedNamedRecord,
        PersistedReadlistBookRecord, PersistedReadlistRecord, PersistedSeriesBookRecord,
        PersistedSeriesRecord, PersistedSeriesSearchRecord,
    };

    type LatestBooksCall = (Option<String>, Option<String>);
    type LatestSeriesCall = Option<String>;

    #[derive(Default)]
    struct TestCatalog {
        keep_reading_books: Mutex<Vec<OpdsBookFeedEntry>>,
        on_deck_books: Mutex<Vec<OpdsBookFeedEntry>>,
        latest_books: Mutex<Vec<OpdsBookFeedEntry>>,
        latest_series: Mutex<Vec<OpdsSeriesEntry>>,
        keep_reading_calls: Mutex<Vec<Option<String>>>,
        on_deck_calls: Mutex<Vec<Option<String>>>,
        latest_books_calls: Mutex<Vec<LatestBooksCall>>,
        latest_series_calls: Mutex<Vec<LatestSeriesCall>>,
    }

    #[async_trait]
    impl OpdsCatalogPort for TestCatalog {
        async fn load_book_feed_page(
            &self,
            query: OpdsBookFeedQuery<'_>,
        ) -> Result<OpdsPagedBooks, String> {
            let books = match query.kind {
                OpdsBookFeedKind::KeepReading => {
                    self.keep_reading_calls
                        .lock()
                        .expect("test calls lock should not be poisoned")
                        .push(query.library_id.map(str::to_string));
                    self.keep_reading_books
                        .lock()
                        .expect("test books lock should not be poisoned")
                        .clone()
                }
                OpdsBookFeedKind::OnDeck => {
                    self.on_deck_calls
                        .lock()
                        .expect("test calls lock should not be poisoned")
                        .push(query.library_id.map(str::to_string));
                    self.on_deck_books
                        .lock()
                        .expect("test books lock should not be poisoned")
                        .clone()
                }
                OpdsBookFeedKind::LatestBooks {
                    include_read_progress,
                } => {
                    self.latest_books_calls
                        .lock()
                        .expect("test calls lock should not be poisoned")
                        .push((
                            include_read_progress.then_some(query.user.user_id.clone()),
                            query.library_id.map(str::to_string),
                        ));
                    self.latest_books
                        .lock()
                        .expect("test books lock should not be poisoned")
                        .clone()
                }
            };

            Ok(OpdsPagedBooks {
                total_visible_books: books.len(),
                books,
                has_next: false,
            })
        }

        async fn load_latest_series_feed_page(
            &self,
            query: OpdsLatestSeriesFeedQuery<'_>,
        ) -> Result<OpdsPagedSeries, String> {
            self.latest_series_calls
                .lock()
                .expect("test calls lock should not be poisoned")
                .push(query.library_id.map(str::to_string));
            let series = self
                .latest_series
                .lock()
                .expect("test series lock should not be poisoned")
                .clone();
            Ok(OpdsPagedSeries {
                total_visible_series: series.len(),
                series,
                has_next: false,
            })
        }

        async fn load_library_series_feed_page(
            &self,
            _query: OpdsLibrarySeriesQuery<'_>,
        ) -> Result<(Vec<OpdsSeriesEntry>, bool), String> {
            unimplemented!()
        }

        async fn load_browse_series_navigation_entries(
            &self,
            _allowed_library_ids: Option<&HashSet<String>>,
            _library_id: Option<&str>,
            _publishers: &[String],
            _page: usize,
            _size: usize,
        ) -> Result<(Vec<BrowseSeriesNavigationEntry>, usize), String> {
            unimplemented!()
        }

        async fn load_browse_publisher_entries(
            &self,
            _allowed_library_ids: Option<&HashSet<String>>,
            _library_id: Option<&str>,
        ) -> Result<Vec<BrowsePublisherEntry>, String> {
            unimplemented!()
        }

        async fn load_series_page(
            &self,
            _allowed_library_ids: Option<&HashSet<String>>,
            _search: Option<&str>,
            _publishers: &[String],
            _offset: i64,
            _limit: i64,
        ) -> Result<Vec<OpdsSeriesEntry>, String> {
            unimplemented!()
        }
    }

    #[derive(Default)]
    struct TestPersisted {
        libraries: HashMap<String, PersistedLibraryRecord>,
        load_library_calls: Mutex<Vec<String>>,
    }

    #[async_trait]
    impl OpdsPersistedPort for TestPersisted {
        async fn load_libraries(&self) -> Result<Vec<PersistedLibraryRecord>, String> {
            Ok(self.libraries.values().cloned().collect())
        }

        async fn load_library(
            &self,
            library_id: &str,
        ) -> Result<Option<PersistedLibraryRecord>, String> {
            self.load_library_calls
                .lock()
                .expect("test calls lock should not be poisoned")
                .push(library_id.to_string());
            Ok(self.libraries.get(library_id).cloned())
        }

        async fn load_readlists_for_library(
            &self,
            _library_id: &str,
        ) -> Result<Vec<PersistedReadlistRecord>, String> {
            Ok(Vec::new())
        }

        async fn load_all_readlists(&self) -> Result<Vec<PersistedReadlistRecord>, String> {
            Ok(Vec::new())
        }

        async fn load_series(
            &self,
            _series_id: &str,
        ) -> Result<Option<PersistedSeriesRecord>, String> {
            unimplemented!()
        }

        async fn load_series_books_paged(
            &self,
            _series_id: &str,
            _user_id: &str,
            _offset: i64,
            _limit: i64,
        ) -> Result<Vec<PersistedSeriesBookRecord>, String> {
            unimplemented!()
        }

        async fn load_series_tags(&self, _series_id: &str) -> Result<Vec<String>, String> {
            unimplemented!()
        }

        async fn load_readlist(
            &self,
            _readlist_id: &str,
        ) -> Result<Option<PersistedReadlistRecord>, String> {
            unimplemented!()
        }

        async fn load_readlist_books(
            &self,
            _readlist_id: &str,
        ) -> Result<Vec<PersistedReadlistBookRecord>, String> {
            Ok(Vec::new())
        }

        async fn load_unified_search_results(
            &self,
            _query: &str,
        ) -> Result<
            (
                Vec<PersistedSeriesSearchRecord>,
                Vec<PersistedBookSearchRecord>,
                Vec<PersistedNamedRecord>,
                Vec<PersistedNamedRecord>,
            ),
            String,
        > {
            unimplemented!()
        }

        async fn load_publishers(
            &self,
            _allowed_library_ids: Option<&HashSet<String>>,
        ) -> Result<Vec<String>, String> {
            unimplemented!()
        }

        async fn load_collections(
            &self,
            _library_id: Option<&str>,
        ) -> Result<Vec<PersistedNamedRecord>, String> {
            Ok(Vec::new())
        }

        async fn load_collection(
            &self,
            _collection_id: &str,
        ) -> Result<Option<PersistedNamedRecord>, String> {
            unimplemented!()
        }

        async fn load_collection_books(
            &self,
            _collection_id: &str,
        ) -> Result<Vec<PersistedBookFeedRecord>, String> {
            unimplemented!()
        }

        async fn load_collection_series(
            &self,
            _collection_id: &str,
            _ordered: bool,
        ) -> Result<Vec<PersistedSeriesRecord>, String> {
            Ok(Vec::new())
        }
    }

    #[tokio::test]
    async fn library_scoped_keep_reading_keeps_route_metadata_without_catalog_scope() {
        let catalog = TestCatalog {
            keep_reading_books: Mutex::new(vec![book("book-lib-b", "lib-b")]),
            ..Default::default()
        };
        let persisted = persisted_with_libraries([library("lib-a", "Library A")]);
        let service = OpdsV2FeedCompositionService::new(&catalog, &persisted);

        let page = service
            .feed_page(
                &user_with_libraries(None),
                OpdsV2FeedKind::KeepReading,
                Some("lib-a"),
                0,
                10,
            )
            .await
            .expect("keep-reading page should load");

        assert_eq!(page.title, "Library A - Keep Reading");
        assert_eq!(page.self_path, "/opds/v2/libraries/lib-a/keep-reading");
        assert_eq!(page.modified.as_deref(), Some("2024-02-03 04:05:06"));
        assert_eq!(page.total, 1);
        assert_eq!(
            catalog
                .keep_reading_calls
                .lock()
                .expect("test calls lock should not be poisoned")
                .as_slice(),
            &[None]
        );
        assert_publication_ids(&page.content, &["book-lib-b"]);
    }

    #[tokio::test]
    async fn library_scoped_on_deck_uses_catalog_scope() {
        let catalog = TestCatalog {
            on_deck_books: Mutex::new(vec![book("book-lib-a", "lib-a")]),
            ..Default::default()
        };
        let persisted = persisted_with_libraries([library("lib-a", "Library A")]);
        let service = OpdsV2FeedCompositionService::new(&catalog, &persisted);

        let page = service
            .feed_page(
                &user_with_libraries(None),
                OpdsV2FeedKind::OnDeck,
                Some("lib-a"),
                0,
                10,
            )
            .await
            .expect("on-deck page should load");

        assert_eq!(page.title, "Library A - On Deck");
        assert_eq!(
            catalog
                .on_deck_calls
                .lock()
                .expect("test calls lock should not be poisoned")
                .as_slice(),
            &[Some("lib-a".to_string())]
        );
        assert_publication_ids(&page.content, &["book-lib-a"]);
    }

    #[tokio::test]
    async fn library_scoped_latest_books_preserves_unscoped_kotlin_results() {
        let catalog = TestCatalog {
            latest_books: Mutex::new(vec![book("book-lib-b", "lib-b")]),
            ..Default::default()
        };
        let persisted = persisted_with_libraries([library("lib-a", "Library A")]);
        let service = OpdsV2FeedCompositionService::new(&catalog, &persisted);

        let page = service
            .feed_page(
                &user_with_libraries(None),
                OpdsV2FeedKind::LatestBooks,
                Some("lib-a"),
                0,
                10,
            )
            .await
            .expect("latest-books page should load");

        assert_eq!(page.title, "Library A - Latest Books");
        assert_eq!(
            catalog
                .latest_books_calls
                .lock()
                .expect("test calls lock should not be poisoned")
                .as_slice(),
            &[(None, None)]
        );
        assert_publication_ids(&page.content, &["book-lib-b"]);
    }

    #[tokio::test]
    async fn library_scoped_latest_series_returns_navigation_entries() {
        let catalog = TestCatalog {
            latest_series: Mutex::new(vec![series("series-lib-a", "lib-a")]),
            ..Default::default()
        };
        let persisted = persisted_with_libraries([library("lib-a", "Library A")]);
        let service = OpdsV2FeedCompositionService::new(&catalog, &persisted);

        let page = service
            .feed_page(
                &user_with_libraries(None),
                OpdsV2FeedKind::LatestSeries,
                Some("lib-a"),
                0,
                10,
            )
            .await
            .expect("latest-series page should load");

        assert_eq!(page.title, "Library A - Latest Series");
        assert_eq!(
            catalog
                .latest_series_calls
                .lock()
                .expect("test calls lock should not be poisoned")
                .as_slice(),
            &[Some("lib-a".to_string())]
        );
        assert_navigation_ids(&page.content, &["series-lib-a"]);
    }

    #[tokio::test]
    async fn forbidden_library_scope_fails_before_catalog_query() {
        let catalog = TestCatalog::default();
        let persisted = persisted_with_libraries([library("lib-b", "Library B")]);
        let service = OpdsV2FeedCompositionService::new(&catalog, &persisted);

        let result = service
            .feed_page(
                &user_with_libraries(Some(&["lib-a"])),
                OpdsV2FeedKind::OnDeck,
                Some("lib-b"),
                0,
                10,
            )
            .await;
        let Err(error) = result else {
            panic!("forbidden library scope should fail");
        };

        assert_eq!(
            error,
            OpdsV2FeedPageError::LibraryScope(OpdsLibraryScopeError::Forbidden)
        );
        assert!(
            catalog
                .on_deck_calls
                .lock()
                .expect("test calls lock should not be poisoned")
                .is_empty()
        );
    }

    #[tokio::test]
    async fn recommended_page_builds_visible_library_and_feed_groups() {
        let catalog = TestCatalog {
            keep_reading_books: Mutex::new(vec![book("keep", "lib-a")]),
            on_deck_books: Mutex::new(vec![book("deck", "lib-a")]),
            latest_books: Mutex::new(vec![book("latest", "lib-a")]),
            latest_series: Mutex::new(vec![series("series", "lib-a")]),
            ..Default::default()
        };
        let persisted = persisted_with_libraries([
            library("lib-a", "Library A"),
            library("lib-b", "Library B"),
        ]);
        let service = OpdsV2FeedCompositionService::new(&catalog, &persisted);

        let page = service
            .recommended_page(&user_with_libraries(Some(&["lib-a"])), None)
            .await
            .expect("recommended page should load");

        assert_eq!(page.title, "All libraries - Recommended");
        assert_eq!(page.self_path, "/opds/v2/libraries");
        assert_eq!(page.groups.len(), 5);
        assert_group_libraries(&page.groups[0], &["lib-a"]);
        assert_eq!(page.groups[1].title, "Keep Reading");
        assert_eq!(page.groups[1].self_path, "/opds/v2/libraries/keep-reading");
        assert_eq!(page.groups[2].title, "On Deck");
        assert_eq!(page.groups[3].title, "Latest Books");
        assert_eq!(page.groups[4].title, "Latest Series");
        assert_eq!(
            catalog
                .latest_books_calls
                .lock()
                .expect("test calls lock should not be poisoned")
                .as_slice(),
            &[(Some("user-1".to_string()), None)]
        );
    }

    fn persisted_with_libraries<const N: usize>(
        libraries: [PersistedLibraryRecord; N],
    ) -> TestPersisted {
        TestPersisted {
            libraries: libraries
                .into_iter()
                .map(|library| (library.id.clone(), library))
                .collect(),
            ..Default::default()
        }
    }

    fn library(id: &str, name: &str) -> PersistedLibraryRecord {
        PersistedLibraryRecord {
            id: id.to_string(),
            name: name.to_string(),
            last_modified: "2024-02-03 04:05:06".to_string(),
        }
    }

    fn user_with_libraries(allowed_library_ids: Option<&[&str]>) -> OpdsFeedUserContext {
        OpdsFeedUserContext {
            user_id: "user-1".to_string(),
            allowed_library_ids: allowed_library_ids
                .map(|ids| ids.iter().map(|id| id.to_string()).collect::<HashSet<_>>()),
            age: None,
            age_restriction: None,
            labels_allow: Vec::new(),
            labels_exclude: Vec::new(),
        }
    }

    fn book(id: &str, library_id: &str) -> OpdsBookFeedEntry {
        OpdsBookFeedEntry {
            id: id.to_string(),
            series_id: "series".to_string(),
            title: id.to_string(),
            series_title: "Series".to_string(),
            number: String::new(),
            number_sort: 0.0,
            summary: String::new(),
            isbn: None,
            authors: Vec::new(),
            tags: Vec::new(),
            file_name: format!("{id}.epub"),
            file_size: 1,
            media_type: "application/epub+zip".to_string(),
            page_count: 1,
            epub_divina_compatible: false,
            last_read: None,
            last_read_date: None,
            library_id: library_id.to_string(),
            age_rating: None,
            sharing_labels: Vec::new(),
            last_modified: "2026-01-01T00:00:00Z".to_string(),
            release_date: None,
        }
    }

    fn series(id: &str, library_id: &str) -> OpdsSeriesEntry {
        OpdsSeriesEntry {
            id: id.to_string(),
            library_id: library_id.to_string(),
            title: id.to_string(),
            one_shot: false,
            age_rating: None,
            sharing_labels: Vec::new(),
            last_modified: "2026-01-01T00:00:00Z".to_string(),
        }
    }

    fn assert_publication_ids(content: &OpdsV2FeedContent, expected: &[&str]) {
        let OpdsV2FeedContent::Publications(books) = content else {
            panic!("expected publication content");
        };
        assert_eq!(
            books
                .iter()
                .map(|book| book.id.as_str())
                .collect::<Vec<_>>(),
            expected
        );
    }

    fn assert_navigation_ids(content: &OpdsV2FeedContent, expected: &[&str]) {
        let OpdsV2FeedContent::Navigation(series) = content else {
            panic!("expected navigation content");
        };
        assert_eq!(
            series
                .iter()
                .map(|series| series.id.as_str())
                .collect::<Vec<_>>(),
            expected
        );
    }

    fn assert_group_libraries(group: &OpdsV2RecommendedGroup, expected: &[&str]) {
        let OpdsV2RecommendedGroupContent::Libraries(libraries) = &group.content else {
            panic!("expected library group");
        };
        assert_eq!(
            libraries
                .iter()
                .map(|library| library.id.as_str())
                .collect::<Vec<_>>(),
            expected
        );
    }
}
