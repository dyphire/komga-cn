use async_trait::async_trait;

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
    ) -> Result<Option<Option<String>>, String> {
        let Some(existing) = self.port.load_book_metadata(book_id).await? else {
            return Ok(None);
        };
        let series_id = self.port.load_book_series_id(book_id).await?;
        let patched = apply_book_metadata_patch(existing, patch)?;
        if self.port.persist_book_metadata(book_id, &patched).await? {
            Ok(Some(series_id))
        } else {
            Ok(None)
        }
    }

    pub async fn batch_update_book_metadata(
        &self,
        updates: Vec<(String, BookMetadataPatch)>,
    ) -> Result<Vec<String>, String> {
        let mut affected_series_ids = Vec::new();

        for (book_id, patch) in updates {
            let Some(existing) = self.port.load_book_metadata(&book_id).await? else {
                continue;
            };
            if let Some(series_id) = self.port.load_book_series_id(&book_id).await?
                && !affected_series_ids.iter().any(|value| value == &series_id)
            {
                affected_series_ids.push(series_id);
            }

            let patched = apply_book_metadata_patch(existing, &patch)?;
            let _ = self.port.persist_book_metadata(&book_id, &patched).await?;
        }

        Ok(affected_series_ids)
    }
}

pub fn apply_book_metadata_patch(
    mut existing: BookMetadata,
    patch: &BookMetadataPatch,
) -> Result<BookMetadata, String> {
    if let Some(title) = patch.title.as_deref() {
        if title.trim().is_empty() {
            return Err("title must not be blank".to_string());
        }
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
        if number.trim().is_empty() {
            return Err("number must not be blank".to_string());
        }
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

    Ok(existing)
}
