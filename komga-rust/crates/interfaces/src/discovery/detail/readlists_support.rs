use super::*;
use komga_application::runtime_sse::register_runtime_sse_event;

use crate::helpers::normalized_date_time;
use crate::state::HttpAppState;
use quick_xml::Reader as XmlReader;
use quick_xml::events::Event as XmlEvent;

pub struct PersistedReadlistWriteInput {
    pub name: String,
    pub summary: String,
    pub ordered: bool,
    pub book_ids: Vec<String>,
}

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
pub struct PersistedReadlistBooksQuery {
    pub page: usize,
    pub size: usize,
    pub unpaged: bool,
    pub library_ids: Option<Vec<String>>,
    pub deleted: Option<bool>,
    pub tags: Vec<String>,
    pub read_statuses: Vec<String>,
    pub media_statuses: Vec<String>,
    pub authors: Vec<String>,
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

pub type PersistedVisibleReadlistBook = BookDetailReadModel;

pub(super) async fn load_persisted_readlists(
    app: &HttpAppState,
    library_ids: Option<&[String]>,
) -> Result<Vec<ReadListReadModel>, String> {
    let rows = app
        .services
        .discovery_detail
        .load_persisted_readlists()
        .await?;

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
    app: &HttpAppState,
) -> Result<Vec<PersistedComicrackMatchCandidateRecord>, String> {
    app.services
        .discovery_detail
        .load_comicrack_match_candidates()
        .await
}

pub async fn load_persisted_book_authors(
    app: &HttpAppState,
    book_id: &str,
) -> Result<Vec<PersistedBookAuthorRecord>, String> {
    app.services
        .discovery_detail
        .load_persisted_book_authors(book_id)
        .await
}

pub(super) async fn load_persisted_readlist_detail(
    app: &HttpAppState,
    readlist_id: &str,
    library_ids: Option<&[String]>,
) -> Result<Option<ReadListReadModel>, String> {
    let Some(row) = app
        .services
        .discovery_detail
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
    app: &HttpAppState,
    readlist_id: &str,
    library_ids: Option<&[String]>,
) -> Result<(Vec<String>, bool), String> {
    let rows = app
        .services
        .discovery_detail
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
) -> PersistedReadlistWriteInput {
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

    PersistedReadlistWriteInput {
        name,
        summary,
        ordered,
        book_ids,
    }
}

pub async fn persist_readlist_create(
    app: &HttpAppState,
    input: &PersistedReadlistWriteInput,
) -> Result<String, String> {
    let readlist_id = generated_readlist_id();
    app.services
        .discovery_detail
        .persist_readlist_create(
            &readlist_id,
            &input.name,
            &input.summary,
            input.ordered,
            &input.book_ids,
        )
        .await?;

    register_runtime_sse_event(
        "ReadListAdded",
        json!({
            "readListId": readlist_id,
            "bookIds": input.book_ids,
        }),
        false,
        None,
    );

    Ok(readlist_id)
}

pub async fn persist_readlist_update(
    app: &HttpAppState,
    readlist_id: &str,
    input: &PersistedReadlistWriteInput,
) -> Result<bool, String> {
    let updated = app
        .services
        .discovery_detail
        .persist_readlist_update(
            readlist_id,
            &input.name,
            &input.summary,
            input.ordered,
            &input.book_ids,
        )
        .await?;
    if updated {
        register_runtime_sse_event(
            "ReadListChanged",
            json!({
                "readListId": readlist_id,
                "bookIds": input.book_ids,
            }),
            false,
            None,
        );
    }
    Ok(updated)
}

pub async fn delete_persisted_readlist(
    app: &HttpAppState,
    readlist_id: &str,
) -> Result<bool, String> {
    let existing = load_persisted_readlist_detail(app, readlist_id, None).await?;
    let deleted = app
        .services
        .discovery_detail
        .delete_persisted_readlist(readlist_id)
        .await?;
    if deleted && let Some(readlist) = existing {
        register_runtime_sse_event(
            "ReadListDeleted",
            json!({
                "readListId": readlist_id,
                "bookIds": readlist.book_ids,
            }),
            false,
            None,
        );
    }
    Ok(deleted)
}

pub async fn upsert_readlist_search_document(
    app: &HttpAppState,
    readlist_id: &str,
) -> Result<bool, String> {
    app.services
        .discovery_detail
        .upsert_readlist_search_document(readlist_id)
        .await
}

pub async fn delete_readlist_search_document(
    app: &HttpAppState,
    readlist_id: &str,
) -> Result<(), String> {
    app.services
        .discovery_detail
        .delete_readlist_search_document(readlist_id)
        .await
}

fn generated_readlist_id() -> String {
    format!("readlist-{}", random_hex_token(12))
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
                .unescape_value()
                .ok()
                .map(|value| value.into_owned());
        } else if xml_name_matches(attribute.key.as_ref(), b"Number") {
            number = attribute
                .unescape_value()
                .ok()
                .map(|value| value.into_owned());
        } else if xml_name_matches(attribute.key.as_ref(), b"Volume") {
            volume = attribute
                .unescape_value()
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
    app: &HttpAppState,
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

pub fn parse_persisted_readlist_books_query(query: &str) -> PersistedReadlistBooksQuery {
    let page = query_value(query, "page")
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(0);
    let size = query_value(query, "size")
        .and_then(|value| value.parse::<usize>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(20);
    let library_ids = {
        let values = query_values(query, "library_id")
            .into_iter()
            .filter(|value| !value.is_empty())
            .map(str::to_string)
            .collect::<Vec<_>>();
        (!values.is_empty()).then_some(values)
    };

    PersistedReadlistBooksQuery {
        page,
        size,
        unpaged: query_bool(query, "unpaged"),
        library_ids,
        deleted: query_value(query, "deleted").map(|value| value.eq_ignore_ascii_case("true")),
        tags: decoded_query_values(query, "tag"),
        read_statuses: decoded_query_values(query, "read_status"),
        media_statuses: decoded_query_values(query, "media_status"),
        authors: decoded_query_values(query, "author"),
    }
}

fn decoded_query_values(query: &str, key: &str) -> Vec<String> {
    query_values(query, key)
        .into_iter()
        .map(decode_query_component)
        .filter(|value| !value.trim().is_empty())
        .collect()
}

pub(super) async fn load_visible_persisted_readlist_books(
    app: &HttpAppState,
    headers: &HeaderMap,
    readlist_id: &str,
    query: &PersistedReadlistBooksQuery,
) -> Result<Option<Vec<PersistedVisibleReadlistBook>>, String> {
    let auth_state = &app.discovery_auth;
    let Some(context) = auth_state
        .resolve_query_context_with_persistence(&*app.services.runtime_identity, headers, None)
        .await
    else {
        return Ok(None);
    };

    let Some(readlist) =
        load_persisted_readlist_detail(app, readlist_id, context.authorized_library_ids.as_deref())
            .await?
    else {
        return Ok(None);
    };
    if context.authorized_library_ids.is_some() && readlist.book_ids.is_empty() {
        return Ok(None);
    }

    let rows = app
        .services
        .discovery_detail
        .load_persisted_readlist_book_rows(readlist_id)
        .await?;
    let mut visible = Vec::new();

    for row in rows {
        if context
            .authorized_library_ids
            .as_ref()
            .is_some_and(|allowed| !allowed.iter().any(|candidate| candidate == &row.library_id))
        {
            continue;
        }
        if query.library_ids.as_ref().is_some_and(|requested| {
            !requested
                .iter()
                .any(|candidate| candidate == &row.library_id)
        }) {
            continue;
        }

        let Some(resource) = load_persisted_book_resource(app, &row.book_id).await? else {
            continue;
        };
        let detail_context = DetailResourceContext {
            library_id: Some(resource.library_id),
            content: Some(DetailContentContext {
                age_rating: resource.age_rating.map(u32::from),
                sharing_labels: resource.sharing_labels,
            }),
        };
        let detail_query_context = match auth_state
            .resolve_detail_query_context_with_persistence(
                &*app.services.runtime_identity,
                headers,
                &detail_context,
            )
            .await
        {
            Ok(context) => context,
            Err(_) => continue,
        };
        let Some(detail) =
            load_persisted_book_detail(app, &row.book_id, detail_query_context.user_id.as_deref())
                .await?
        else {
            continue;
        };

        let book_authors = load_persisted_book_authors(app, &row.book_id).await?;

        if !matches_persisted_readlist_book_filters(&detail, &book_authors, query) {
            continue;
        }

        visible.push(detail);
    }

    Ok(Some(visible))
}

pub(super) fn sort_visible_persisted_readlist_books(
    books: &mut [PersistedVisibleReadlistBook],
    ordered: bool,
) {
    if ordered {
        return;
    }

    books.sort_by(|left, right| left.metadata_release_date.cmp(&right.metadata_release_date));
}

fn matches_persisted_readlist_book_filters(
    book: &BookDetailReadModel,
    book_authors: &[PersistedBookAuthorRecord],
    query: &PersistedReadlistBooksQuery,
) -> bool {
    if query.deleted.is_some_and(|deleted| deleted != book.deleted) {
        return false;
    }
    if !query.tags.is_empty()
        && !query.tags.iter().any(|tag| {
            book.metadata_tags
                .iter()
                .any(|candidate| candidate.eq_ignore_ascii_case(tag))
        })
    {
        return false;
    }
    if !query.media_statuses.is_empty()
        && !query
            .media_statuses
            .iter()
            .any(|status| book.media_status.eq_ignore_ascii_case(status))
    {
        return false;
    }
    if !query.read_statuses.is_empty() {
        let read_status = persisted_read_status(book);
        if !query
            .read_statuses
            .iter()
            .any(|status| read_status.eq_ignore_ascii_case(status))
        {
            return false;
        }
    }
    if !query.authors.is_empty() {
        let mut has_author_filters = false;
        let matches_author_filter = query
            .authors
            .iter()
            .filter_map(|author| parse_author_filter(author))
            .any(|(requested_name, requested_role)| {
                has_author_filters = true;
                book_authors.iter().any(|author| {
                    author.name.eq_ignore_ascii_case(&requested_name)
                        && author.role.eq_ignore_ascii_case(&requested_role)
                })
            });
        if has_author_filters && !matches_author_filter {
            return false;
        }
    }

    true
}

fn parse_author_filter(value: &str) -> Option<(String, String)> {
    let (name, role) = value.rsplit_once(',')?;
    let name = name.trim().to_string();
    if name.is_empty() {
        return None;
    }
    Some((name, role.trim().to_ascii_lowercase()))
}

fn persisted_read_status(book: &BookDetailReadModel) -> &'static str {
    match book.read_progress.as_ref() {
        Some(progress) if progress.completed => "READ",
        Some(progress) if progress.page > 0 => "IN_PROGRESS",
        _ => "UNREAD",
    }
}

pub(super) fn paginate_persisted_readlist_books(
    books: Vec<PersistedVisibleReadlistBook>,
    query: &PersistedReadlistBooksQuery,
) -> PageEnvelope<BookDetailReadModel> {
    let total_elements = books.len();
    if query.unpaged {
        return PageEnvelope::from_slice(books, 0, total_elements.max(1), total_elements);
    }

    let offset = query.page.saturating_mul(query.size);
    let content = if offset >= total_elements {
        Vec::new()
    } else {
        books.into_iter().skip(offset).take(query.size).collect()
    };
    PageEnvelope::from_slice(content, query.page, query.size, total_elements)
}

pub fn decode_query_component(value: &str) -> String {
    let mut decoded = Vec::with_capacity(value.len());
    let bytes = value.as_bytes();
    let mut index = 0usize;

    while index < bytes.len() {
        match bytes[index] {
            b'+' => {
                decoded.push(b' ');
                index += 1;
            }
            b'%' if index + 2 < bytes.len() => {
                let first = (bytes[index + 1] as char).to_digit(16);
                let second = (bytes[index + 2] as char).to_digit(16);

                if let (Some(first), Some(second)) = (first, second) {
                    decoded.push((first * 16 + second) as u8);
                    index += 3;
                } else {
                    decoded.push(bytes[index]);
                    index += 1;
                }
            }
            byte => {
                decoded.push(byte);
                index += 1;
            }
        }
    }

    String::from_utf8_lossy(&decoded).into_owned()
}

#[derive(Clone, Copy)]
pub enum ReadListsSort {
    NameAsc,
    NameDesc,
    CreatedDateAsc,
    CreatedDateDesc,
    LastModifiedDateAsc,
    LastModifiedDateDesc,
    SearchOrName,
}

pub fn parse_readlists_sort(value: &str) -> ReadListsSort {
    let mut parts = value.splitn(2, ',');
    let field = parts.next().unwrap_or_default().trim();
    let direction = parts.next().unwrap_or("asc").trim();

    if field.eq_ignore_ascii_case("name") {
        if direction.eq_ignore_ascii_case("desc") {
            ReadListsSort::NameDesc
        } else {
            ReadListsSort::NameAsc
        }
    } else if field.eq_ignore_ascii_case("createdDate") {
        if direction.eq_ignore_ascii_case("desc") {
            ReadListsSort::CreatedDateDesc
        } else {
            ReadListsSort::CreatedDateAsc
        }
    } else if field.eq_ignore_ascii_case("lastModifiedDate") {
        if direction.eq_ignore_ascii_case("desc") {
            ReadListsSort::LastModifiedDateDesc
        } else {
            ReadListsSort::LastModifiedDateAsc
        }
    } else {
        ReadListsSort::SearchOrName
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
    use super::super::{
        BookDetailReadModel, BookMetadataAuthorReadModel, BookMetadataLinkReadModel,
    };
    use super::{
        ComicRackReadListRequest, ComicRackReadListRequestBook, comicrack_payload,
        decode_query_component, parse_comicrack_readlist, sort_visible_persisted_readlist_books,
    };
    use serde_json::json;

    #[test]
    fn decode_query_component_decodes_percent_encoded_utf8_sequences() {
        assert_eq!(decode_query_component("caf%C3%A9+au+lait"), "café au lait");
    }

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
    fn unordered_readlist_book_sort_uses_release_date_only() {
        let mut books = vec![
            sample_readlist_book("book-b", Some("2024-01-01"), "Zeta", 2),
            sample_readlist_book("book-a", Some("2024-01-01"), "Alpha", 1),
            sample_readlist_book("book-c", Some("2024-01-02"), "Gamma", 3),
        ];

        sort_visible_persisted_readlist_books(&mut books, false);

        assert_eq!(books[0].id, "book-b");
        assert_eq!(books[1].id, "book-a");
        assert_eq!(books[2].id, "book-c");
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

    fn sample_readlist_book(
        id: &str,
        release_date: Option<&str>,
        series_title: &str,
        number: i32,
    ) -> BookDetailReadModel {
        BookDetailReadModel {
            id: id.to_string(),
            series_id: "series-1".to_string(),
            series_title: series_title.to_string(),
            series_title_sort: series_title.to_string(),
            library_id: "lib-1".to_string(),
            name: format!("Book {id}"),
            url: format!("/books/{id}.cbz"),
            number,
            created: "2024-01-01T00:00:00Z".to_string(),
            last_modified: "2024-01-01T00:00:00Z".to_string(),
            file_last_modified: "2024-01-01T00:00:00Z".to_string(),
            size_bytes: 1,
            media_status: "READY".to_string(),
            media_type: "application/vnd.comicbook+zip".to_string(),
            media_pages_count: 1,
            media_comment: String::new(),
            metadata_title: format!("Meta {id}"),
            metadata_summary: String::new(),
            metadata_number: number.to_string(),
            metadata_number_sort: f64::from(number),
            metadata_release_date: release_date.map(str::to_string),
            metadata_title_lock: false,
            metadata_summary_lock: false,
            metadata_number_lock: false,
            metadata_number_sort_lock: false,
            metadata_release_date_lock: false,
            metadata_authors: vec![BookMetadataAuthorReadModel {
                name: "Author".to_string(),
                role: "Writer".to_string(),
            }],
            metadata_authors_lock: false,
            metadata_tags: vec![],
            metadata_tags_lock: false,
            metadata_isbn: String::new(),
            metadata_isbn_lock: false,
            metadata_links: vec![BookMetadataLinkReadModel {
                label: "Site".to_string(),
                url: "https://example.com".to_string(),
            }],
            metadata_links_lock: false,
            metadata_created: "2024-01-01T00:00:00Z".to_string(),
            metadata_last_modified: "2024-01-01T00:00:00Z".to_string(),
            media_epub_divina_compatible: false,
            media_epub_is_kepub: false,
            read_progress: None,
            deleted: false,
            file_hash: String::new(),
            oneshot: false,
        }
    }
}
