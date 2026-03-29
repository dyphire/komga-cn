use super::*;

use crate::discovery_detail_access::collections as collections_access;

#[derive(Clone)]
pub struct PersistedCollectionSeriesReadModel {
    pub id: String,
    pub library_id: String,
    pub name: String,
    pub title: String,
    pub deleted: bool,
    pub oneshot: bool,
}

pub struct PersistedCollectionWriteInput {
    pub name: String,
    pub ordered: bool,
    pub series_ids: Vec<String>,
}

pub async fn persisted_collections_exist(database_file: &FsPath) -> Result<bool, String> {
    collections_access::persisted_collections_exist(database_file).await
}

pub async fn load_persisted_collections(
    database_file: &FsPath,
) -> Result<Vec<CollectionReadModel>, String> {
    let rows = collections_access::load_persisted_collections(database_file).await?;

    let mut collections = Vec::with_capacity(rows.len());
    for row in rows {
        let id = row.id;
        collections.push(CollectionReadModel {
            id: id.clone(),
            name: row.name,
            ordered: row.ordered,
            series_ids: collections_access::load_persisted_collection_series_ids(
                database_file,
                &id,
            )
            .await?,
            created_date: row.created_date,
            last_modified_date: row.last_modified_date,
            filtered: false,
        });
    }

    Ok(collections)
}

pub async fn load_persisted_collection_detail(
    database_file: &FsPath,
    collection_id: &str,
) -> Result<Option<CollectionReadModel>, String> {
    let Some(row) =
        collections_access::load_persisted_collection_detail(database_file, collection_id).await?
    else {
        return Ok(None);
    };

    let collection = CollectionReadModel {
        id: row.id,
        name: row.name,
        ordered: row.ordered,
        series_ids: collections_access::load_persisted_collection_series_ids(
            database_file,
            collection_id,
        )
        .await?,
        created_date: row.created_date,
        last_modified_date: row.last_modified_date,
        filtered: false,
    };

    Ok(Some(collection))
}

pub async fn load_persisted_collection_series(
    database_file: &FsPath,
    collection_id: &str,
) -> Result<Option<Vec<PersistedCollectionSeriesReadModel>>, String> {
    let exists =
        collections_access::persisted_collection_exists(database_file, collection_id).await?;

    if !exists {
        return Ok(None);
    }

    let rows =
        collections_access::load_persisted_collection_series(database_file, collection_id).await?;

    let series = rows
        .into_iter()
        .map(|row| PersistedCollectionSeriesReadModel {
            id: row.id,
            library_id: row.library_id,
            name: row.name,
            title: row.title,
            deleted: row.deleted,
            oneshot: row.oneshot,
        })
        .collect::<Vec<_>>();

    Ok(Some(series))
}

pub async fn series_visible_to_context(
    database_file: &FsPath,
    context: &DiscoveryQueryContext,
    series_id: &str,
    known_library_id: Option<&str>,
) -> Result<bool, String> {
    let library_id = match known_library_id {
        Some(value) => value.to_string(),
        None => {
            let Some(row) =
                collections_access::load_series_library_id(database_file, series_id).await?
            else {
                return Ok(false);
            };
            row
        }
    };

    if let Some(authorized_libraries) = context.authorized_library_ids.as_ref()
        && !authorized_libraries
            .iter()
            .any(|candidate| candidate == &library_id)
    {
        return Ok(false);
    }

    let Some(restrictions) = context.restrictions.as_ref() else {
        return Ok(true);
    };

    let restriction_record =
        collections_access::load_series_restrictions(database_file, series_id).await?;

    Ok(restrictions_allow_content(
        restrictions,
        restriction_record.age_rating,
        &restriction_record.labels,
    ))
}

