use axum::Json;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode, Uri};
use axum::response::{IntoResponse, Response};
use komga_domain::discovery::{
    PageEnvelope, QueryRestrictions as DomainRestrictions, content_allowed_by_restrictions,
};
use serde_json::{Value, json};

use crate::discovery_auth::context::QueryRestrictions;
use crate::discovery_auth::principal::AgeRestrictionKind;
use crate::helpers::{books_page_payload, query_bool, query_value, to_domain_query_context};
use crate::identity_access::auth::Authenticated;
use crate::state::DiscoveryState;

use super::super::persisted::common_helpers::{
    filter_rows, internal_error_response, requested_query_values,
};
use super::super::persisted::library_mappings::remap_requested_library_ids_for_persisted;

fn normalize_books_latest_unpaged_page_shape<T>(mut page: PageEnvelope<T>) -> PageEnvelope<T> {
    const KOTLIN_PAGE_SIZE: usize = 20;

    page.page = 0;
    page.size = KOTLIN_PAGE_SIZE;
    page.total_pages = if page.total_elements == 0 {
        0
    } else {
        ((page.total_elements - 1) / KOTLIN_PAGE_SIZE) + 1
    };
    page
}

fn ondeck_content_allowed(
    restrictions: Option<&QueryRestrictions>,
    age_rating: Option<u16>,
    sharing_labels: &[String],
) -> bool {
    let Some(restrictions) = restrictions else {
        return true;
    };
    let domain_restrictions = DomainRestrictions {
        age: restrictions.age,
        age_restriction: restrictions.age_restriction.map(|kind| match kind {
            AgeRestrictionKind::AllowOnly => komga_domain::discovery::AgeRestrictionKind::AllowOnly,
            AgeRestrictionKind::Exclude => komga_domain::discovery::AgeRestrictionKind::Exclude,
        }),
        labels_allow: restrictions.labels_allow.clone(),
        labels_exclude: restrictions.labels_exclude.clone(),
    };
    content_allowed_by_restrictions(&domain_restrictions, age_rating, sharing_labels)
}

fn ondeck_page_payload(content: Vec<Value>, uri: &Uri) -> Value {
    let query = uri.query().unwrap_or_default();
    let requested_page = query_value(query, "page")
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(0);
    let requested_size = query_value(query, "size")
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(20)
        .max(1);
    let unpaged = query_bool(query, "unpaged");

    let total_elements = content.len();
    let page_size = if unpaged {
        total_elements.max(20)
    } else {
        requested_size
    };
    let offset = if unpaged {
        0
    } else {
        requested_page.saturating_mul(page_size)
    };
    let content = if unpaged {
        content
    } else if offset >= total_elements {
        vec![]
    } else {
        content.into_iter().skip(offset).take(page_size).collect()
    };

    let page = if unpaged { 0 } else { requested_page };
    let total_pages = if total_elements == 0 {
        0
    } else {
        total_elements.div_ceil(page_size)
    };
    let number_of_elements = content.len();
    let first = page == 0;
    let last = total_pages == 0 || page + 1 >= total_pages;
    let sort = json!({
        "empty": true,
        "sorted": false,
        "unsorted": true,
    });

    json!({
        "content": content,
        "pageable": {
            "pageNumber": page,
            "pageSize": page_size,
            "sort": sort.clone(),
            "offset": offset,
            "paged": true,
            "unpaged": false,
        },
        "last": last,
        "totalElements": total_elements,
        "totalPages": total_pages,
        "first": first,
        "size": page_size,
        "number": page,
        "sort": sort,
        "numberOfElements": number_of_elements,
        "empty": number_of_elements == 0,
    })
}

pub async fn books_latest(
    State(app): State<DiscoveryState>,
    _authenticated: Authenticated,
    headers: HeaderMap,
    uri: Uri,
) -> Response {
    let query = uri.query().unwrap_or_default();

    let requested_library_ids = requested_query_values(query, "library_id");
    let library_ids = remap_requested_library_ids_for_persisted(
        app.library_id_mapping.as_ref(),
        requested_library_ids.as_ref(),
    )
    .await
    .or(requested_library_ids);

    let interfaces_context = match app
        .discovery_auth
        .resolve_query_context_with_persistence(&app.identity, &headers, library_ids.as_deref())
        .await
    {
        Some(context) => context,
        None => return StatusCode::UNAUTHORIZED.into_response(),
    };
    let context = to_domain_query_context(interfaces_context);

    let resolved = super::super::query::resolve_latest_books_request(&uri, library_ids);

    match app
        .discovery_browse
        .list_latest_books(&context, resolved.request)
        .await
    {
        Ok(page) => {
            let page = if resolved.response.kotlin_unpaged_shape {
                (normalize_books_latest_unpaged_page_shape(page), true)
            } else {
                (page, true)
            }
            .0;
            Json(books_page_payload(
                page,
                context.is_admin,
                resolved.response.paged,
                resolved.response.sorted,
            ))
            .into_response()
        }
        Err(error) => internal_error_response(format!("{error:?}")),
    }
}

pub async fn books_ondeck(
    State(app): State<DiscoveryState>,
    _authenticated: Authenticated,
    headers: HeaderMap,
    uri: Uri,
) -> Response {
    let query = uri.query().unwrap_or_default();
    let requested_library_ids = requested_query_values(query, "library_id");
    let library_ids = remap_requested_library_ids_for_persisted(
        app.library_id_mapping.as_ref(),
        requested_library_ids.as_ref(),
    )
    .await
    .or(requested_library_ids);
    let context = match app
        .discovery_auth
        .resolve_query_context_with_persistence(&app.identity, &headers, library_ids.as_deref())
        .await
    {
        Some(context) => context,
        None => return StatusCode::UNAUTHORIZED.into_response(),
    };
    let Some(user_id) = context.user_id.as_deref() else {
        return StatusCode::UNAUTHORIZED.into_response();
    };

    match app.book_special_lists.load_ondeck_books(user_id).await {
        Ok(entries) => {
            let filtered_entries =
                if let Some(allowed_ids) = context.authorized_library_ids.as_ref() {
                    filter_rows(entries, |row| {
                        allowed_ids.iter().any(|id| id == row.library_id.as_str())
                    })
                } else {
                    entries
                };
            let mut content = Vec::with_capacity(filtered_entries.len());
            for entry in filtered_entries {
                let resource =
                    match super::super::detail::load_persisted_book_resource(&app, &entry.id).await
                    {
                        Ok(Some(resource)) => resource,
                        Ok(None) => {
                            return internal_error_response(format!(
                                "missing persisted on-deck book resource for '{}'",
                                entry.id
                            ));
                        }
                        Err(error) => return internal_error_response(error),
                    };

                if !ondeck_content_allowed(
                    context.restrictions.as_ref(),
                    resource.age_rating,
                    &resource.sharing_labels,
                ) {
                    continue;
                }

                let detail = match super::super::detail::load_persisted_book_detail(
                    &app,
                    &entry.id,
                    Some(user_id),
                )
                .await
                {
                    Ok(Some(detail)) => detail,
                    Ok(None) => {
                        return internal_error_response(format!(
                            "missing persisted on-deck book detail for '{}'",
                            entry.id
                        ));
                    }
                    Err(error) => return internal_error_response(error),
                };
                content.push(super::super::detail::book_detail_payload(
                    &detail,
                    context.is_admin,
                ));
            }

            Json(ondeck_page_payload(content, &uri)).into_response()
        }
        Err(error) => internal_error_response(error),
    }
}
