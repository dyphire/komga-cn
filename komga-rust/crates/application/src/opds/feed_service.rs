use crate::opds::{
    OpdsBookFeedKind, OpdsBookFeedQuery, OpdsCatalogPort, OpdsFeedUserContext,
    OpdsLatestSeriesFeedQuery, OpdsLibrarySeriesQuery, OpdsPagedBooks, OpdsPagedSeries,
    OpdsSeriesEntry,
};

pub struct OpdsFeedService<'a> {
    catalog: &'a dyn OpdsCatalogPort,
}

impl<'a> OpdsFeedService<'a> {
    pub fn new(catalog: &'a dyn OpdsCatalogPort) -> Self {
        Self { catalog }
    }

    pub async fn keep_reading_page(
        &self,
        user: &OpdsFeedUserContext,
        library_id: Option<&str>,
        page: usize,
        size: usize,
    ) -> Result<OpdsPagedBooks, String> {
        self.catalog
            .load_book_feed_page(OpdsBookFeedQuery {
                user,
                library_id,
                page,
                size,
                kind: OpdsBookFeedKind::KeepReading,
            })
            .await
    }

    pub async fn on_deck_page(
        &self,
        user: &OpdsFeedUserContext,
        library_id: Option<&str>,
        page: usize,
        size: usize,
    ) -> Result<OpdsPagedBooks, String> {
        self.catalog
            .load_book_feed_page(OpdsBookFeedQuery {
                user,
                library_id,
                page,
                size,
                kind: OpdsBookFeedKind::OnDeck,
            })
            .await
    }

    pub async fn latest_books_page(
        &self,
        user: &OpdsFeedUserContext,
        library_id: Option<&str>,
        page: usize,
        size: usize,
    ) -> Result<OpdsPagedBooks, String> {
        self.load_latest_books_page(user, library_id, page, size, false)
            .await
    }

    pub async fn latest_books_page_with_read_progress(
        &self,
        user: &OpdsFeedUserContext,
        library_id: Option<&str>,
        page: usize,
        size: usize,
    ) -> Result<OpdsPagedBooks, String> {
        self.load_latest_books_page(user, library_id, page, size, true)
            .await
    }

    async fn load_latest_books_page(
        &self,
        user: &OpdsFeedUserContext,
        library_id: Option<&str>,
        page: usize,
        size: usize,
        include_read_progress: bool,
    ) -> Result<OpdsPagedBooks, String> {
        self.catalog
            .load_book_feed_page(OpdsBookFeedQuery {
                user,
                library_id,
                page,
                size,
                kind: OpdsBookFeedKind::LatestBooks {
                    include_read_progress,
                },
            })
            .await
    }

    pub async fn latest_series_page(
        &self,
        user: &OpdsFeedUserContext,
        library_id: Option<&str>,
        page: usize,
        size: usize,
    ) -> Result<OpdsPagedSeries, String> {
        self.load_latest_series_page(user, library_id, page, size, false)
            .await
    }

    pub async fn latest_series_page_including_one_shots(
        &self,
        user: &OpdsFeedUserContext,
        library_id: Option<&str>,
        page: usize,
        size: usize,
    ) -> Result<OpdsPagedSeries, String> {
        self.load_latest_series_page(user, library_id, page, size, true)
            .await
    }

    pub async fn library_series_page(
        &self,
        user: &OpdsFeedUserContext,
        library_id: &str,
        page: usize,
        size: usize,
    ) -> Result<(Vec<OpdsSeriesEntry>, bool), String> {
        self.catalog
            .load_library_series_feed_page(OpdsLibrarySeriesQuery {
                user,
                library_id,
                page,
                size,
            })
            .await
    }

