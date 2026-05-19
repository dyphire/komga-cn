use komga_application::media_assets::{BookMetadata, BookMetadataAuthor, BookMetadataLink};

#[derive(Default)]
pub(super) struct BookMetadataImportPatch {
    pub title: Option<String>,
    pub summary: Option<String>,
    pub number: Option<String>,
    pub number_sort: Option<f64>,
    pub release_date: Option<String>,
    pub authors: Option<Vec<BookMetadataAuthor>>,
    pub tags: Option<Vec<String>>,
    pub isbn: Option<String>,
    pub links: Option<Vec<BookMetadataLink>>,
}

#[derive(Clone, Debug, Default)]
pub(super) struct SeriesMetadataImportPatch {
    pub title: Option<String>,
    pub title_sort: Option<String>,
    pub status: Option<String>,
    pub summary: Option<String>,
    pub reading_direction: Option<String>,
    pub publisher: Option<String>,
    pub age_rating: Option<u32>,
    pub language: Option<String>,
    pub genres: Option<Vec<String>>,
    pub total_book_count: Option<u32>,
    pub collections: Vec<String>,
}

/// Apply a metadata import patch to existing book metadata, respecting field locks.
/// Returns `true` if any field was changed.
pub(super) fn apply_patch_to_metadata(
    metadata: &mut BookMetadata,
    patch: BookMetadataImportPatch,
) -> bool {
    let mut changed = false;

    if let Some(title) = patch.title
        && !metadata.title_lock
        && metadata.title != title
    {
        metadata.title = title;
        changed = true;
    }

    if let Some(summary) = patch.summary
        && !metadata.summary_lock
        && metadata.summary != summary
    {
        metadata.summary = summary;
        changed = true;
    }

    if let Some(number) = patch.number
        && !metadata.number_lock
        && metadata.number != number
    {
        metadata.number = number;
        changed = true;
    }

    if let Some(number_sort) = patch.number_sort
        && !metadata.number_sort_lock
        && metadata.number_sort != number_sort
    {
        metadata.number_sort = number_sort;
        changed = true;
    }

    if let Some(release_date) = patch.release_date
        && !metadata.release_date_lock
        && metadata.release_date.as_deref() != Some(release_date.as_str())
    {
        metadata.release_date = Some(release_date);
        changed = true;
    }

    if let Some(authors) = patch.authors
        && !metadata.authors_lock
        && metadata.authors != authors
    {
        metadata.authors = authors;
        changed = true;
    }

    if let Some(tags) = patch.tags
        && !metadata.tags_lock
        && metadata.tags != tags
    {
        metadata.tags = tags;
        changed = true;
    }

    if let Some(isbn) = patch.isbn
        && !metadata.isbn_lock
        && metadata.isbn != isbn
    {
        metadata.isbn = isbn;
        changed = true;
    }

    if let Some(links) = patch.links
        && !metadata.links_lock
        && metadata.links != links
    {
        metadata.links = links;
        changed = true;
    }

    changed
}

#[cfg(test)]
mod tests {
    use super::*;

    fn blank_metadata() -> BookMetadata {
        BookMetadata {
            title: String::new(),
            title_lock: false,
            summary: String::new(),
            summary_lock: false,
            number: String::new(),
            number_lock: false,
            number_sort: 0.0,
            number_sort_lock: false,
            release_date: None,
            release_date_lock: false,
            authors: vec![],
            authors_lock: false,
            tags: vec![],
            tags_lock: false,
            isbn: String::new(),
            isbn_lock: false,
            links: vec![],
            links_lock: false,
        }
    }

    #[test]
    fn applies_unlocked_fields() {
        let mut metadata = blank_metadata();
        let patch = BookMetadataImportPatch {
            title: Some("New Title".to_string()),
            isbn: Some("978-0-123456-78-9".to_string()),
            ..Default::default()
        };

        let changed = apply_patch_to_metadata(&mut metadata, patch);

        assert!(changed);
        assert_eq!(metadata.title, "New Title");
        assert_eq!(metadata.isbn, "978-0-123456-78-9");
    }

    #[test]
    fn respects_locked_fields() {
        let mut metadata = blank_metadata();
        metadata.title = "Original".to_string();
        metadata.title_lock = true;

        let patch = BookMetadataImportPatch {
            title: Some("Overwritten".to_string()),
            ..Default::default()
        };

        let changed = apply_patch_to_metadata(&mut metadata, patch);

        assert!(!changed);
        assert_eq!(metadata.title, "Original");
    }

    #[test]
    fn no_change_when_values_match() {
        let mut metadata = blank_metadata();
        metadata.title = "Same".to_string();

        let patch = BookMetadataImportPatch {
            title: Some("Same".to_string()),
            ..Default::default()
        };

        let changed = apply_patch_to_metadata(&mut metadata, patch);

        assert!(!changed);
    }

    #[test]
    fn partial_patch_only_touches_provided_fields() {
        let mut metadata = blank_metadata();
        metadata.title = "Keep This".to_string();
        metadata.summary = "Old Summary".to_string();

        let patch = BookMetadataImportPatch {
            summary: Some("New Summary".to_string()),
            ..Default::default()
        };

        let changed = apply_patch_to_metadata(&mut metadata, patch);

        assert!(changed);
        assert_eq!(metadata.title, "Keep This");
        assert_eq!(metadata.summary, "New Summary");
    }
}
