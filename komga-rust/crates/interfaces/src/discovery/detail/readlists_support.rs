use crate::helpers::normalized_date_time;
use komga_application::discovery::{
    ComicRackReadListMatchError, ComicRackReadListMatchResult, ReadListReadModel,
    ReadlistMutationInput,
};
use komga_domain::discovery::PageEnvelope;
use serde_json::{Value, json};

pub(super) fn merge_readlist_write_input(
    existing: &ReadListReadModel,
    payload: &Value,
) -> ReadlistMutationInput {
    let name = payload
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or(existing.name.as_str())
        .to_string();
    let summary = payload
        .get("summary")
        .and_then(Value::as_str)
        .unwrap_or(existing.summary.as_str())
        .to_string();
    let ordered = payload
        .get("ordered")
        .and_then(Value::as_bool)
        .unwrap_or(existing.ordered);
    let book_ids = payload
        .get("bookIds")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect::<Vec<_>>()
        })
        .unwrap_or_else(|| existing.book_ids.clone());

    ReadlistMutationInput {
        name,
        summary,
        ordered,
        book_ids,
    }
}

pub(super) fn comicrack_match_payload(result: &ComicRackReadListMatchResult) -> Value {
    json!({
        "readListMatch": {
            "name": result.name,
            "errorCode": result.error.map(comicrack_match_error_code).unwrap_or(""),
        },
        "requests": result.requests.iter().map(|request_match| {
            json!({
                "request": {
                    "series": &request_match.request.series_candidates,
                    "number": &request_match.request.number,
                },
                "matches": request_match.matches.iter().map(|group| {
                    json!({
                        "series": {
                            "seriesId": &group.series.series_id,
                            "title": &group.series.title,
                            "releaseDate": &group.series.release_date,
                        },
                        "books": group.books.iter().map(|candidate| {
                            json!({
                                "bookId": &candidate.book_id,
                                "number": &candidate.number,
                                "title": &candidate.title,
                            })
                        }).collect::<Vec<_>>(),
                    })
                }).collect::<Vec<_>>(),
            })
        }).collect::<Vec<_>>(),
        "errorCode": "",
    })
}

fn comicrack_match_error_code(error: ComicRackReadListMatchError) -> &'static str {
    error.error_code()
}

pub(super) fn readlists_page_payload(page: PageEnvelope<ReadListReadModel>) -> Value {
    let content = page
        .content
        .iter()
        .map(readlist_payload)
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

pub(super) fn readlist_payload(readlist: &ReadListReadModel) -> Value {
    json!({
        "id": readlist.id,
        "name": readlist.name,
        "summary": readlist.summary,
        "ordered": readlist.ordered,
        "bookIds": readlist.book_ids,
        "createdDate": normalized_date_time(&readlist.created_date),
        "lastModifiedDate": normalized_date_time(&readlist.last_modified_date),
        "filtered": readlist.filtered,
    })
}

#[cfg(test)]
mod tests {
    use komga_application::discovery::{
        ComicRackMatchBook, ComicRackMatchSeries, ComicRackReadListMatchError,
        ComicRackReadListMatchGroup, ComicRackReadListMatchResult, ComicRackReadListRequestBook,
        ComicRackReadListRequestMatch,
    };
    use serde_json::json;

    #[test]
    fn comicrack_match_payload_serializes_matches_and_error_codes() {
        let payload = super::comicrack_match_payload(&ComicRackReadListMatchResult {
            name: "ReadList 1".to_string(),
            error: Some(ComicRackReadListMatchError::DuplicateName),
            requests: vec![ComicRackReadListRequestMatch {
                request: ComicRackReadListRequestBook {
                    series_candidates: vec!["Series 1".to_string()],
                    number: "1".to_string(),
                },
                matches: vec![ComicRackReadListMatchGroup {
                    series: ComicRackMatchSeries {
                        series_id: "series-1".to_string(),
                        title: "Series 1".to_string(),
                        release_date: Some("2024-01-15".to_string()),
                    },
                    books: vec![ComicRackMatchBook {
                        book_id: "book-1".to_string(),
                        number: "1".to_string(),
                        title: "Book 1".to_string(),
                    }],
                }],
            }],
        });

        assert_eq!(
            payload.get("errorCode").and_then(|it| it.as_str()),
            Some("")
        );
        assert_eq!(
            payload
                .get("readListMatch")
                .and_then(|it| it.get("errorCode"))
                .and_then(|it| it.as_str()),
            Some("ERR_1009"),
        );
        assert_eq!(
            payload
                .get("requests")
                .and_then(|it| it.as_array())
                .map(Vec::len),
            Some(1),
        );
    }

    #[test]
    fn readlist_payload_normalizes_datetime_fields() {
        let payload = super::readlist_payload(&super::ReadListReadModel {
            id: "readlist-1".to_string(),
            name: "ReadList 1".to_string(),
            summary: "Summary".to_string(),
            ordered: true,
            book_ids: vec!["book-1".to_string()],
            created_date: "2024-01-01 00:00:00".to_string(),
            last_modified_date: "2024-01-02 00:00:00".to_string(),
            filtered: false,
        });

        assert_eq!(
            payload.get("createdDate"),
            Some(&json!("2024-01-01T00:00:00Z"))
        );
        assert_eq!(
            payload.get("lastModifiedDate"),
            Some(&json!("2024-01-02T00:00:00Z"))
        );
    }
}