    async fn load_latest_series_page(
        &self,
        user: &OpdsFeedUserContext,
        library_id: Option<&str>,
        page: usize,
        size: usize,
        include_one_shots: bool,
    ) -> Result<OpdsPagedSeries, String> {
        self.catalog
            .load_latest_series_feed_page(OpdsLatestSeriesFeedQuery {
                user,
                library_id,
                page,
                size,
                include_one_shots,
            })
            .await
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;
    use std::sync::Mutex;

    use async_trait::async_trait;

    use super::*;
    use crate::opds::{
        BrowsePublisherEntry, BrowseSeriesNavigationEntry, OpdsAgeRestrictionKind,
        OpdsBookFeedEntry, OpdsBookFeedKind, OpdsBookFeedQuery, OpdsCatalogPort,
        OpdsLatestSeriesFeedQuery, OpdsLibrarySeriesQuery, OpdsSeriesEntry,
    };

    #[derive(Debug, PartialEq)]
    struct BookFeedCall {
        kind: OpdsBookFeedKind,
        user_id: String,
        library_id: Option<String>,
        page: usize,
        size: usize,
    }

    #[derive(Debug, PartialEq)]
    struct LatestSeriesCall {
        include_one_shots: bool,
        library_id: Option<String>,
        page: usize,
        size: usize,
    }

    #[derive(Debug, PartialEq)]
    struct LibrarySeriesCall {
        library_id: String,
        page: usize,
        size: usize,
    }

    #[derive(Default)]
    struct TestCatalog {
        book_feed_calls: Mutex<Vec<BookFeedCall>>,
        latest_series_calls: Mutex<Vec<LatestSeriesCall>>,
        library_series_calls: Mutex<Vec<LibrarySeriesCall>>,
    }

    #[async_trait]
    impl OpdsCatalogPort for TestCatalog {
        async fn load_book_feed_page(
            &self,
            query: OpdsBookFeedQuery<'_>,
        ) -> Result<OpdsPagedBooks, String> {
            self.book_feed_calls
                .lock()
                .expect("test calls lock should not be poisoned")
                .push(BookFeedCall {
                    kind: query.kind,
                    user_id: query.user.user_id.clone(),
                    library_id: query.library_id.map(str::to_string),
                    page: query.page,
                    size: query.size,
                });

            Ok(OpdsPagedBooks {
                books: vec![book(
                    "book-1",
                    query.library_id.unwrap_or("lib-a"),
                    None,
                    &[],
                )],
                total_visible_books: 1,
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
                .push(LatestSeriesCall {
                    include_one_shots: query.include_one_shots,
                    library_id: query.library_id.map(str::to_string),
                    page: query.page,
                    size: query.size,
                });

            Ok(OpdsPagedSeries {
                series: vec![series(
                    "series-1",
                    query.library_id.unwrap_or("lib-a"),
                    false,
                )],
                total_visible_series: 1,
                has_next: false,
            })
        }

        async fn load_library_series_feed_page(
            &self,
            query: OpdsLibrarySeriesQuery<'_>,
        ) -> Result<(Vec<OpdsSeriesEntry>, bool), String> {
            self.library_series_calls
                .lock()
                .expect("test calls lock should not be poisoned")
                .push(LibrarySeriesCall {
                    library_id: query.library_id.to_string(),
                    page: query.page,
                    size: query.size,
                });

            Ok((vec![series("series-1", query.library_id, false)], false))
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

    #[tokio::test]
    async fn latest_books_page_forwards_read_progress_query() {
        let catalog = TestCatalog::default();
        let service = OpdsFeedService::new(&catalog);
        let user = test_user();

        let page = service
            .latest_books_page_with_read_progress(&user, Some("lib-a"), 2, 25)
            .await
            .expect("latest books page should load");

        assert_eq!(page.total_visible_books, 1);
        assert_eq!(
            catalog
                .book_feed_calls
                .lock()
                .expect("test calls lock should not be poisoned")
                .as_slice(),
            &[BookFeedCall {
                kind: OpdsBookFeedKind::LatestBooks {
                    include_read_progress: true,
                },
                user_id: "user-1".to_string(),
                library_id: Some("lib-a".to_string()),
                page: 2,
                size: 25,
            }]
        );
    }

    #[tokio::test]
    async fn keep_reading_page_forwards_query_kind() {
        let catalog = TestCatalog::default();
        let service = OpdsFeedService::new(&catalog);
        let user = test_user();

        service
            .keep_reading_page(&user, None, 1, 10)
            .await
            .expect("latest books page should load");

        assert_eq!(
            catalog
                .book_feed_calls
                .lock()
                .expect("test calls lock should not be poisoned")
                .as_slice(),
            &[BookFeedCall {
                kind: OpdsBookFeedKind::KeepReading,
                user_id: "user-1".to_string(),
                library_id: None,
                page: 1,
                size: 10,
            }]
        );
    }

    #[tokio::test]
    async fn latest_series_page_forwards_one_shot_policy() {
        let catalog = TestCatalog::default();
        let service = OpdsFeedService::new(&catalog);
        let user = test_user();

        service
            .latest_series_page_including_one_shots(&user, None, 0, 10)
            .await
            .expect("latest series page should load");

        assert_eq!(
            catalog
                .latest_series_calls
                .lock()
                .expect("test calls lock should not be poisoned")
                .as_slice(),
            &[LatestSeriesCall {
                include_one_shots: true,
                library_id: None,
                page: 0,
                size: 10,
            }]
        );
    }

    #[tokio::test]
    async fn library_series_page_forwards_library_scope() {
        let catalog = TestCatalog::default();
        let service = OpdsFeedService::new(&catalog);
        let user = test_user();

        let (series, has_next) = service
            .library_series_page(&user, "lib-a", 3, 50)
            .await
            .expect("library series page should load");

        assert_eq!(series.len(), 1);
        assert!(!has_next);
        assert_eq!(
            catalog
                .library_series_calls
                .lock()
                .expect("test calls lock should not be poisoned")
                .as_slice(),
            &[LibrarySeriesCall {
                library_id: "lib-a".to_string(),
                page: 3,
                size: 50,
            }]
        );
    }

    #[test]
    fn feed_user_context_owns_entry_visibility_rules() {
        let user = OpdsFeedUserContext {
            user_id: "user-1".to_string(),
            allowed_library_ids: Some(HashSet::from(["lib-a".to_string()])),
            age: Some(15),
            age_restriction: Some(OpdsAgeRestrictionKind::AllowOnly),
            labels_allow: vec!["kids".to_string()],
            labels_exclude: vec!["adult".to_string()],
        };

        assert!(user.can_access_book_feed_entry(&book(
            "visible-book",
            "lib-a",
            Some(12),
            &["kids"],
        )));
        assert!(!user.can_access_book_feed_entry(&book(
            "blocked-book",
            "lib-b",
            Some(12),
            &["kids"],
        )));
        assert!(user.can_access_series_feed_entry(
            &series_with_rules("visible-series", "lib-a", false, Some(12), &["kids"]),
            false,
        ));
        assert!(!user.can_access_series_feed_entry(
            &series_with_rules("oneshot", "lib-a", true, Some(12), &["kids"]),
            false,
        ));
    }

    fn test_user() -> OpdsFeedUserContext {
        OpdsFeedUserContext {
            user_id: "user-1".to_string(),
            allowed_library_ids: Some(HashSet::from(["lib-a".to_string()])),
            age: None,
            age_restriction: None,
            labels_allow: Vec::new(),
            labels_exclude: Vec::new(),
        }
    }

    fn book(
        id: &str,
        library_id: &str,
        age_rating: Option<u16>,
        sharing_labels: &[&str],
    ) -> OpdsBookFeedEntry {
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
            age_rating,
            sharing_labels: sharing_labels
                .iter()
                .map(|value| value.to_string())
                .collect(),
            last_modified: "2026-01-01T00:00:00Z".to_string(),
            release_date: None,
        }
    }

    fn series(id: &str, library_id: &str, one_shot: bool) -> OpdsSeriesEntry {
        series_with_rules(id, library_id, one_shot, None, &[])
    }

    fn series_with_rules(
        id: &str,
        library_id: &str,
        one_shot: bool,
        age_rating: Option<u16>,
        sharing_labels: &[&str],
    ) -> OpdsSeriesEntry {
        OpdsSeriesEntry {
            id: id.to_string(),
            library_id: library_id.to_string(),
            title: id.to_string(),
            one_shot,
            age_rating,
            sharing_labels: sharing_labels
                .iter()
                .map(|value| value.to_string())
                .collect(),
            last_modified: "2026-01-01T00:00:00Z".to_string(),
        }
    }
}
