use super::*;

use crate::discovery_detail_access::readlists as readlists_access;

pub struct PersistedReadlistWriteInput {
    pub name: String,
    pub summary: String,
    pub ordered: bool,
    pub book_ids: Vec<String>,
}

pub async fn persisted_readlists_exist(database_file: &FsPath) -> Result<bool, String> {
    readlists_access::persisted_readlists_exist(database_file).await
}

pub async fn load_persisted_readlists(
    database_file: &FsPath,
    library_ids: Option<&[String]>,
) -> Result<Vec<ReadListReadModel>, String> {
    let rows = readlists_access::load_persisted_readlists(database_file).await?;

    let mut readlists = Vec::with_capacity(rows.len());
    for row in rows {
        let id = row.id;
        let (book_ids, filtered) =
            load_persisted_readlist_book_ids(database_file, &id, library_ids).await?;
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

pub async fn load_persisted_readlist_detail(
    database_file: &FsPath,
    readlist_id: &str,
    library_ids: Option<&[String]>,
) -> Result<Option<ReadListReadModel>, String> {
    let Some(row) =
        readlists_access::load_persisted_readlist_detail(database_file, readlist_id).await?
    else {
        return Ok(None);
    };

    let (book_ids, filtered) =
        load_persisted_readlist_book_ids(database_file, readlist_id, library_ids).await?;

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
    database_file: &FsPath,
    readlist_id: &str,
    library_ids: Option<&[String]>,
) -> Result<(Vec<String>, bool), String> {
    let rows =
        readlists_access::load_persisted_readlist_book_rows(database_file, readlist_id).await?;

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

pub fn readlist_write_input(payload: &Value) -> PersistedReadlistWriteInput {
    let name = payload
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or("readlist")
        .to_string();
    let summary = payload
        .get("summary")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let ordered = payload
        .get("ordered")
        .and_then(Value::as_bool)
        .unwrap_or(true);
    let book_ids = payload
        .get("bookIds")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(str::to_string)
        .collect::<Vec<_>>();

    PersistedReadlistWriteInput {
        name,
        summary,
        ordered,
        book_ids,
    }
}

pub async fn persist_readlist_create(
    database_file: &FsPath,
    input: &PersistedReadlistWriteInput,
) -> Result<String, String> {
    let readlist_id = generated_readlist_id();
    readlists_access::persist_readlist_create(
        database_file,
        &readlist_id,
        &input.name,
        &input.summary,
        input.ordered,
        &input.book_ids,
    )
    .await?;

    Ok(readlist_id)
}

pub async fn persist_readlist_update(
    database_file: &FsPath,
    readlist_id: &str,
    input: &PersistedReadlistWriteInput,
) -> Result<bool, String> {
    readlists_access::persist_readlist_update(
        database_file,
        readlist_id,
        &input.name,
        &input.summary,
        input.ordered,
        &input.book_ids,
    )
    .await
}

pub async fn delete_persisted_readlist(
    database_file: &FsPath,
    readlist_id: &str,
) -> Result<bool, String> {
    readlists_access::delete_persisted_readlist(database_file, readlist_id).await
}

fn generated_readlist_id() -> String {
    format!("readlist-{}", random_hex_token(12))
}

pub fn readlist_query_values(query: &str, key: &str) -> Option<Vec<String>> {
    let values = query_values(query, key)
        .into_iter()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .collect::<Vec<_>>();
    (!values.is_empty()).then_some(values)
}

pub fn readlist_author_query_values(query: &str) -> Result<Option<Vec<String>>, String> {
    let raw_values = query_values(query, "author");
    if raw_values.is_empty() {
        return Ok(None);
    }

    let mut authors = Vec::with_capacity(raw_values.len());
    for value in raw_values {
        if let Some(author) = parse_readlist_author_query_value(value)? {
            authors.push(author);
        }
    }

    if authors.is_empty() {
        Err("readlist author filter must include at least one supported value".to_string())
    } else {
        Ok(Some(authors))
    }
}

pub fn parse_readlist_author_query_value(value: &str) -> Result<Option<String>, String> {
    let mut parts = value.splitn(2, ',');
    let Some(name) = parts.next() else {
        return Ok(None);
    };
    let name = name.trim();
    if name.is_empty() {
        return Ok(None);
    }

    let Some(role) = parts.next() else {
        return Ok(Some(name.to_ascii_lowercase()));
    };
    let role = role.trim();
    if role.is_empty() || role.eq_ignore_ascii_case("writer") {
        Ok(Some(name.to_ascii_lowercase()))
    } else {
        Err(format!(
            "unsupported readlist author role '{role}', only empty role or 'writer' is supported",
        ))
    }
}

pub fn parse_optional_query_bool(value: &str) -> Option<bool> {
    if value.eq_ignore_ascii_case("true") {
        Some(true)
    } else if value.eq_ignore_ascii_case("false") {
        Some(false)
    } else {
        None
    }
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
    } else {
        ReadListsSort::SearchOrName
    }
}

pub fn readlist_search_score(readlist: &ReadListReadModel, tokens: &[String]) -> usize {
    let name = readlist.name.to_ascii_lowercase();
    let summary = readlist.summary.to_ascii_lowercase();

    tokens
        .iter()
        .map(|token| {
            let name_hits = name.matches(token).count();
            let summary_hits = summary.matches(token).count();
            name_hits + summary_hits
        })
        .sum::<usize>()
}

pub fn readlists_page_payload(page: PageEnvelope<ReadListReadModel>) -> Value {
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

pub fn readlist_payload(readlist: &ReadListReadModel) -> Value {
    json!({
        "id": readlist.id,
        "name": readlist.name,
        "summary": readlist.summary,
        "ordered": readlist.ordered,
        "bookIds": readlist.book_ids,
        "createdDate": readlist.created_date,
        "lastModifiedDate": readlist.last_modified_date,
        "filtered": readlist.filtered,
    })
}

#[cfg(test)]
mod tests {
    use super::{
        decode_query_component, parse_readlist_author_query_value, readlist_author_query_values,
    };

    #[test]
    fn decode_query_component_decodes_percent_encoded_utf8_sequences() {
        assert_eq!(decode_query_component("caf%C3%A9+au+lait"), "café au lait");
    }

    #[test]
    fn parse_readlist_author_query_value_accepts_writer_role_and_plain_name() {
        assert_eq!(
            parse_readlist_author_query_value("Jane Writer")
                .expect("plain author name should parse"),
            Some("jane writer".to_string()),
        );
        assert_eq!(
            parse_readlist_author_query_value("Jane Writer,writer")
                .expect("writer role should parse"),
            Some("jane writer".to_string()),
        );
    }

    #[test]
    fn parse_readlist_author_query_value_rejects_unsupported_roles() {
        let error = parse_readlist_author_query_value("Jane Writer,inker")
            .expect_err("unsupported readlist role should be rejected");
        assert!(error.contains("unsupported readlist author role"));
        assert!(error.contains("inker"));
    }

    #[test]
    fn readlist_author_query_values_rejects_payloads_without_supported_author_values() {
        let error = readlist_author_query_values("author=Jane%20Writer,inker")
            .expect_err("unsupported role-only payload should be rejected");
        assert!(error.contains("unsupported readlist author role"));

        let empty_error = readlist_author_query_values("author=,%20")
            .expect_err("blank author payload should be rejected");
        assert!(empty_error.contains("must include at least one supported value"));
    }
}
