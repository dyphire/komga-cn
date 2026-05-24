use std::collections::HashSet;

use komga_domain::discovery::{
    AgeRestrictionKind as DomainAgeRestrictionKind, QueryRestrictions,
    content_allowed_by_restrictions,
};

use crate::identity_access::{
    AuthUser, user_id, user_shared_all_libraries, user_shared_library_ids,
};
use crate::opds::{OpdsBookFeedEntry, OpdsCatalogPort, OpdsSeriesEntry};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OpdsAgeRestrictionKind {
    AllowOnly,
    Exclude,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OpdsFeedUserContext {
    pub user_id: String,
    pub allowed_library_ids: Option<HashSet<String>>,
    pub age: Option<u16>,
    pub age_restriction: Option<OpdsAgeRestrictionKind>,
    pub labels_allow: Vec<String>,
    pub labels_exclude: Vec<String>,
}

impl OpdsFeedUserContext {
    pub fn from_auth_user(user: &AuthUser) -> Self {
        let allowed_library_ids = if user_shared_all_libraries(user) {
            None
        } else {
            Some(
                user_shared_library_ids(user)
                    .iter()
                    .cloned()
                    .collect::<HashSet<_>>(),
            )
        };
        let age = user
            .age_restriction
            .as_ref()
            .and_then(|restriction| u16::try_from(restriction.age).ok());
        let age_restriction =
            user.age_restriction.as_ref().and_then(|restriction| {
                match restriction.restriction.trim().to_ascii_uppercase().as_str() {
                    "ALLOW_ONLY" => Some(OpdsAgeRestrictionKind::AllowOnly),
                    "EXCLUDE" => Some(OpdsAgeRestrictionKind::Exclude),
                    _ => None,
                }
            });

        Self {
            user_id: user_id(user).to_string(),
            allowed_library_ids,
            age,
            age_restriction,
            labels_allow: normalized_labels(&user.labels_allow),
            labels_exclude: normalized_labels(&user.labels_exclude),
        }
    }

    pub fn can_access_library(&self, library_id: &str) -> bool {
        match &self.allowed_library_ids {
            None => true,
            Some(ids) => ids.contains(library_id),
        }
    }

    pub fn content_allowed(&self, age_rating: Option<u16>, sharing_labels: &[String]) -> bool {
        let restrictions = QueryRestrictions {
            age: self.age,
            age_restriction: self.age_restriction.map(|kind| match kind {
                OpdsAgeRestrictionKind::AllowOnly => DomainAgeRestrictionKind::AllowOnly,
                OpdsAgeRestrictionKind::Exclude => DomainAgeRestrictionKind::Exclude,
            }),
            labels_allow: self.labels_allow.clone(),
            labels_exclude: self.labels_exclude.clone(),
        };
        content_allowed_by_restrictions(&restrictions, age_rating, sharing_labels)
    }

    pub fn allowed_library_ids(&self) -> Option<&HashSet<String>> {
        self.allowed_library_ids.as_ref()
    }
}

pub struct OpdsPagedBooks {
    pub books: Vec<OpdsBookFeedEntry>,
    pub total_visible_books: usize,
    pub has_next: bool,
}

pub struct OpdsPagedSeries {
    pub series: Vec<OpdsSeriesEntry>,
    pub total_visible_series: usize,
    pub has_next: bool,
}

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
        let books = self
            .catalog
            .load_keep_reading_books(&user.user_id, library_id)
            .await?;
        Ok(paged_books(visible_books(user, books), page, size))
    }

    pub async fn on_deck_page(
        &self,
        user: &OpdsFeedUserContext,
        library_id: Option<&str>,
        page: usize,
        size: usize,
    ) -> Result<OpdsPagedBooks, String> {
        let books = self
            .catalog
            .load_on_deck_books(&user.user_id, library_id)
            .await?;
        Ok(paged_books(visible_books(user, books), page, size))
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
        let scan_limit = size.max(100) as i64;
        let start = page.saturating_mul(size);
        let end = start.saturating_add(size);
        let mut offset = 0_i64;
        let mut total_visible_books = 0_usize;
        let mut visible_page = Vec::new();

        loop {
            let batch = self
                .catalog
                .load_latest_books_paged(
                    user.allowed_library_ids(),
                    include_read_progress.then_some(user.user_id.as_str()),
                    library_id,
                    offset,
                    scan_limit,
                )
                .await?;
            if batch.is_empty() {
                break;
            }

            let batch_len = batch.len();
            for book in batch {
                if book_is_visible(user, &book) {
                    if total_visible_books >= start && total_visible_books < end {
                        visible_page.push(book);
                    }
                    total_visible_books += 1;
                }
            }

            if batch_len < scan_limit as usize {
                break;
            }
            offset += batch_len as i64;
        }

        Ok(OpdsPagedBooks {
            books: visible_page,
            total_visible_books,
            has_next: end < total_visible_books,
        })
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

    async fn load_latest_series_page(
        &self,
        user: &OpdsFeedUserContext,
        library_id: Option<&str>,
        page: usize,
        size: usize,
        include_one_shots: bool,
    ) -> Result<OpdsPagedSeries, String> {
        let scan_limit = size.max(100) as i64;
        let start = page.saturating_mul(size);
        let end = start.saturating_add(size);
        let mut offset = 0_i64;
        let mut total_visible_series = 0_usize;
        let mut visible_page = Vec::new();

        loop {
            let batch = self
                .catalog
                .load_latest_series_paged(
                    user.allowed_library_ids(),
                    library_id,
                    offset,
                    scan_limit,
                )
                .await?;
            if batch.is_empty() {
                break;
            }

            let batch_len = batch.len();
            for series in batch {
                if series_is_visible(user, &series, include_one_shots) {
                    if total_visible_series >= start && total_visible_series < end {
                        visible_page.push(series);
                    }
                    total_visible_series += 1;
                }
            }

            if batch_len < scan_limit as usize {
                break;
            }
            offset += batch_len as i64;
        }

        Ok(OpdsPagedSeries {
            series: visible_page,
            total_visible_series,
            has_next: end < total_visible_series,
        })
    }
}

