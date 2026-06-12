use komga_application::discovery::CollectionReadModel;
use komga_domain::discovery::PageEnvelope;
use serde_json::{Value, json};
use time::PrimitiveDateTime;
use time::macros::format_description;

pub(super) fn collections_page_payload(page: PageEnvelope<CollectionReadModel>) -> Value {
    let content = page
        .content
        .iter()
        .map(collection_payload)
        .collect::<Vec<_>>();
    let number_of_elements = content.len();
    let first = page.page == 0;
    let last = page.total_pages == 0 || page.page + 1 >= page.total_pages;
    let offset = page.page.saturating_mul(page.size);

    json!({
        "content": content,
        "pageable": {
            "pageNumber": page.page,
            "pageSize": page.size,
            "sort": {
                "empty": false,
                "sorted": true,
                "unsorted": false
            },
            "offset": offset,
            "paged": true,
            "unpaged": false
        },
        "last": last,
        "totalElements": page.total_elements,
        "totalPages": page.total_pages,
        "first": first,
        "size": page.size,
        "number": page.page,
        "sort": {
            "empty": false,
            "sorted": true,
            "unsorted": false
        },
        "numberOfElements": number_of_elements,
        "empty": number_of_elements == 0
    })
}

pub(super) fn collections_unpaged_payload(content: Vec<CollectionReadModel>) -> Value {
    let total_elements = content.len();
    let content = content.iter().map(collection_payload).collect::<Vec<_>>();

    json!({
        "content": content,
        "pageable": {
            "pageNumber": 0,
            "pageSize": total_elements.max(1),
            "sort": {
                "empty": false,
                "sorted": true,
                "unsorted": false
            },
            "offset": 0,
            "paged": false,
            "unpaged": true
        },
        "last": true,
        "totalElements": total_elements,
        "totalPages": if total_elements == 0 { 0 } else { 1 },
        "first": true,
        "size": total_elements.max(1),
        "number": 0,
        "sort": {
            "empty": false,
            "sorted": true,
            "unsorted": false
        },
        "numberOfElements": total_elements,
        "empty": total_elements == 0
    })
}

pub(super) fn collection_payload(collection: &CollectionReadModel) -> Value {
    json!({
        "id": collection.id,
        "name": collection.name,
        "ordered": collection.ordered,
        "seriesIds": collection.series_ids,
        "createdDate": kotlin_utc_datetime(&collection.created_date),
        "lastModifiedDate": kotlin_utc_datetime(&collection.last_modified_date),
        "filtered": collection.filtered,
    })
}

fn kotlin_utc_datetime(raw: &str) -> String {
    let sqlite_format = format_description!("[year]-[month]-[day] [hour]:[minute]:[second]");
    let kotlin_format = format_description!("[year]-[month]-[day]T[hour]:[minute]:[second]Z");

    let parsed = PrimitiveDateTime::parse(raw, sqlite_format)
        .or_else(|_| PrimitiveDateTime::parse(raw, kotlin_format));

    match parsed {
        Ok(value) => value
            .format(kotlin_format)
            .unwrap_or_else(|_| raw.to_string()),
        Err(_) => raw.to_string(),
    }
}
