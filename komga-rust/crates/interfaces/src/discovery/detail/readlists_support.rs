use super::*;

use crate::helpers::normalized_date_time;
use crate::state::DiscoveryState;
use komga_application::discovery::ReadlistMutationInput;
use quick_xml::Reader as XmlReader;
use quick_xml::XmlVersion;
use quick_xml::events::Event as XmlEvent;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ComicRackReadListRequestBook {
    pub series_candidates: Vec<String>,
    pub number: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ComicRackReadListRequest {
    pub name: String,
    pub books: Vec<ComicRackReadListRequestBook>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ComicRackMatchSeries {
    pub series_id: String,
    pub title: String,
    pub release_date: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ComicRackMatchBook {
    pub book_id: String,
    pub number: String,
    pub title: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ComicRackRequestMatchGroup {
    pub series: ComicRackMatchSeries,
    pub books: Vec<ComicRackMatchBook>,
}

async fn load_persisted_readlists(
    app: &DiscoveryState,
    library_ids: Option<&[String]>,
) -> Result<Vec<ReadListReadModel>, String> {
    let rows = app.readlist.load_persisted_readlists().await?;

    let mut readlists = Vec::with_capacity(rows.len());
    for row in rows {
        let id = row.id;
        let (book_ids, filtered) = load_persisted_readlist_book_ids(app, &id, library_ids).await?;
        if library_ids.is_some() && book_ids.is_empty() {
            continue;
        }

        readlists.push(ReadListReadModel {
            id,
            name: row.name,
            summary: row.summary,
            ordered: row.ordered,
            book_ids,
            created_date: row.created_date,
            last_modified_date: row.last_modified_date,
            filtered,
        });
    }

    Ok(readlists)
}

pub async fn load_comicrack_match_candidates(
    app: &DiscoveryState,
) -> Result<Vec<PersistedComicrackMatchCandidateRecord>, String> {
    app.readlist.load_comicrack_match_candidates().await
}

pub(super) async fn load_persisted_readlist_detail(
    app: &DiscoveryState,
    readlist_id: &str,
    library_ids: Option<&[String]>,
) -> Result<Option<ReadListReadModel>, String> {
    let Some(row) = app
        .readlist
        .load_persisted_readlist_detail(readlist_id)
        .await?
    else {
        return Ok(None);
    };

    let (book_ids, filtered) =
        load_persisted_readlist_book_ids(app, readlist_id, library_ids).await?;

    let readlist = ReadListReadModel {
        id: row.id,
        name: row.name,
        summary: row.summary,
        ordered: row.ordered,
        book_ids,
        created_date: row.created_date,
        last_modified_date: row.last_modified_date,
        filtered,
    };

    Ok(Some(readlist))
}

async fn load_persisted_readlist_book_ids(
    app: &DiscoveryState,
    readlist_id: &str,
    library_ids: Option<&[String]>,
) -> Result<(Vec<String>, bool), String> {
    let rows = app
        .readlist
        .load_persisted_readlist_book_rows(readlist_id)
        .await?;

    let total_count = rows.len();
    let book_ids = rows
        .into_iter()
        .filter(|row| {
            library_ids
                .is_none_or(|allowed| allowed.iter().any(|candidate| candidate == &row.library_id))
        })
        .map(|row| row.book_id)
        .collect::<Vec<_>>();

    Ok((book_ids.clone(), book_ids.len() < total_count))
}

pub fn merge_readlist_write_input(
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

pub fn parse_comicrack_readlist(bytes: &[u8]) -> Result<ComicRackReadListRequest, &'static str> {
    let mut reader = XmlReader::from_reader(bytes);
    reader.config_mut().trim_text(true);

    let mut buffer = Vec::new();
    let mut readlist_name = None::<String>;
    let mut books = Vec::<ComicRackReadListRequestBook>::new();
    let mut reading_name = false;
    let mut depth = 0usize;

    loop {
        match reader.read_event_into(&mut buffer) {
            Ok(XmlEvent::Start(event)) => {
                depth += 1;
                if xml_name_matches(event.name().as_ref(), b"Name") {
                    reading_name = true;
                } else if xml_name_matches(event.name().as_ref(), b"Book") {
                    books.push(parse_comicrack_book(&event)?);
                }
            }
            Ok(XmlEvent::Empty(event)) if xml_name_matches(event.name().as_ref(), b"Book") => {
                books.push(parse_comicrack_book(&event)?);
            }
            Ok(XmlEvent::Text(text)) if reading_name => {
                let value = String::from_utf8_lossy(text.as_ref()).trim().to_string();
                readlist_name = Some(value);
            }
            Ok(XmlEvent::End(event)) if xml_name_matches(event.name().as_ref(), b"Name") => {
                depth = depth.saturating_sub(1);
                reading_name = false;
            }
            Ok(XmlEvent::End(_)) => {
                depth = depth.saturating_sub(1);
            }
            Ok(XmlEvent::Eof) => {
                if depth != 0 {
                    return Err("ERR_1015");
                }
                break;
            }
            Err(_) => return Err("ERR_1015"),
            _ => {}
        }
        buffer.clear();
    }

    let name = readlist_name
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .ok_or("ERR_1030")?;
    if books.is_empty() {
        return Err("ERR_1029");
    }

    Ok(ComicRackReadListRequest { name, books })
}

fn parse_comicrack_book(
    event: &quick_xml::events::BytesStart<'_>,
) -> Result<ComicRackReadListRequestBook, &'static str> {
    let mut series = None::<String>;
    let mut number = None::<String>;
    let mut volume = None::<String>;

    for attribute in event.attributes().flatten() {
        if xml_name_matches(attribute.key.as_ref(), b"Series") {
            series = attribute
                .normalized_value(XmlVersion::Implicit1_0)
                .ok()
                .map(|value| value.into_owned());
        } else if xml_name_matches(attribute.key.as_ref(), b"Number") {
            number = attribute
                .normalized_value(XmlVersion::Implicit1_0)
                .ok()
                .map(|value| value.into_owned());
        } else if xml_name_matches(attribute.key.as_ref(), b"Volume") {
            volume = attribute
                .normalized_value(XmlVersion::Implicit1_0)
                .ok()
                .map(|value| value.into_owned());
        }
    }

    let series = series
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .ok_or("ERR_1031")?;
    let number = number
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .ok_or("ERR_1031")?;

    let mut series_candidates = vec![series.clone()];
    if let Some(volume) = volume
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty() && value != "1")
    {
        series_candidates.push(format!("{series} ({volume})"));
    }

    Ok(ComicRackReadListRequestBook {
        series_candidates,
        number,
    })
}

fn xml_name_matches(actual: &[u8], expected: &[u8]) -> bool {
    actual.eq_ignore_ascii_case(expected)
}

pub fn comicrack_payload(name: &str, error_code: &str, requests: Vec<Value>) -> Value {
    json!({
        "readListMatch": {
            "name": name,
            "errorCode": error_code,
        },
        "requests": requests,
        "errorCode": "",
    })
}

pub async fn match_comicrack_readlist(
    app: &DiscoveryState,
    request: &ComicRackReadListRequest,
) -> Result<Value, String> {
    let readlists = load_persisted_readlists(app, None).await?;
    let duplicate_error_code = if readlists
        .iter()
        .any(|readlist| readlist.name.eq_ignore_ascii_case(&request.name))
    {
        "ERR_1009"
    } else {
        ""
    };

    let candidates = load_comicrack_match_candidates(app).await?;
    let requests = request
        .books
        .iter()
        .map(|book| {
            let mut grouped =
                std::collections::BTreeMap::<String, ComicRackRequestMatchGroup>::new();
            for candidate in candidates.iter().filter(|candidate| {
                book.series_candidates
                    .iter()
                    .any(|series| series.eq_ignore_ascii_case(&candidate.series_title))
                    && normalized_comicrack_number(&book.number)
                        == normalized_comicrack_number(&candidate.book_number)
            }) {
                grouped
                    .entry(candidate.series_id.clone())
                    .or_insert_with(|| ComicRackRequestMatchGroup {
                        series: ComicRackMatchSeries {
                            series_id: candidate.series_id.clone(),
                            title: candidate.series_title.clone(),
                            release_date: candidate.series_release_date.clone(),
                        },
                        books: Vec::new(),
                    })
                    .books
                    .push(ComicRackMatchBook {
                        book_id: candidate.book_id.clone(),
                        number: candidate.book_number.clone(),
                        title: candidate.book_title.clone(),
                    });
            }

            json!({
                "request": {
                    "series": book.series_candidates,
                    "number": book.number,
                },
                "matches": grouped.into_values().map(|group| {
                    json!({
                        "series": {
                            "seriesId": group.series.series_id,
                            "title": group.series.title,
                            "releaseDate": group.series.release_date,
                        },
                        "books": group.books.into_iter().map(|candidate| {
                            json!({
                                "bookId": candidate.book_id,
                                "number": candidate.number,
                                "title": candidate.title,
                            })
                        }).collect::<Vec<_>>(),
                    })
                }).collect::<Vec<_>>(),
            })
        })
        .collect::<Vec<_>>();

    Ok(comicrack_payload(
        &request.name,
        duplicate_error_code,
        requests,
    ))
}

fn normalized_comicrack_number(value: &str) -> String {
    let normalized = value.trim().trim_start_matches('0').to_ascii_lowercase();
    if normalized.is_empty() {
        value.trim().to_ascii_lowercase()
    } else {
        normalized
    }
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
    use super::{
        ComicRackReadListRequest, ComicRackReadListRequestBook, comicrack_payload,
        parse_comicrack_readlist,
    };
    use serde_json::json;

    #[test]
    fn parse_comicrack_readlist_rejects_invalid_xml() {
        let error = parse_comicrack_readlist(b"<ReadingList>")
            .expect_err("invalid xml should fail with coded error");

        assert_eq!(error, "ERR_1015");
    }

    #[test]
    fn parse_comicrack_readlist_rejects_blank_name() {
        let error = parse_comicrack_readlist(
            br#"<ReadingList><Name>   </Name><Books><Book Series="Series 1" Number="1" /></Books></ReadingList>"#,
        )
        .expect_err("blank name should fail with coded error");

        assert_eq!(error, "ERR_1030");
    }

    #[test]
    fn parse_comicrack_readlist_rejects_missing_books() {
        let error =
            parse_comicrack_readlist(br#"<ReadingList><Name>ReadList 1</Name></ReadingList>"#)
                .expect_err("missing books should fail with coded error");

        assert_eq!(error, "ERR_1029");
    }

    #[test]
    fn parse_comicrack_readlist_rejects_missing_series_or_number() {
        let error = parse_comicrack_readlist(
            br#"<ReadingList><Name>ReadList 1</Name><Books><Book Series="Series 1" /></Books></ReadingList>"#,
        )
        .expect_err("missing number should fail with coded error");

        assert_eq!(error, "ERR_1031");
    }

    #[test]
    fn parse_comicrack_readlist_returns_normalized_request() {
        let request = parse_comicrack_readlist(
            br#"<ReadingList><Name>ReadList 1</Name><Books><Book Series="Series 1" Number="001" Volume="2" /></Books></ReadingList>"#,
        )
        .expect("valid cbl should parse");

        assert_eq!(
            request,
            ComicRackReadListRequest {
                name: "ReadList 1".to_string(),
                books: vec![ComicRackReadListRequestBook {
                    series_candidates: vec!["Series 1".to_string(), "Series 1 (2)".to_string()],
                    number: "001".to_string(),
                }],
            },
        );
    }

    #[test]
    fn comicrack_payload_serializes_matches_and_error_codes() {
        let payload = comicrack_payload(
            "ReadList 1",
            "ERR_1009",
            vec![json!({
                "request": {"series": ["Series 1"], "number": "1"},
                "matches": [{
                    "series": {"seriesId": "series-1", "title": "Series 1", "releaseDate": "2024-01-15"},
                    "books": [{"bookId": "book-1", "number": "1", "title": "Book 1"}]
                }]
            })],
        );

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
