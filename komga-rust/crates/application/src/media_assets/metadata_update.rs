use async_trait::async_trait;
use komga_domain::validation::is_valid_isbn13;
use std::fmt;
use url::Url;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BookMetadataAuthor {
    pub name: String,
    pub role: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BookMetadataLink {
    pub label: String,
    pub url: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct BookMetadata {
    pub title: String,
    pub title_lock: bool,
    pub summary: String,
    pub summary_lock: bool,
    pub number: String,
    pub number_lock: bool,
    pub number_sort: f64,
    pub number_sort_lock: bool,
    pub release_date: Option<String>,
    pub release_date_lock: bool,
    pub authors: Vec<BookMetadataAuthor>,
    pub authors_lock: bool,
    pub tags: Vec<String>,
    pub tags_lock: bool,
    pub isbn: String,
    pub isbn_lock: bool,
    pub links: Vec<BookMetadataLink>,
    pub links_lock: bool,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct BookMetadataPatch {
    pub title: Option<String>,
    pub title_lock: Option<bool>,
    pub summary: Option<Option<String>>,
    pub summary_lock: Option<bool>,
    pub number: Option<String>,
    pub number_lock: Option<bool>,
    pub number_sort: Option<f64>,
    pub number_sort_lock: Option<bool>,
    pub release_date: Option<Option<String>>,
    pub release_date_lock: Option<bool>,
    pub authors: Option<Vec<BookMetadataAuthor>>,
    pub authors_lock: Option<bool>,
    pub tags: Option<Vec<String>>,
    pub tags_lock: Option<bool>,
    pub isbn: Option<Option<String>>,
    pub isbn_lock: Option<bool>,
    pub links: Option<Vec<BookMetadataLink>>,
    pub links_lock: Option<bool>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct BookMetadataUpdate {
    pub book_id: String,
    pub patch: BookMetadataPatch,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct BookMetadataBatchUpdateOutcome {
    pub updated_book_ids: Vec<String>,
    pub affected_series_ids: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum BookMetadataUpdateError {
    Validation(String),
    Persistence(String),
}

impl BookMetadataUpdateError {
    pub(crate) fn validation(message: impl Into<String>) -> Self {
        Self::Validation(message.into())
    }

    pub(crate) fn persistence(message: impl Into<String>) -> Self {
        Self::Persistence(message.into())
    }

    pub fn message(&self) -> &str {
        match self {
            Self::Validation(message) | Self::Persistence(message) => message,
        }
    }
}

impl fmt::Display for BookMetadataUpdateError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.message())
    }
}

impl std::error::Error for BookMetadataUpdateError {}

#[async_trait]
pub trait BookMetadataPort: Send + Sync {
    async fn load_book_metadata(&self, book_id: &str) -> Result<Option<BookMetadata>, String>;
    async fn load_book_series_id(&self, book_id: &str) -> Result<Option<String>, String>;
    async fn load_book_library_id(&self, book_id: &str) -> Result<Option<String>, String>;
    async fn persist_book_metadata(
        &self,
        book_id: &str,
        metadata: &BookMetadata,
    ) -> Result<bool, String>;
}

pub struct BookMetadataService {
    port: Box<dyn BookMetadataPort>,
}

impl BookMetadataService {
    pub fn new(port: Box<dyn BookMetadataPort>) -> Self {
        Self { port }
    }

    pub fn port(&self) -> &dyn BookMetadataPort {
        self.port.as_ref()
    }

    pub async fn update_book_metadata(
        &self,
        book_id: &str,
        patch: &BookMetadataPatch,
    ) -> Result<Option<Option<String>>, BookMetadataUpdateError> {
        validate_book_metadata_patch(patch)?;

        let Some(existing) = self
            .port
            .load_book_metadata(book_id)
            .await
            .map_err(BookMetadataUpdateError::persistence)?
        else {
            return Ok(None);
        };
        let series_id = self
            .port
            .load_book_series_id(book_id)
            .await
            .map_err(BookMetadataUpdateError::persistence)?;
        let patched = apply_book_metadata_patch(existing, patch);
        if self
            .port
            .persist_book_metadata(book_id, &patched)
            .await
            .map_err(BookMetadataUpdateError::persistence)?
        {
            Ok(Some(series_id))
        } else {
            Ok(None)
        }
    }

    pub async fn batch_update_book_metadata(
        &self,
        updates: Vec<BookMetadataUpdate>,
    ) -> Result<BookMetadataBatchUpdateOutcome, BookMetadataUpdateError> {
        for update in &updates {
            validate_book_metadata_patch(&update.patch).map_err(|error| {
                if let BookMetadataUpdateError::Validation(message) = error {
                    BookMetadataUpdateError::Validation(format!(
                        "invalid metadata patch for {}: {message}",
                        update.book_id
                    ))
                } else {
                    error
                }
            })?;
        }

        let mut outcome = BookMetadataBatchUpdateOutcome::default();

        for update in updates {
            let book_id = update.book_id;
            let Some(existing) = self
                .port
                .load_book_metadata(&book_id)
                .await
                .map_err(BookMetadataUpdateError::persistence)?
            else {
                continue;
            };
            let series_id = self
                .port
                .load_book_series_id(&book_id)
                .await
                .map_err(BookMetadataUpdateError::persistence)?;

            let patched = apply_book_metadata_patch(existing, &update.patch);
            if self
                .port
                .persist_book_metadata(&book_id, &patched)
                .await
                .map_err(BookMetadataUpdateError::persistence)?
            {
                outcome.updated_book_ids.push(book_id);
                if let Some(series_id) = series_id
                    && !outcome
                        .affected_series_ids
                        .iter()
                        .any(|value| value == &series_id)
                {
                    outcome.affected_series_ids.push(series_id);
                }
            }
        }

        Ok(outcome)
    }
}

fn apply_book_metadata_patch(
    mut existing: BookMetadata,
    patch: &BookMetadataPatch,
) -> BookMetadata {
    if let Some(title) = patch.title.as_deref() {
        existing.title = title.to_string();
    }

    if let Some(title_lock) = patch.title_lock {
        existing.title_lock = title_lock;
    }

    if let Some(summary) = &patch.summary {
        existing.summary = summary.clone().unwrap_or_default();
    }

    if let Some(summary_lock) = patch.summary_lock {
        existing.summary_lock = summary_lock;
    }

    if let Some(number) = patch.number.as_deref() {
        existing.number = number.to_string();
    }

    if let Some(number_lock) = patch.number_lock {
        existing.number_lock = number_lock;
    }

    if let Some(number_sort) = patch.number_sort {
        existing.number_sort = number_sort;
    }

    if let Some(number_sort_lock) = patch.number_sort_lock {
        existing.number_sort_lock = number_sort_lock;
    }

    if let Some(release_date) = &patch.release_date {
        existing.release_date = release_date.clone();
    }

    if let Some(release_date_lock) = patch.release_date_lock {
        existing.release_date_lock = release_date_lock;
    }

    if let Some(authors) = &patch.authors {
        existing.authors = authors.clone();
    }

    if let Some(authors_lock) = patch.authors_lock {
        existing.authors_lock = authors_lock;
    }

    if let Some(tags) = &patch.tags {
        let mut tags = tags.clone();
        tags.sort();
        tags.dedup();
        existing.tags = tags;
    }

    if let Some(tags_lock) = patch.tags_lock {
        existing.tags_lock = tags_lock;
    }

    if let Some(isbn) = &patch.isbn {
        existing.isbn = isbn
            .clone()
            .unwrap_or_default()
            .chars()
            .filter(|value| value.is_ascii_digit())
            .collect();
    }

    if let Some(isbn_lock) = patch.isbn_lock {
        existing.isbn_lock = isbn_lock;
    }

    if let Some(links) = &patch.links {
        existing.links = links.clone();
    }

    if let Some(links_lock) = patch.links_lock {
        existing.links_lock = links_lock;
    }

    existing
}

fn validate_book_metadata_patch(patch: &BookMetadataPatch) -> Result<(), BookMetadataUpdateError> {
    if let Some(title) = patch.title.as_deref()
        && title.trim().is_empty()
    {
        return Err(BookMetadataUpdateError::validation(
            "title must not be blank",
        ));
    }

    if let Some(number) = patch.number.as_deref()
        && number.trim().is_empty()
    {
        return Err(BookMetadataUpdateError::validation(
            "number must not be blank",
        ));
    }

    if let Some(authors) = &patch.authors {
        validate_book_metadata_authors(authors)?;
    }

    if let Some(isbn) = &patch.isbn {
        validate_book_metadata_isbn(isbn.as_deref())?;
    }

    if let Some(links) = &patch.links {
        validate_book_metadata_links(links)?;
    }

    Ok(())
}

fn validate_book_metadata_isbn(value: Option<&str>) -> Result<(), BookMetadataUpdateError> {
    if let Some(value) = value
        && !value.trim().is_empty()
        && !is_valid_isbn13(value)
    {
        return Err(BookMetadataUpdateError::validation(
            "isbn must be null, blank, or a valid ISBN-13",
        ));
    }

    Ok(())
}

fn validate_book_metadata_authors(
    authors: &[BookMetadataAuthor],
) -> Result<(), BookMetadataUpdateError> {
    if authors
        .iter()
        .any(|author| author.name.trim().is_empty() || author.role.trim().is_empty())
    {
        return Err(BookMetadataUpdateError::validation(
            "author name/role must not be blank",
        ));
    }

    Ok(())
}

fn validate_book_metadata_links(links: &[BookMetadataLink]) -> Result<(), BookMetadataUpdateError> {
    for link in links {
        if link.label.trim().is_empty() {
            return Err(BookMetadataUpdateError::validation(
                "links.label must not be blank",
            ));
        }
        if link.url.trim().is_empty() || Url::parse(&link.url).is_err() {
            return Err(BookMetadataUpdateError::validation(
                "links.url must be a valid URL",
            ));
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use async_trait::async_trait;

    use super::{
        BookMetadata, BookMetadataAuthor, BookMetadataLink, BookMetadataPatch, BookMetadataPort,
        BookMetadataService, BookMetadataUpdate, BookMetadataUpdateError,
    };
    use std::sync::{Arc, Mutex};

    #[tokio::test]
    async fn book_metadata_service_rejects_invalid_semantic_patch_values() {
        let cases = [
            (
                BookMetadataPatch {
                    title: Some(" ".to_string()),
                    ..BookMetadataPatch::default()
                },
                "title must not be blank",
            ),
            (
                BookMetadataPatch {
                    number: Some(" ".to_string()),
                    ..BookMetadataPatch::default()
                },
                "number must not be blank",
            ),
            (
                BookMetadataPatch {
                    isbn: Some(Some("123".to_string())),
                    ..BookMetadataPatch::default()
                },
                "isbn must be null, blank, or a valid ISBN-13",
            ),
            (
                BookMetadataPatch {
                    authors: Some(vec![BookMetadataAuthor {
                        name: " ".to_string(),
                        role: "writer".to_string(),
                    }]),
                    ..BookMetadataPatch::default()
                },
                "author name/role must not be blank",
            ),
            (
                BookMetadataPatch {
                    links: Some(vec![BookMetadataLink {
                        label: " ".to_string(),
                        url: "https://example.com".to_string(),
                    }]),
                    ..BookMetadataPatch::default()
                },
                "links.label must not be blank",
            ),
            (
                BookMetadataPatch {
                    links: Some(vec![BookMetadataLink {
                        label: "Publisher".to_string(),
                        url: "not a url".to_string(),
                    }]),
                    ..BookMetadataPatch::default()
                },
                "links.url must be a valid URL",
            ),
        ];

        for (patch, expected_error) in cases {
            let service = BookMetadataService::new(Box::new(TestBookMetadataPort::with_metadata(
                Some(sample_metadata()),
            )));
            let error = service
                .update_book_metadata("book-1", &patch)
                .await
                .expect_err("invalid metadata patch should be rejected by application");

            assert_eq!(
                error,
                BookMetadataUpdateError::Validation(expected_error.to_string()),
            );
        }
    }

    #[tokio::test]
    async fn book_metadata_service_rejects_invalid_patch_before_missing_book_short_circuit() {
        let service = BookMetadataService::new(Box::new(TestBookMetadataPort::with_metadata(None)));

        let error = service
            .update_book_metadata(
                "missing-book",
                &BookMetadataPatch {
                    title: Some(" ".to_string()),
                    ..BookMetadataPatch::default()
                },
            )
            .await
            .expect_err("invalid patch should be rejected before missing-book handling");

        assert_eq!(
            error,
            BookMetadataUpdateError::Validation("title must not be blank".to_string()),
        );
    }

    #[tokio::test]
    async fn book_metadata_service_validates_entire_batch_before_persisting_any_book() {
        let port = TestBookMetadataPort::with_metadata(Some(sample_metadata()));
        let persisted_book_ids = port.persisted_book_ids.clone();
        let service = BookMetadataService::new(Box::new(port));

        let error = service
            .batch_update_book_metadata(vec![
                BookMetadataUpdate {
                    book_id: "book-1".to_string(),
                    patch: BookMetadataPatch {
                        title: Some("Updated".to_string()),
                        ..BookMetadataPatch::default()
                    },
                },
                BookMetadataUpdate {
                    book_id: "book-2".to_string(),
                    patch: BookMetadataPatch {
                        title: Some(" ".to_string()),
                        ..BookMetadataPatch::default()
                    },
                },
            ])
            .await
            .expect_err("invalid batch patch should reject the whole batch");

        assert_eq!(
            error,
            BookMetadataUpdateError::Validation(
                "invalid metadata patch for book-2: title must not be blank".to_string(),
            ),
        );
        assert!(
            persisted_book_ids.lock().unwrap().is_empty(),
            "batch validation must happen before any persistence side effect",
        );
    }

    struct TestBookMetadataPort {
        metadata: Option<BookMetadata>,
        persisted_book_ids: Arc<Mutex<Vec<String>>>,
    }

    impl TestBookMetadataPort {
        fn with_metadata(metadata: Option<BookMetadata>) -> Self {
            Self {
                metadata,
                persisted_book_ids: Default::default(),
            }
        }
    }

    #[async_trait]
    impl BookMetadataPort for TestBookMetadataPort {
        async fn load_book_metadata(&self, _book_id: &str) -> Result<Option<BookMetadata>, String> {
            Ok(self.metadata.clone())
        }

        async fn load_book_series_id(&self, _book_id: &str) -> Result<Option<String>, String> {
            Ok(Some("series-1".to_string()))
        }

        async fn load_book_library_id(&self, _book_id: &str) -> Result<Option<String>, String> {
            Ok(Some("library-1".to_string()))
        }

        async fn persist_book_metadata(
            &self,
            book_id: &str,
            _metadata: &BookMetadata,
        ) -> Result<bool, String> {
            self.persisted_book_ids
                .lock()
                .unwrap()
                .push(book_id.to_string());
            Ok(true)
        }
    }

    fn sample_metadata() -> BookMetadata {
        BookMetadata {
            title: "Book 1".to_string(),
            title_lock: false,
            summary: String::new(),
            summary_lock: false,
            number: "1".to_string(),
            number_lock: false,
            number_sort: 1.0,
            number_sort_lock: false,
            release_date: None,
            release_date_lock: false,
            authors: Vec::new(),
            authors_lock: false,
            tags: Vec::new(),
            tags_lock: false,
            isbn: String::new(),
            isbn_lock: false,
            links: Vec::new(),
            links_lock: false,
        }
    }
}
