use serde_json::Value;

use super::models::KoboSyncSnapshot;
use super::wire::{
    build_kobo_changed_entitlement_removed, build_kobo_changed_product_metadata,
    build_kobo_changed_reading_state, build_kobo_changed_tag, build_kobo_deleted_tag,
    build_kobo_new_entitlement, build_kobo_new_tag,
};

pub fn build_kobo_sync_events(
    from: Option<&KoboSyncSnapshot>,
    to: &KoboSyncSnapshot,
    base_url: &str,
    auth_token: &str,
) -> Vec<Value> {
    let mut events = Vec::new();

    match from {
        None => {
            let mut books = to.books.values().collect::<Vec<_>>();
            books.sort_by(|a, b| a.id.cmp(&b.id));
            for book in books {
                events.push(build_kobo_new_entitlement(
                    book,
                    to.progress.get(&book.id),
                    base_url,
                    auth_token,
                ));
            }

            let mut readlists = to.readlists.values().collect::<Vec<_>>();
            readlists.sort_by(|a, b| a.id.cmp(&b.id));
            for readlist in readlists {
                events.push(build_kobo_new_tag(readlist));
            }
        }
        Some(from) => {
            let mut to_book_ids = to.books.keys().cloned().collect::<Vec<_>>();
            to_book_ids.sort();
            for book_id in to_book_ids {
                let Some(to_book) = to.books.get(&book_id) else {
                    continue;
                };
                match from.books.get(&book_id) {
                    None => {
                        events.push(build_kobo_new_entitlement(
                            to_book,
                            to.progress.get(&book_id),
                            base_url,
                            auth_token,
                        ));
                    }
                    Some(from_book) => {
                        if from_book.last_modified != to_book.last_modified {
                            events.push(build_kobo_new_entitlement(
                                to_book,
                                to.progress.get(&book_id),
                                base_url,
                                auth_token,
                            ));
                            events.push(build_kobo_changed_product_metadata(
                                to_book, base_url, auth_token,
                            ));
                            if let Some(to_progress) = to.progress.get(&book_id) {
                                events.push(build_kobo_changed_reading_state(to_book, to_progress));
                            }
                        }
                    }
                }
            }

            let mut removed_book_ids = from.books.keys().cloned().collect::<Vec<_>>();
            removed_book_ids.sort();
            for book_id in removed_book_ids {
                if to.books.contains_key(&book_id) {
                    continue;
                }
                if let Some(from_book) = from.books.get(&book_id) {
                    events.push(build_kobo_changed_entitlement_removed(
                        from_book, base_url, auth_token,
                    ));
                }
            }

            let mut progress_book_ids = to
                .progress
                .keys()
                .chain(from.progress.keys())
                .cloned()
                .collect::<Vec<_>>();
            progress_book_ids.sort();
            progress_book_ids.dedup();
            for book_id in progress_book_ids {
                let from_progress = from.progress.get(&book_id);
                let to_progress = to.progress.get(&book_id);
                if from_progress.map(|value| {
                    (
                        &value.last_modified,
                        value.page,
                        value.completed,
                        value.locator.as_ref(),
                    )
                }) == to_progress.map(|value| {
                    (
                        &value.last_modified,
                        value.page,
                        value.completed,
                        value.locator.as_ref(),
                    )
                }) {
                    continue;
                }
                if let Some(book) = to.books.get(&book_id)
                    && let Some(progress) = to_progress
                {
                    events.push(build_kobo_changed_reading_state(book, progress));
                }
            }

            let mut to_readlist_ids = to.readlists.keys().cloned().collect::<Vec<_>>();
            to_readlist_ids.sort();
            for readlist_id in to_readlist_ids {
                let Some(to_readlist) = to.readlists.get(&readlist_id) else {
                    continue;
                };
                match from.readlists.get(&readlist_id) {
                    None => events.push(build_kobo_new_tag(to_readlist)),
                    Some(from_readlist)
                        if from_readlist.last_modified != to_readlist.last_modified
                            || from_readlist.name != to_readlist.name
                            || from_readlist.items != to_readlist.items =>
                    {
                        events.push(build_kobo_changed_tag(to_readlist));
                    }
                    Some(_) => {}
                }
            }

            let mut removed_readlists = from.readlists.keys().cloned().collect::<Vec<_>>();
            removed_readlists.sort();
            for readlist_id in removed_readlists {
                if to.readlists.contains_key(&readlist_id) {
                    continue;
                }
                let Some(previous) = from.readlists.get(&readlist_id) else {
                    continue;
                };
                events.push(build_kobo_deleted_tag(previous));
            }
        }
    }

    events
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use serde_json::{Value, json};

    use super::super::models::{
        KoboSyncBookSnapshot, KoboSyncReadListSnapshot, KoboSyncReadProgressSnapshot,
    };
    use super::*;

    #[test]
    fn initial_sync_emits_entitlements_before_tags() {
        let to = KoboSyncSnapshot {
            books: HashMap::from([("book-1".to_string(), book("book-1", "Book One"))]),
            progress: HashMap::from([(
                "book-1".to_string(),
                KoboSyncReadProgressSnapshot {
                    page: 4,
                    completed: false,
                    created: "2026-01-01T00:00:00Z".to_string(),
                    last_modified: "2026-01-03T00:00:00Z".to_string(),
                    locator: Some(
                        json!({
                            "href": "/chapter-1.xhtml",
                            "koboSpan": "kobo.1.1",
                            "locations": {
                                "progression": 0.2,
                                "totalProgression": 0.4,
                            }
                        })
                        .to_string()
                        .into_bytes(),
                    ),
                },
            )]),
            readlists: HashMap::from([(
                "list-1".to_string(),
                KoboSyncReadListSnapshot {
                    id: "list-1".to_string(),
                    name: "On Deck".to_string(),
                    created: "2026-01-01T00:00:00Z".to_string(),
                    last_modified: "2026-01-03T00:00:00Z".to_string(),
                    items: vec!["book-1".to_string()],
                },
            )]),
        };

        let events = build_kobo_sync_events(None, &to, "http://localhost:8080", "token-1");

        assert_eq!(events.len(), 2);
        let entitlement = events[0]
            .get("NewEntitlement")
            .expect("new entitlement expected");
        assert_eq!(
            entitlement
                .get("BookMetadata")
                .and_then(|value| value.get("DownloadUrls"))
                .and_then(Value::as_array)
                .and_then(|items| items.first())
                .and_then(|item| item.get("Url")),
            Some(&Value::String(
                "http://localhost:8080/kobo/token-1/v1/books/book-1/file/epub".to_string()
            ))
        );
        assert_eq!(
            entitlement
                .get("ReadingState")
                .and_then(|value| value.get("CurrentBookmark"))
                .and_then(|value| value.get("Location"))
                .and_then(|value| value.get("Source")),
            Some(&Value::String("/chapter-1.xhtml".to_string()))
        );

        let tag = events[1].get("NewTag").expect("new tag expected");
        assert_eq!(
            tag.get("Tag")
                .and_then(|value| value.get("Items"))
                .and_then(Value::as_array)
                .and_then(|items| items.first())
                .and_then(|item| item.get("RevisionId"))
                .and_then(Value::as_str),
            Some("book-1")
        );
    }

    #[test]
    fn incremental_sync_emits_added_changed_progress_removed_and_deleted_tag_events() {
        let from = KoboSyncSnapshot {
            books: HashMap::from([("book-1".to_string(), book("book-1", "Old"))]),
            progress: HashMap::new(),
            readlists: HashMap::from([(
                "list-1".to_string(),
                KoboSyncReadListSnapshot {
                    id: "list-1".to_string(),
                    name: "List One".to_string(),
                    created: "2026-01-01T00:00:00Z".to_string(),
                    last_modified: "2026-01-01T00:00:00Z".to_string(),
                    items: vec!["book-1".to_string()],
                },
            )]),
        };
        let to = KoboSyncSnapshot {
            books: HashMap::from([("book-2".to_string(), book("book-2", "New"))]),
            progress: HashMap::from([(
                "book-2".to_string(),
                KoboSyncReadProgressSnapshot {
                    page: 5,
                    completed: false,
                    created: "2026-01-02T00:00:00Z".to_string(),
                    last_modified: "2026-01-03T00:00:00Z".to_string(),
                    locator: None,
                },
            )]),
            readlists: HashMap::new(),
        };

        let events = build_kobo_sync_events(Some(&from), &to, "http://localhost:8080", "token-1");

        assert!(
            events
                .iter()
                .any(|event| event.get("NewEntitlement").is_some())
        );
        assert!(
            events
                .iter()
                .any(|event| event.get("ChangedEntitlement").is_some())
        );
        assert!(
            events
                .iter()
                .any(|event| event.get("ChangedReadingState").is_some())
        );
        assert!(events.iter().any(|event| event.get("DeletedTag").is_some()));

        let removed = events
            .iter()
            .find_map(|event| event.get("ChangedEntitlement"))
            .expect("removed entitlement expected");
        assert_eq!(
            removed
                .get("BookEntitlement")
                .and_then(|value| value.get("IsRemoved")),
            Some(&Value::Bool(true))
        );
    }

    fn book(id: &str, title: &str) -> KoboSyncBookSnapshot {
        KoboSyncBookSnapshot {
            id: id.to_string(),
            title: title.to_string(),
            summary: String::new(),
            release_date: None,
            language: "EN".to_string(),
            file_size: 123,
            page_count: 10,
            created: "2026-01-01T00:00:00Z".to_string(),
            last_modified: "2026-01-02T00:00:00Z".to_string(),
            contributor_names: vec!["Jane Writer".to_string()],
            isbn: Some("9781234567890".to_string()),
            publisher_name: Some("PubHouse".to_string()),
            cover_image_id: Some("thumb-book-1".to_string()),
            series_id: Some("series-1".to_string()),
            series_name: Some("Series 1".to_string()),
            series_number: Some("1".to_string()),
            series_number_float: Some(1.0),
            oneshot: false,
        }
    }
}