fn visible_books(
    user: &OpdsFeedUserContext,
    books: Vec<OpdsBookFeedEntry>,
) -> Vec<OpdsBookFeedEntry> {
    books
        .into_iter()
        .filter(|book| book_is_visible(user, book))
        .collect()
}

fn paged_books(books: Vec<OpdsBookFeedEntry>, page: usize, size: usize) -> OpdsPagedBooks {
    let total_visible_books = books.len();
    let start = page.saturating_mul(size);
    let end = start.saturating_add(size);
    OpdsPagedBooks {
        books: books.into_iter().skip(start).take(size).collect(),
        total_visible_books,
        has_next: end < total_visible_books,
    }
}

fn book_is_visible(user: &OpdsFeedUserContext, book: &OpdsBookFeedEntry) -> bool {
    user.can_access_library(&book.library_id)
        && user.content_allowed(book.age_rating, &book.sharing_labels)
}

fn series_is_visible(
    user: &OpdsFeedUserContext,
    series: &OpdsSeriesEntry,
    include_one_shots: bool,
) -> bool {
    (include_one_shots || !series.one_shot)
        && user.can_access_library(&series.library_id)
        && user.content_allowed(series.age_rating, &series.sharing_labels)
}

fn normalized_labels(labels: &[String]) -> Vec<String> {
    labels
        .iter()
        .map(|label| label.trim())
        .filter(|label| !label.is_empty())
        .map(str::to_ascii_lowercase)
        .collect()
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;
    use std::sync::Mutex;

    use async_trait::async_trait;

    use super::*;
    use crate::opds::{
        BrowsePublisherEntry, BrowseSeriesNavigationEntry, OpdsBookFeedEntry, OpdsCatalogPort,
        OpdsSeriesEntry,
    };

    type LatestBooksCall = (Option<String>, Option<String>, i64, i64);

    #[derive(Default)]
    struct TestCatalog {
        latest_books: Mutex<Vec<OpdsBookFeedEntry>>,
        latest_series: Mutex<Vec<OpdsSeriesEntry>>,
        latest_books_calls: Mutex<Vec<LatestBooksCall>>,
        latest_series_calls: Mutex<Vec<(i64, i64)>>,
    }

    #[async_trait]
    impl OpdsCatalogPort for TestCatalog {
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

        async fn load_keep_reading_books(
            &self,
            _user_id: &str,
            _library_id: Option<&str>,
        ) -> Result<Vec<OpdsBookFeedEntry>, String> {
            unimplemented!()
        }

        async fn load_on_deck_books(
            &self,
            _user_id: &str,
            _library_id: Option<&str>,
        ) -> Result<Vec<OpdsBookFeedEntry>, String> {
            unimplemented!()
        }

        async fn load_latest_books(
            &self,
            _library_id: Option<&str>,
            _limit: i64,
        ) -> Result<Vec<OpdsBookFeedEntry>, String> {
            unimplemented!()
        }

        async fn load_latest_books_paged(
            &self,
            allowed_library_ids: Option<&HashSet<String>>,
            user_id: Option<&str>,
            library_id: Option<&str>,
            offset: i64,
            limit: i64,
        ) -> Result<Vec<OpdsBookFeedEntry>, String> {
            assert_eq!(
                allowed_library_ids.map(|ids| ids.iter().cloned().collect::<Vec<_>>()),
                Some(vec!["lib-a".to_string()])
            );
            assert_eq!(library_id, Some("lib-a"));
            self.latest_books_calls
                .lock()
                .expect("test calls lock should not be poisoned")
                .push((
                    user_id.map(str::to_string),
                    library_id.map(str::to_string),
                    offset,
                    limit,
                ));

            Ok(take_page(&self.latest_books, offset, limit))
        }

        async fn load_latest_series(
            &self,
            _library_id: Option<&str>,
            _limit: i64,
        ) -> Result<Vec<OpdsSeriesEntry>, String> {
            unimplemented!()
        }

        async fn load_latest_series_paged(
            &self,
            allowed_library_ids: Option<&HashSet<String>>,
            library_id: Option<&str>,
            offset: i64,
            limit: i64,
        ) -> Result<Vec<OpdsSeriesEntry>, String> {
            assert!(allowed_library_ids.is_some());
            assert_eq!(library_id, None);
            self.latest_series_calls
                .lock()
                .expect("test calls lock should not be poisoned")
                .push((offset, limit));

            Ok(take_page(&self.latest_series, offset, limit))
        }

        async fn load_library_series(
            &self,
            _library_id: &str,
            _offset: i64,
            _limit: i64,
        ) -> Result<Vec<OpdsSeriesEntry>, String> {
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
    async fn latest_books_page_filters_restrictions_and_scans_until_total_is_known() {
        let catalog = TestCatalog {
            latest_books: Mutex::new(vec![
                book("visible-1", "lib-a", Some(12), &["kids"]),
                book("blocked-label", "lib-a", Some(12), &["adult"]),
                book("blocked-library", "lib-b", Some(12), &["kids"]),
                book("visible-2", "lib-a", Some(12), &["kids"]),
            ]),
            ..Default::default()
        };
        let service = OpdsFeedService::new(&catalog);
        let user = OpdsFeedUserContext {
            user_id: "user-1".to_string(),
            allowed_library_ids: Some(HashSet::from(["lib-a".to_string()])),
            age: Some(15),
            age_restriction: Some(OpdsAgeRestrictionKind::AllowOnly),
            labels_allow: vec!["kids".to_string()],
            labels_exclude: vec!["adult".to_string()],
        };

        let page = service
            .latest_books_page(&user, Some("lib-a"), 0, 2)
            .await
            .expect("latest books page should load");

        assert_eq!(
            page.books
                .iter()
                .map(|book| book.id.as_str())
                .collect::<Vec<_>>(),
            vec!["visible-1", "visible-2"]
        );
        assert_eq!(page.total_visible_books, 2);
        assert_eq!(
            catalog
                .latest_books_calls
                .lock()
                .expect("test calls lock should not be poisoned")
                .as_slice(),
            &[(None, Some("lib-a".to_string()), 0, 100)]
        );
    }

    #[tokio::test]
    async fn latest_books_page_can_include_read_progress_user_context() {
        let catalog = TestCatalog {
            latest_books: Mutex::new(vec![book("visible-1", "lib-a", Some(12), &["kids"])]),
            ..Default::default()
        };
        let service = OpdsFeedService::new(&catalog);
        let user = OpdsFeedUserContext {
            user_id: "user-1".to_string(),
            allowed_library_ids: Some(HashSet::from(["lib-a".to_string()])),
            age: None,
            age_restriction: None,
            labels_allow: Vec::new(),
            labels_exclude: Vec::new(),
        };

        let page = service
            .latest_books_page_with_read_progress(&user, Some("lib-a"), 0, 10)
            .await
            .expect("latest books page should load");

        assert_eq!(
            page.books
                .iter()
                .map(|book| book.id.as_str())
                .collect::<Vec<_>>(),
            vec!["visible-1"]
        );
        assert_eq!(
            catalog
                .latest_books_calls
                .lock()
                .expect("test calls lock should not be poisoned")
                .as_slice(),
            &[(
                Some("user-1".to_string()),
                Some("lib-a".to_string()),
                0,
                100
            )]
        );
    }

    #[tokio::test]
    async fn latest_series_page_hides_oneshots_and_reports_has_next() {
        let catalog = TestCatalog {
            latest_series: Mutex::new(vec![
                series("series-1", "lib-a", false),
                series("oneshot", "lib-a", true),
                series("series-2", "lib-a", false),
            ]),
            ..Default::default()
        };
        let service = OpdsFeedService::new(&catalog);
        let user = OpdsFeedUserContext {
            user_id: "user-1".to_string(),
            allowed_library_ids: Some(HashSet::from(["lib-a".to_string()])),
            age: None,
            age_restriction: None,
            labels_allow: Vec::new(),
            labels_exclude: Vec::new(),
        };

        let page = service
            .latest_series_page(&user, None, 0, 1)
            .await
            .expect("latest series page should load");

        assert_eq!(
            page.series
                .iter()
                .map(|series| series.id.as_str())
                .collect::<Vec<_>>(),
            vec!["series-1"]
        );
        assert_eq!(page.total_visible_series, 2);
        assert!(page.has_next);
    }

    #[tokio::test]
    async fn latest_series_page_can_preserve_v1_oneshot_visibility() {
        let catalog = TestCatalog {
            latest_series: Mutex::new(vec![
                series("series-1", "lib-a", false),
                series("oneshot", "lib-a", true),
            ]),
            ..Default::default()
        };
        let service = OpdsFeedService::new(&catalog);
        let user = OpdsFeedUserContext {
            user_id: "user-1".to_string(),
            allowed_library_ids: Some(HashSet::from(["lib-a".to_string()])),
            age: None,
            age_restriction: None,
            labels_allow: Vec::new(),
            labels_exclude: Vec::new(),
        };

        let page = service
            .latest_series_page_including_one_shots(&user, None, 0, 10)
            .await
            .expect("latest series page should load");

        assert_eq!(
            page.series
                .iter()
                .map(|series| series.id.as_str())
                .collect::<Vec<_>>(),
            vec!["series-1", "oneshot"]
        );
        assert_eq!(page.total_visible_series, 2);
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
        OpdsSeriesEntry {
            id: id.to_string(),
            library_id: library_id.to_string(),
            title: id.to_string(),
            one_shot,
            age_rating: None,
            sharing_labels: Vec::new(),
            last_modified: "2026-01-01T00:00:00Z".to_string(),
        }
    }

    fn take_page<T>(items: &Mutex<Vec<T>>, offset: i64, limit: i64) -> Vec<T> {
        let start = usize::try_from(offset).expect("test offset should be non-negative");
        let limit = usize::try_from(limit).expect("test limit should be non-negative");
        let mut guard = items.lock().expect("test rows lock should not be poisoned");
        if start >= guard.len() {
            return Vec::new();
        }
        let end = start.saturating_add(limit).min(guard.len());
        guard.drain(start..end).collect()
    }
}
