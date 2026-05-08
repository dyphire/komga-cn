use super::*;
use crate::identity_access::auth::Authenticated;
use crate::state::DiscoveryState;
use axum::extract::State;

pub async fn book_detail(
    State(app): State<DiscoveryState>,
    _: Authenticated,
    headers: HeaderMap,
    Path(book_id): Path<String>,
) -> Response {
    let Some(resource) = (match load_persisted_book_resource(&app, &book_id).await {
        Ok(resource) => resource,
        Err(error) => return internal_error_response(error),
    }) else {
        return StatusCode::NOT_FOUND.into_response();
    };

    let detail_context = DetailResourceContext {
        library_id: Some(resource.library_id),
        content: Some(DetailContentContext {
            age_rating: resource.age_rating.map(u32::from),
            sharing_labels: resource.sharing_labels,
        }),
    };

    let detail_query_context = match app
        .discovery_auth
        .resolve_detail_query_context_with_persistence(
            &*app.identity.service,
            &headers,
            &detail_context,
        )
        .await
    {
        Ok(context) => context,
        Err(denial) => return detail_access_denial_response(denial),
    };

    let is_admin = detail_query_context.is_admin;
    match load_persisted_book_detail(&app, &book_id, detail_query_context.user_id.as_deref()).await
    {
        Ok(Some(book)) => Json(book_detail_payload(&book, is_admin)).into_response(),
        Ok(None) => StatusCode::NOT_FOUND.into_response(),
        Err(error) => internal_error_response(error),
    }
}

pub async fn book_sibling_previous(
    State(app): State<DiscoveryState>,
    _: Authenticated,
    headers: HeaderMap,
    Path(book_id): Path<String>,
) -> Response {
    let book_id = resolve_book_id_for_persisted(&app, &book_id).await;

    let Some(resource) = (match load_persisted_book_resource(&app, &book_id).await {
        Ok(resource) => resource,
        Err(error) => return internal_error_response(error),
    }) else {
        return StatusCode::NOT_FOUND.into_response();
    };

    let detail_context = DetailResourceContext {
        library_id: Some(resource.library_id),
        content: Some(DetailContentContext {
            age_rating: resource.age_rating.map(u32::from),
            sharing_labels: resource.sharing_labels,
        }),
    };

    let detail_query_context = match app
        .discovery_auth
        .resolve_detail_query_context_with_persistence(
            &*app.identity.service,
            &headers,
            &detail_context,
        )
        .await
    {
        Ok(context) => context,
        Err(denial) => return detail_access_denial_response(denial),
    };
    let is_admin = detail_query_context.is_admin;

    match load_persisted_book_sibling_detail(
        &app,
        &book_id,
        PersistedBookSiblingDirection::Previous,
        detail_query_context.user_id.as_deref(),
    )
    .await
    {
        Ok(Some(book)) => Json(book_detail_payload(&book, is_admin)).into_response(),
        Ok(None) => StatusCode::NOT_FOUND.into_response(),
        Err(error) => internal_error_response(error),
    }
}

pub async fn book_sibling_next(
    State(app): State<DiscoveryState>,
    _: Authenticated,
    headers: HeaderMap,
    Path(book_id): Path<String>,
) -> Response {
    let book_id = resolve_book_id_for_persisted(&app, &book_id).await;

    let Some(resource) = (match load_persisted_book_resource(&app, &book_id).await {
        Ok(resource) => resource,
        Err(error) => return internal_error_response(error),
    }) else {
        return StatusCode::NOT_FOUND.into_response();
    };

    let detail_context = DetailResourceContext {
        library_id: Some(resource.library_id),
        content: Some(DetailContentContext {
            age_rating: resource.age_rating.map(u32::from),
            sharing_labels: resource.sharing_labels,
        }),
    };

    let detail_query_context = match app
        .discovery_auth
        .resolve_detail_query_context_with_persistence(
            &*app.identity.service,
            &headers,
            &detail_context,
        )
        .await
    {
        Ok(context) => context,
        Err(denial) => return detail_access_denial_response(denial),
    };
    let is_admin = detail_query_context.is_admin;

    match load_persisted_book_sibling_detail(
        &app,
        &book_id,
        PersistedBookSiblingDirection::Next,
        detail_query_context.user_id.as_deref(),
    )
    .await
    {
        Ok(Some(book)) => Json(book_detail_payload(&book, is_admin)).into_response(),
        Ok(None) => StatusCode::NOT_FOUND.into_response(),
        Err(error) => internal_error_response(error),
    }
}

pub async fn book_readlists(
    State(app): State<DiscoveryState>,
    _: Authenticated,
    headers: HeaderMap,
    Path(book_id): Path<String>,
) -> Response {
    let book_id = resolve_book_id_for_persisted(&app, &book_id).await;

    let Some(resource) = (match load_persisted_book_resource(&app, &book_id).await {
        Ok(resource) => resource,
        Err(error) => return internal_error_response(error),
    }) else {
        return StatusCode::NOT_FOUND.into_response();
    };

    let detail_context = DetailResourceContext {
        library_id: Some(resource.library_id),
        content: Some(DetailContentContext {
            age_rating: resource.age_rating.map(u32::from),
            sharing_labels: resource.sharing_labels,
        }),
    };

    let detail_query_context = match app
        .discovery_auth
        .resolve_detail_query_context_with_persistence(
            &*app.identity.service,
            &headers,
            &detail_context,
        )
        .await
    {
        Ok(context) => context,
        Err(denial) => return detail_access_denial_response(denial),
    };

    let mut readlists = match load_persisted_readlists(
        &app,
        detail_query_context.authorized_library_ids.as_deref(),
    )
    .await
    {
        Ok(readlists) => readlists,
        Err(error) => return internal_error_response(error),
    };

    let detail_query = PersistedReadlistBooksQuery {
        page: 0,
        size: 20,
        unpaged: false,
        library_ids: None,
        deleted: None,
        tags: Vec::new(),
        read_statuses: Vec::new(),
        media_statuses: Vec::new(),
        authors: Vec::new(),
    };

    readlists.retain(|readlist| {
        readlist
            .book_ids
            .iter()
            .any(|candidate| candidate == &book_id)
    });

    let mut visible_readlists = Vec::with_capacity(readlists.len());
    for mut readlist in readlists {
        let Some(visible_books) = (match load_visible_persisted_readlist_books(
            &app,
            &headers,
            &readlist.id,
            &detail_query,
        )
        .await
        {
            Ok(books) => books,
            Err(error) => return internal_error_response(error),
        }) else {
            continue;
        };

        let visible_book_ids = visible_books
            .into_iter()
            .map(|book| book.id)
            .collect::<Vec<_>>();
        if !visible_book_ids
            .iter()
            .any(|candidate| candidate == &book_id)
        {
            continue;
        }

        readlist.filtered = readlist.book_ids != visible_book_ids;
        readlist.book_ids = visible_book_ids;
        visible_readlists.push(readlist);
    }

    Json(Value::Array(
        visible_readlists
            .iter()
            .map(readlist_payload)
            .collect::<Vec<_>>(),
    ))
    .into_response()
}