fn restrictions_allow_content(
    restrictions: &QueryRestrictions,
    age_rating: Option<u16>,
    sharing_labels: &[String],
) -> bool {
    let normalized_labels = sharing_labels
        .iter()
        .map(|value| value.to_ascii_lowercase())
        .collect::<Vec<_>>();

    let age_allowed = if restrictions.age_restriction == Some(AgeRestrictionKind::AllowOnly) {
        restrictions
            .age
            .map(|age_limit| age_rating.is_some_and(|age| age <= age_limit))
    } else {
        None
    };

    let label_allowed = if restrictions.labels_allow.is_empty() {
        None
    } else {
        Some(
            restrictions
                .labels_allow
                .iter()
                .any(|candidate| normalized_labels.contains(&candidate.to_ascii_lowercase())),
        )
    };

    let allowed = match (age_allowed, label_allowed) {
        (None, value) => value != Some(false),
        (value, None) => value != Some(false),
        (age_value, label_value) => age_value != Some(false) || label_value != Some(false),
    };
    if !allowed {
        return false;
    }

    let age_denied = if restrictions.age_restriction == Some(AgeRestrictionKind::Exclude) {
        restrictions
            .age
            .is_some_and(|age_limit| age_rating.is_some_and(|age| age >= age_limit))
    } else {
        false
    };

    let label_denied = if restrictions.labels_exclude.is_empty() {
        false
    } else {
        restrictions
            .labels_exclude
            .iter()
            .any(|candidate| normalized_labels.contains(&candidate.to_ascii_lowercase()))
    };

    !age_denied && !label_denied
}

pub fn collection_series_page_payload(
    mut series: Vec<PersistedCollectionSeriesReadModel>,
    page: usize,
    size: usize,
    unpaged: bool,
) -> Value {
    let total_elements = series.len();
    if unpaged {
        return json!({
            "content": collection_series_payload(&series),
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
        });
    }

    let page_size = size.max(1);
    let offset = page.saturating_mul(page_size);
    let page_content = if offset >= total_elements {
        vec![]
    } else {
        series
            .drain(offset..(offset + page_size).min(total_elements))
            .collect()
    };
    let total_pages = if total_elements == 0 {
        0
    } else {
        ((total_elements - 1) / page_size) + 1
    };
    let number_of_elements = page_content.len();
    let first = page == 0;
    let last = total_pages == 0 || page + 1 >= total_pages;

    json!({
        "content": collection_series_payload(&page_content),
        "pageable": {
            "pageNumber": page,
            "pageSize": page_size,
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
        "totalElements": total_elements,
        "totalPages": total_pages,
        "first": first,
        "size": page_size,
        "number": page,
        "sort": {
            "empty": false,
            "sorted": true,
            "unsorted": false
        },
        "numberOfElements": number_of_elements,
        "empty": number_of_elements == 0
    })
}

pub fn collection_write_input(payload: &Value) -> PersistedCollectionWriteInput {
    let name = payload
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or("collection")
        .to_string();
    let ordered = payload
        .get("ordered")
        .and_then(Value::as_bool)
        .unwrap_or(true);
    let series_ids = payload
        .get("seriesIds")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(str::to_string)
        .collect::<Vec<_>>();

    PersistedCollectionWriteInput {
        name,
        ordered,
        series_ids,
    }
}

pub async fn persist_collection_create(
    database_file: &FsPath,
    input: &PersistedCollectionWriteInput,
) -> Result<String, String> {
    let collection_id = generated_collection_id();
    collections_access::persist_collection_create(
        database_file,
        &collection_id,
        &input.name,
        input.ordered,
        &input.series_ids,
    )
    .await?;

    Ok(collection_id)
}

pub async fn persist_collection_update(
    database_file: &FsPath,
    collection_id: &str,
    input: &PersistedCollectionWriteInput,
) -> Result<bool, String> {
    collections_access::persist_collection_update(
        database_file,
        collection_id,
        &input.name,
        input.ordered,
        &input.series_ids,
    )
    .await
}

pub async fn delete_persisted_collection(
    database_file: &FsPath,
    collection_id: &str,
) -> Result<bool, String> {
    collections_access::delete_persisted_collection(database_file, collection_id).await
}

fn generated_collection_id() -> String {
    format!("collection-{}", random_hex_token(12))
}

pub fn collections_page_payload(page: PageEnvelope<CollectionReadModel>) -> Value {
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

pub fn collection_payload(collection: &CollectionReadModel) -> Value {
    json!({
        "id": collection.id,
        "name": collection.name,
        "ordered": collection.ordered,
        "seriesIds": collection.series_ids,
        "createdDate": collection.created_date,
        "lastModifiedDate": collection.last_modified_date,
        "filtered": collection.filtered,
    })
}

pub fn collection_series_payload(series: &[PersistedCollectionSeriesReadModel]) -> Value {
    Value::Array(
        series
            .iter()
            .map(|series| {
                json!({
                    "id": series.id,
                    "libraryId": series.library_id,
                    "name": series.name,
                    "metadata": {
                        "title": series.title,
                        "sharingLabels": []
                    },
                    "deleted": series.deleted,
                    "oneshot": series.oneshot,
                })
            })
            .collect(),
    )
}
