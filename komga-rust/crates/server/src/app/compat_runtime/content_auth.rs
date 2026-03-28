use axum::Json;
use axum::extract::Path;
use axum::http::{HeaderMap, HeaderValue, StatusCode, Uri, header};
use axum::response::{IntoResponse, Response};
use axum_extra::extract::cookie::{Cookie, SameSite};
use bcrypt::{DEFAULT_COST, hash as hash_bcrypt_password};
use komga_persistence::sqlite::connect_pool;
use serde_json::Value;
use serde_json::json;
use sha2::Digest;
use sqlx::Row;
use std::collections::BTreeSet;
use std::io::Read;
use std::time::{SystemTime, UNIX_EPOCH};

use super::super::AuthDatabaseState;
use crate::app::discovery_auth::{DiscoveryAuthState, principal_from_user_payload};
use crate::app::runtime_auth::{
    AuthOutcome, AuthUser, PersistedAuthenticationActivity, auth_token_user,
    bootstrap_api_key_user, bootstrap_user, bootstrap_user_with_remember_me_cookies,
    bootstrap_user_with_remember_me_token, empty_auth_token_supplied, expired_remember_me_cookie,
    expired_session_cookie, invalidate_remember_me_token, invalidate_session_token,
    invalidate_user_sessions, persisted_api_key_comment_exists, persisted_api_key_metadata,
    persisted_api_key_user, persisted_basic_user, persisted_create_api_key,
    persisted_delete_api_key_by_id, persisted_latest_authentication_activity_by_user_and_api_key,
    persisted_list_api_keys, persisted_list_authentication_activity,
    persisted_record_successful_authentication_activity, persisted_update_password_by_user_id,
    persisted_users, remember_me_requested, remember_me_token_for_user_with_namespace,
    remember_me_token_from_headers, require_admin, require_auth, resolved_token,
    session_token_for_user_with_namespace, session_token_from_headers, unauthorized_json_response,
    user_id, user_is_admin, user_payload_json,
};

pub(super) async fn users_me(
    headers: HeaderMap,
    uri: Uri,
    auth_state: DiscoveryAuthState,
    auth_db: AuthDatabaseState,
) -> Response {
    match persisted_api_key_user(&headers, auth_db.database_file.as_path())
        .await
        .unwrap_or(AuthOutcome::Missing)
    {
        AuthOutcome::Valid(user) => {
            let api_key_metadata =
                persisted_api_key_metadata(&headers, auth_db.database_file.as_path()).await;
            let (api_key_id, api_key_comment) = api_key_metadata
                .as_ref()
                .map(|metadata| (Some(metadata.id.as_str()), Some(metadata.comment.as_str())))
                .unwrap_or((None, None));
            let _ = persisted_record_successful_authentication_activity(
                auth_db.database_file.as_path(),
                &user,
                "API_KEY",
                api_key_id,
                api_key_comment,
            )
            .await;
            let token = session_token_for_user_with_namespace(
                &user,
                auth_db.remember_me_namespace.as_str(),
            );
            register_discovery_principal(
                &auth_state,
                &crate::app::runtime_auth::user_payload_json(&user),
                &token,
            );
            return bootstrap_api_key_user(user, token);
        }
        AuthOutcome::Invalid => return unauthorized_json_response(uri.path()),
        AuthOutcome::Missing => {}
    }

    if let Some(user) = auth_token_user(&headers) {
        let token = session_token_from_headers(&headers).unwrap_or_else(|| {
            session_token_for_user_with_namespace(&user, auth_db.remember_me_namespace.as_str())
        });
        let payload = crate::app::runtime_auth::user_payload_json(&user);
        register_discovery_principal(&auth_state, &payload, &token);
        if session_token_from_headers(&headers).is_some() {
            return Json(payload).into_response();
        }
        return bootstrap_user(user, token);
    }

    match persisted_basic_user(&headers, auth_db.database_file.as_path())
        .await
        .unwrap_or(AuthOutcome::Missing)
    {
        AuthOutcome::Valid(user) if remember_me_requested(&uri) => {
            let _ = persisted_record_successful_authentication_activity(
                auth_db.database_file.as_path(),
                &user,
                "BASIC",
                None,
                None,
            )
            .await;
            let Some(remember_me_token) = remember_me_token_for_user_with_namespace(
                &user,
                auth_db.remember_me_namespace.as_str(),
            ) else {
                return StatusCode::INTERNAL_SERVER_ERROR.into_response();
            };
            if empty_auth_token_supplied(&headers) {
                let token = session_token_for_user_with_namespace(
                    &user,
                    auth_db.remember_me_namespace.as_str(),
                );
                register_discovery_principal(
                    &auth_state,
                    &crate::app::runtime_auth::user_payload_json(&user),
                    &token,
                );
                bootstrap_user_with_remember_me_token(user, token, remember_me_token)
            } else {
                let token = session_token_for_user_with_namespace(
                    &user,
                    auth_db.remember_me_namespace.as_str(),
                );
                register_discovery_principal(
                    &auth_state,
                    &crate::app::runtime_auth::user_payload_json(&user),
                    &token,
                );
                bootstrap_user_with_remember_me_cookies(user, token, remember_me_token)
            }
        }
        AuthOutcome::Valid(user) => {
            let _ = persisted_record_successful_authentication_activity(
                auth_db.database_file.as_path(),
                &user,
                "BASIC",
                None,
                None,
            )
            .await;
            let token = session_token_for_user_with_namespace(
                &user,
                auth_db.remember_me_namespace.as_str(),
            );
            register_discovery_principal(
                &auth_state,
                &crate::app::runtime_auth::user_payload_json(&user),
                &token,
            );
            bootstrap_user(user, token)
        }
        AuthOutcome::Invalid => StatusCode::UNAUTHORIZED.into_response(),
        AuthOutcome::Missing => StatusCode::UNAUTHORIZED.into_response(),
    }
}

pub(super) async fn login_set_cookie(headers: HeaderMap) -> Response {
    if let Some(response) = require_auth(&headers) {
        return response;
    }

    let token = resolved_token(&headers);
    let cookie = Cookie::build(("KOMGA-SESSION", token))
        .path("/")
        .http_only(true)
        .same_site(SameSite::Lax)
        .build()
        .to_string();

    (
        StatusCode::NO_CONTENT,
        [(
            header::SET_COOKIE,
            HeaderValue::from_str(&cookie)
                .unwrap_or_else(|_| HeaderValue::from_static("KOMGA-SESSION=; Path=/")),
        )],
    )
        .into_response()
}

pub(super) async fn users_list(headers: HeaderMap, auth_db: AuthDatabaseState) -> Response {
    if let Some(response) = require_admin(&headers) {
        return response;
    }

    let users = persisted_users(auth_db.database_file.as_path())
        .await
        .unwrap_or_default();

    Json(users.iter().map(user_payload_json).collect::<Vec<_>>()).into_response()
}

pub(super) async fn users_create(
    headers: HeaderMap,
    body: Value,
    auth_db: AuthDatabaseState,
) -> Response {
    let Some(current_user) = authenticated_user(&headers, &auth_db).await else {
        return StatusCode::UNAUTHORIZED.into_response();
    };
    if !user_is_admin(&current_user) {
        return StatusCode::FORBIDDEN.into_response();
    }

    let Some(payload) = body.as_object() else {
        return StatusCode::BAD_REQUEST.into_response();
    };

    let Some(email) = payload.get("email").and_then(Value::as_str).map(str::trim) else {
        return StatusCode::BAD_REQUEST.into_response();
    };
    if email.is_empty() || !looks_like_kotlin_user_email(email) {
        return StatusCode::BAD_REQUEST.into_response();
    }

    let Some(password) = payload
        .get("password")
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
    else {
        return StatusCode::BAD_REQUEST.into_response();
    };

    let roles = match parse_roles_array(payload.get("roles")) {
        Ok(roles) => roles,
        Err(response) => return response,
    };
    let labels_allow = match parse_string_set_optional(payload.get("labelsAllow")) {
        Ok(labels) => labels,
        Err(response) => return response,
    }
    .unwrap_or_default();
    let labels_exclude = match parse_string_set_optional(payload.get("labelsExclude")) {
        Ok(labels) => labels,
        Err(response) => return response,
    }
    .unwrap_or_default();
    let age_restriction = match parse_age_restriction_optional(payload.get("ageRestriction")) {
        Ok(age_restriction) => age_restriction,
        Err(response) => return response,
    };

    let new_user_id = generated_user_id();
    let hashed_password = match hash_bcrypt_password(password, DEFAULT_COST) {
        Ok(hash) => hash,
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };

    let pool = match connect_pool(auth_db.database_file.as_path(), 1).await {
        Ok(pool) => pool,
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };
    let mut tx = match pool.begin().await {
        Ok(tx) => tx,
        Err(_) => {
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };

    let email_exists = match sqlx::query(
        "SELECT 1 \
                                          FROM USER \
                                          WHERE LOWER(EMAIL) = LOWER(?) \
                                          LIMIT 1",
    )
    .bind(email)
    .fetch_optional(&mut *tx)
    .await
    {
        Ok(Some(_)) => true,
        Ok(None) => false,
        Err(_) => {
            let _ = tx.rollback().await;
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };
    if email_exists {
        let _ = tx.rollback().await;
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "message": "A user with this email already exists" })),
        )
            .into_response();
    }

    let shared_libraries =
        match parse_shared_libraries_create(payload.get("sharedLibraries"), &mut tx).await {
            Ok(shared_libraries) => shared_libraries,
            Err(response) => {
                let _ = tx.rollback().await;
                return response;
            }
        };

    let age = age_restriction.as_ref().map(|value| value.age);
    let allow_only = age_restriction.as_ref().map(|value| value.allow_only);

    if sqlx::query(
        "INSERT INTO USER (ID, EMAIL, PASSWORD, SHARED_ALL_LIBRARIES, AGE_RESTRICTION, \
           AGE_RESTRICTION_ALLOW_ONLY) \
         VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind(&new_user_id)
    .bind(email)
    .bind(hashed_password)
    .bind(shared_libraries.all)
    .bind(age)
    .bind(allow_only)
    .execute(&mut *tx)
    .await
    .is_err()
    {
        let _ = tx.rollback().await;
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    }

    for role in &roles {
        if sqlx::query(
            "INSERT \
                        OR IGNORE INTO USER_ROLE (USER_ID, ROLE) \
                        VALUES (?, ?)",
        )
        .bind(&new_user_id)
        .bind(role)
        .execute(&mut *tx)
        .await
        .is_err()
        {
            let _ = tx.rollback().await;
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    }

    if !shared_libraries.all {
        for library_id in &shared_libraries.library_ids {
            if sqlx::query(
                "INSERT \
                 OR IGNORE INTO USER_LIBRARY_SHARING (USER_ID, LIBRARY_ID) \
                 VALUES (?, ?)",
            )
            .bind(&new_user_id)
            .bind(library_id)
            .execute(&mut *tx)
            .await
            .is_err()
            {
                let _ = tx.rollback().await;
                return StatusCode::INTERNAL_SERVER_ERROR.into_response();
            }
        }
    }

    for label in &labels_allow {
        if sqlx::query(
            "INSERT \
             OR IGNORE INTO USER_SHARING (LABEL, ALLOW, USER_ID) \
             VALUES (?, ?, ?)",
        )
        .bind(label)
        .bind(true)
        .bind(&new_user_id)
        .execute(&mut *tx)
        .await
        .is_err()
        {
            let _ = tx.rollback().await;
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    }
    for label in &labels_exclude {
        if sqlx::query(
            "INSERT \
             OR IGNORE INTO USER_SHARING (LABEL, ALLOW, USER_ID) \
             VALUES (?, ?, ?)",
        )
        .bind(label)
        .bind(false)
        .bind(&new_user_id)
        .execute(&mut *tx)
        .await
        .is_err()
        {
            let _ = tx.rollback().await;
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    }

    if tx.commit().await.is_err() {
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    }

    let created_user = persisted_users(auth_db.database_file.as_path())
        .await
        .and_then(|users| {
            users
                .into_iter()
                .find(|candidate| user_id(candidate) == new_user_id)
        });

    match created_user {
        Some(user) => (StatusCode::CREATED, Json(user_payload_json(&user))).into_response(),
        None => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
}

pub(super) async fn users_delete(
    headers: HeaderMap,
    Path(target_user_id): Path<String>,
    auth_db: AuthDatabaseState,
) -> Response {
    let Some(current_user) = authenticated_user(&headers, &auth_db).await else {
        return StatusCode::UNAUTHORIZED.into_response();
    };
    if !user_is_admin(&current_user) && user_id(&current_user) != target_user_id {
        return StatusCode::FORBIDDEN.into_response();
    }
    if user_id(&current_user) == target_user_id {
        return StatusCode::FORBIDDEN.into_response();
    }

    let pool = match connect_pool(auth_db.database_file.as_path(), 1).await {
        Ok(pool) => pool,
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };
    let mut tx = match pool.begin().await {
        Ok(tx) => tx,
        Err(_) => {
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };

    let exists = match sqlx::query(
        "SELECT 1 \
                                    FROM USER \
                                    WHERE ID = ? \
                                    LIMIT 1",
    )
    .bind(&target_user_id)
    .fetch_optional(&mut *tx)
    .await
    {
        Ok(Some(_)) => true,
        Ok(None) => false,
        Err(_) => {
            let _ = tx.rollback().await;
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };
    if !exists {
        let _ = tx.rollback().await;
        return StatusCode::NOT_FOUND.into_response();
    }

    let sync_point_ids = match sqlx::query(
        "SELECT ID \
                                            FROM SYNC_POINT \
                                            WHERE USER_ID = ?",
    )
    .bind(&target_user_id)
    .fetch_all(&mut *tx)
    .await
    {
        Ok(rows) => rows
            .into_iter()
            .map(|row| row.get::<String, _>("ID"))
            .collect::<Vec<_>>(),
        Err(_) => {
            let _ = tx.rollback().await;
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };

    for sync_point_id in &sync_point_ids {
        for sql in [
            "DELETE \
             FROM SYNC_POINT_BOOK \
             WHERE SYNC_POINT_ID = ?",
            "DELETE \
             FROM SYNC_POINT_BOOK_REMOVED_SYNCED \
             WHERE SYNC_POINT_ID = ?",
            "DELETE \
             FROM SYNC_POINT_READLIST \
             WHERE SYNC_POINT_ID = ?",
            "DELETE \
             FROM SYNC_POINT_READLIST_BOOK \
             WHERE SYNC_POINT_ID = ?",
            "DELETE \
             FROM SYNC_POINT_READLIST_REMOVED_SYNCED \
             WHERE SYNC_POINT_ID = ?",
        ] {
            if sqlx::query(sql)
                .bind(sync_point_id)
                .execute(&mut *tx)
                .await
                .is_err()
            {
                let _ = tx.rollback().await;
                return StatusCode::INTERNAL_SERVER_ERROR.into_response();
            }
        }
    }

    for sql in [
        "DELETE \
         FROM SYNC_POINT \
         WHERE USER_ID = ?",
        "DELETE \
         FROM AUTHENTICATION_ACTIVITY \
         WHERE USER_ID = ?",
        "DELETE \
         FROM USER_API_KEY \
         WHERE USER_ID = ?",
        "DELETE \
         FROM USER_ROLE \
         WHERE USER_ID = ?",
        "DELETE \
         FROM USER_LIBRARY_SHARING \
         WHERE USER_ID = ?",
        "DELETE \
         FROM USER_SHARING \
         WHERE USER_ID = ?",
        "DELETE \
         FROM CLIENT_SETTINGS_USER \
         WHERE USER_ID = ?",
        "DELETE \
         FROM READ_PROGRESS \
         WHERE USER_ID = ?",
        "DELETE \
         FROM READ_PROGRESS_SERIES \
         WHERE USER_ID = ?",
        "DELETE \
         FROM ANNOUNCEMENTS_READ \
         WHERE USER_ID = ?",
    ] {
        if sqlx::query(sql)
            .bind(&target_user_id)
            .execute(&mut *tx)
            .await
            .is_err()
        {
            let _ = tx.rollback().await;
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    }

    if sqlx::query(
        "DELETE \
                    FROM USER \
                    WHERE ID = ?",
    )
    .bind(&target_user_id)
    .execute(&mut *tx)
    .await
    .is_err()
    {
        let _ = tx.rollback().await;
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    }

    if tx.commit().await.is_err() {
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    }

    invalidate_user_sessions(&target_user_id);
    StatusCode::NO_CONTENT.into_response()
}

pub(super) async fn users_update(
    headers: HeaderMap,
    Path(target_user_id): Path<String>,
    body: Value,
    auth_db: AuthDatabaseState,
) -> Response {
    let Some(current_user) = authenticated_user(&headers, &auth_db).await else {
        return StatusCode::UNAUTHORIZED.into_response();
    };
    if !user_is_admin(&current_user) {
        return StatusCode::FORBIDDEN.into_response();
    }
    if user_id(&current_user) == target_user_id {
        return StatusCode::FORBIDDEN.into_response();
    }

    let Some(payload) = body.as_object() else {
        return StatusCode::BAD_REQUEST.into_response();
    };

    let roles_patch = if payload.contains_key("roles") {
        match parse_roles_array(payload.get("roles")) {
            Ok(roles) => Some(roles),
            Err(response) => return response,
        }
    } else {
        None
    };

    let shared_libraries_patch_raw = if payload.contains_key("sharedLibraries") {
        match parse_shared_libraries_patch(payload.get("sharedLibraries")) {
            Ok(shared_libraries) => Some(shared_libraries),
            Err(response) => return response,
        }
    } else {
        None
    };

    let labels_allow_patch = if payload.contains_key("labelsAllow") {
        match parse_string_set_optional(payload.get("labelsAllow")) {
            Ok(labels) => Some(labels.unwrap_or_default()),
            Err(response) => return response,
        }
    } else {
        None
    };

    let labels_exclude_patch = if payload.contains_key("labelsExclude") {
        match parse_string_set_optional(payload.get("labelsExclude")) {
            Ok(labels) => Some(labels.unwrap_or_default()),
            Err(response) => return response,
        }
    } else {
        None
    };

    let age_restriction_patch = if payload.contains_key("ageRestriction") {
        match parse_age_restriction_optional(payload.get("ageRestriction")) {
            Ok(age_restriction) => Some(age_restriction),
            Err(response) => return response,
        }
    } else {
        None
    };

    let pool = match connect_pool(auth_db.database_file.as_path(), 1).await {
        Ok(pool) => pool,
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };
    let mut tx = match pool.begin().await {
        Ok(tx) => tx,
        Err(_) => {
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };

    let Some(user_row) = (match sqlx::query(
        "SELECT SHARED_ALL_LIBRARIES, AGE_RESTRICTION, AGE_RESTRICTION_ALLOW_ONLY \
         FROM USER \
         WHERE ID = ? \
         LIMIT 1",
    )
    .bind(&target_user_id)
    .fetch_optional(&mut *tx)
    .await
    {
        Ok(row) => row,
        Err(_) => {
            let _ = tx.rollback().await;
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    }) else {
        let _ = tx.rollback().await;
        return StatusCode::NOT_FOUND.into_response();
    };

    let shared_libraries_patch = if let Some(shared_libraries) = shared_libraries_patch_raw {
        match resolve_shared_libraries(&mut tx, shared_libraries).await {
            Ok(shared_libraries) => Some(shared_libraries),
            Err(response) => {
                let _ = tx.rollback().await;
                return response;
            }
        }
    } else {
        None
    };

    let mut shared_all_libraries = user_row.get::<bool, _>("SHARED_ALL_LIBRARIES");
    if let Some(shared_libraries) = &shared_libraries_patch {
        shared_all_libraries = shared_libraries.all;
    }

    let mut age_restriction = user_row.get::<Option<i64>, _>("AGE_RESTRICTION");
    let mut age_restriction_allow_only =
        user_row.get::<Option<bool>, _>("AGE_RESTRICTION_ALLOW_ONLY");
    if let Some(age_patch) = &age_restriction_patch {
        age_restriction = age_patch.as_ref().map(|value| value.age);
        age_restriction_allow_only = age_patch.as_ref().map(|value| value.allow_only);
    }

    if sqlx::query(
        "UPDATE USER \
         SET SHARED_ALL_LIBRARIES = ?, AGE_RESTRICTION = ?, AGE_RESTRICTION_ALLOW_ONLY = ?, \
             LAST_MODIFIED_DATE = CURRENT_TIMESTAMP \
         WHERE ID = ?",
    )
    .bind(shared_all_libraries)
    .bind(age_restriction)
    .bind(age_restriction_allow_only)
    .bind(&target_user_id)
    .execute(&mut *tx)
    .await
    .is_err()
    {
        let _ = tx.rollback().await;
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    }

    if let Some(roles) = &roles_patch {
        if sqlx::query(
            "DELETE \
                        FROM USER_ROLE \
                        WHERE USER_ID = ?",
        )
        .bind(&target_user_id)
        .execute(&mut *tx)
        .await
        .is_err()
        {
            let _ = tx.rollback().await;
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
        for role in roles {
            if sqlx::query(
                "INSERT \
                            OR IGNORE INTO USER_ROLE (USER_ID, ROLE) \
                            VALUES (?, ?)",
            )
            .bind(&target_user_id)
            .bind(role)
            .execute(&mut *tx)
            .await
            .is_err()
            {
                let _ = tx.rollback().await;
                return StatusCode::INTERNAL_SERVER_ERROR.into_response();
            }
        }
    }

    if let Some(shared_libraries) = &shared_libraries_patch {
        if sqlx::query(
            "DELETE \
                        FROM USER_LIBRARY_SHARING \
                        WHERE USER_ID = ?",
        )
        .bind(&target_user_id)
        .execute(&mut *tx)
        .await
        .is_err()
        {
            let _ = tx.rollback().await;
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
        for library_id in &shared_libraries.library_ids {
            if sqlx::query(
                "INSERT \
                 OR IGNORE INTO USER_LIBRARY_SHARING (USER_ID, LIBRARY_ID) \
                 VALUES (?, ?)",
            )
            .bind(&target_user_id)
            .bind(library_id)
            .execute(&mut *tx)
            .await
            .is_err()
            {
                let _ = tx.rollback().await;
                return StatusCode::INTERNAL_SERVER_ERROR.into_response();
            }
        }
    }

    if labels_allow_patch.is_some() || labels_exclude_patch.is_some() {
        let (existing_allow, existing_exclude) =
            match load_user_sharing_labels(&mut tx, &target_user_id).await {
                Ok(labels) => labels,
                Err(_) => {
                    let _ = tx.rollback().await;
                    return StatusCode::INTERNAL_SERVER_ERROR.into_response();
                }
            };

        let labels_allow = labels_allow_patch.unwrap_or(existing_allow);
        let labels_exclude = labels_exclude_patch.unwrap_or(existing_exclude);

        if sqlx::query(
            "DELETE \
                        FROM USER_SHARING \
                        WHERE USER_ID = ?",
        )
        .bind(&target_user_id)
        .execute(&mut *tx)
        .await
        .is_err()
        {
            let _ = tx.rollback().await;
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
        for label in &labels_allow {
            if sqlx::query(
                "INSERT \
                 OR IGNORE INTO USER_SHARING (LABEL, ALLOW, USER_ID) \
                 VALUES (?, ?, ?)",
            )
            .bind(label)
            .bind(true)
            .bind(&target_user_id)
            .execute(&mut *tx)
            .await
            .is_err()
            {
                let _ = tx.rollback().await;
                return StatusCode::INTERNAL_SERVER_ERROR.into_response();
            }
        }
        for label in &labels_exclude {
            if sqlx::query(
                "INSERT \
                 OR IGNORE INTO USER_SHARING (LABEL, ALLOW, USER_ID) \
                 VALUES (?, ?, ?)",
            )
            .bind(label)
            .bind(false)
            .bind(&target_user_id)
            .execute(&mut *tx)
            .await
            .is_err()
            {
                let _ = tx.rollback().await;
                return StatusCode::INTERNAL_SERVER_ERROR.into_response();
            }
        }
    }

    if tx.commit().await.is_err() {
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    }

    invalidate_user_sessions(&target_user_id);
    StatusCode::NO_CONTENT.into_response()
}

pub(super) async fn logout(headers: HeaderMap) -> Response {
    if let Some(response) = require_auth(&headers) {
        return response;
    }

    if let Some(token) = session_token_from_headers(&headers) {
        invalidate_session_token(&token);
    }
    if let Some(remember_me_token) = remember_me_token_from_headers(&headers) {
        invalidate_remember_me_token(&remember_me_token);
    }

    let mut response = StatusCode::NO_CONTENT.into_response();
    let headers = response.headers_mut();
    headers.append(header::SET_COOKIE, expired_session_cookie());
    headers.append(header::SET_COOKIE, expired_remember_me_cookie());
    response
}

pub(super) async fn users_me_password(
    headers: HeaderMap,
    body: Value,
    auth_db: AuthDatabaseState,
) -> Response {
    let Some(current_user) = authenticated_user(&headers, &auth_db).await else {
        return StatusCode::UNAUTHORIZED.into_response();
    };

    let Some(password) = password_from_request(&body) else {
        return StatusCode::BAD_REQUEST.into_response();
    };

    match persisted_update_password_by_user_id(
        auth_db.database_file.as_path(),
        user_id(&current_user),
        password,
    )
    .await
    {
        Some(true) => StatusCode::NO_CONTENT.into_response(),
        Some(false) => StatusCode::NOT_FOUND.into_response(),
        None => StatusCode::NOT_FOUND.into_response(),
    }
}

pub(super) async fn users_by_id_password(
    headers: HeaderMap,
    Path(target_user_id): Path<String>,
    body: Value,
    auth_db: AuthDatabaseState,
) -> Response {
    let Some(current_user) = authenticated_user(&headers, &auth_db).await else {
        return StatusCode::UNAUTHORIZED.into_response();
    };
    if !user_is_admin(&current_user) {
        return StatusCode::FORBIDDEN.into_response();
    }

    let Some(password) = password_from_request(&body) else {
        return StatusCode::BAD_REQUEST.into_response();
    };

    match persisted_update_password_by_user_id(
        auth_db.database_file.as_path(),
        &target_user_id,
        password,
    )
    .await
    {
        Some(true) => {
            invalidate_user_sessions(&target_user_id);
            StatusCode::NO_CONTENT.into_response()
        }
        Some(false) => StatusCode::NOT_FOUND.into_response(),
        None => StatusCode::NOT_FOUND.into_response(),
    }
}

pub(super) async fn users_me_api_keys_create(
    headers: HeaderMap,
    body: Value,
    auth_db: AuthDatabaseState,
) -> Response {
    let Some(current_user) = authenticated_user(&headers, &auth_db).await else {
        return StatusCode::UNAUTHORIZED.into_response();
    };
    let Some(comment) = api_key_comment_from_request(&body) else {
        return StatusCode::BAD_REQUEST.into_response();
    };

    match persisted_api_key_comment_exists(
        auth_db.database_file.as_path(),
        user_id(&current_user),
        &comment,
    )
    .await
    {
        Some(true) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({ "message": "api key comment already exists for this user" })),
            )
                .into_response();
        }
        Some(false) => {}
        None => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }

    match persisted_create_api_key(
        auth_db.database_file.as_path(),
        user_id(&current_user),
        comment.as_str(),
    )
    .await
    {
        Some(api_key) => (
            StatusCode::OK,
            Json(serde_json::json!({
                "id": api_key.id,
                "userId": api_key.user_id,
                "key": api_key.key,
                "comment": api_key.comment,
            })),
        )
            .into_response(),
        None => StatusCode::SERVICE_UNAVAILABLE.into_response(),
    }
}

pub(super) async fn users_me_api_keys_list(
    headers: HeaderMap,
    auth_db: AuthDatabaseState,
) -> Response {
    let Some(current_user) = authenticated_user(&headers, &auth_db).await else {
        return StatusCode::UNAUTHORIZED.into_response();
    };

    let api_keys = persisted_list_api_keys(auth_db.database_file.as_path(), user_id(&current_user))
        .await
        .unwrap_or_default();

    Json(
        api_keys
            .iter()
            .map(|api_key| {
                serde_json::json!({
                    "id": api_key.id,
                    "userId": api_key.user_id,
                    "key": api_key.key,
                    "comment": api_key.comment,
                })
            })
            .collect::<Vec<_>>(),
    )
    .into_response()
}

pub(super) async fn users_me_api_keys_delete(
    headers: HeaderMap,
    Path(api_key_id): Path<String>,
    auth_db: AuthDatabaseState,
) -> Response {
    let Some(current_user) = authenticated_user(&headers, &auth_db).await else {
        return StatusCode::UNAUTHORIZED.into_response();
    };

    match persisted_delete_api_key_by_id(
        auth_db.database_file.as_path(),
        user_id(&current_user),
        &api_key_id,
    )
    .await
    {
        Some(true) => StatusCode::NO_CONTENT.into_response(),
        Some(false) => StatusCode::NOT_FOUND.into_response(),
        None => StatusCode::NOT_FOUND.into_response(),
    }
}

pub(super) async fn users_me_authentication_activity(
    headers: HeaderMap,
    uri: Uri,
    auth_db: AuthDatabaseState,
) -> Response {
    let Some(current_user) = authenticated_user(&headers, &auth_db).await else {
        return StatusCode::UNAUTHORIZED.into_response();
    };

    let unpaged = query_bool(uri.query().unwrap_or_default(), "unpaged");
    let rows = persisted_list_authentication_activity(
        auth_db.database_file.as_path(),
        Some(user_id(&current_user)),
    )
    .await
    .unwrap_or_default();

    Json(authentication_activity_page_payload(rows, unpaged)).into_response()
}

pub(super) async fn users_authentication_activity(
    headers: HeaderMap,
    uri: Uri,
    auth_db: AuthDatabaseState,
) -> Response {
    let Some(current_user) = authenticated_user(&headers, &auth_db).await else {
        return StatusCode::UNAUTHORIZED.into_response();
    };
    if !user_is_admin(&current_user) {
        return StatusCode::FORBIDDEN.into_response();
    }

    let unpaged = query_bool(uri.query().unwrap_or_default(), "unpaged");
    let rows = persisted_list_authentication_activity(auth_db.database_file.as_path(), None)
        .await
        .unwrap_or_default();

    Json(authentication_activity_page_payload(rows, unpaged)).into_response()
}

pub(super) async fn users_by_id_authentication_activity_latest(
    headers: HeaderMap,
    Path(target_user_id): Path<String>,
    uri: Uri,
    auth_db: AuthDatabaseState,
) -> Response {
    let Some(current_user) = authenticated_user(&headers, &auth_db).await else {
        return StatusCode::UNAUTHORIZED.into_response();
    };
    if !user_is_admin(&current_user) && user_id(&current_user) != target_user_id {
        return StatusCode::FORBIDDEN.into_response();
    }

    let api_key_id = query_value(uri.query().unwrap_or_default(), "apikey_id")
        .and_then(|value| (!value.is_empty()).then_some(value));

    let activity = if let Some(api_key_id) = api_key_id {
        persisted_latest_authentication_activity_by_user_and_api_key(
            auth_db.database_file.as_path(),
            &target_user_id,
            api_key_id,
        )
        .await
    } else {
        persisted_list_authentication_activity(
            auth_db.database_file.as_path(),
            Some(&target_user_id),
        )
        .await
        .and_then(|rows| rows.into_iter().next())
    };

    let Some(activity) = activity else {
        return StatusCode::NOT_FOUND.into_response();
    };

    Json(authentication_activity_payload(&activity)).into_response()
}

fn register_discovery_principal(
    auth_state: &DiscoveryAuthState,
    payload: &serde_json::Value,
    token: &str,
) {
    if let Some(principal) = principal_from_user_payload(payload) {
        auth_state.register_session_principal(token, principal);
    }
}

#[derive(Clone, Debug)]
struct SharedLibrariesPatch {
    all: bool,
    library_ids: Vec<String>,
}

#[derive(Clone, Debug)]
struct AgeRestrictionPatch {
    age: i64,
    allow_only: bool,
}

fn bad_request(message: &str) -> Response {
    (StatusCode::BAD_REQUEST, Json(json!({ "message": message }))).into_response()
}

fn generated_user_id() -> String {
    let mut bytes = [0u8; 16];
    if let Ok(mut file) = std::fs::File::open("/dev/urandom") {
        let _ = file.read_exact(&mut bytes);
    } else {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
            .to_string();
        let mut hasher = sha2::Sha256::new();
        hasher.update(nanos.as_bytes());
        let digest = hasher.finalize();
        bytes.copy_from_slice(&digest[..16]);
    }

    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    format!(
        "{:02x}{:02x}{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
        bytes[0],
        bytes[1],
        bytes[2],
        bytes[3],
        bytes[4],
        bytes[5],
        bytes[6],
        bytes[7],
        bytes[8],
        bytes[9],
        bytes[10],
        bytes[11],
        bytes[12],
        bytes[13],
        bytes[14],
        bytes[15],
    )
}

fn looks_like_kotlin_user_email(value: &str) -> bool {
    let mut parts = value.split('@');
    let Some(local) = parts.next() else {
        return false;
    };
    let Some(domain) = parts.next() else {
        return false;
    };
    if parts.next().is_some() {
        return false;
    }

    if local.is_empty() || domain.is_empty() {
        return false;
    }

    let mut domain_segments = domain.split('.');
    let has_all_non_empty_segments = domain_segments.all(|segment| !segment.is_empty());
    has_all_non_empty_segments && domain.contains('.')
}

fn parse_roles_array(value: Option<&Value>) -> Result<Vec<String>, Response> {
    let Some(value) = value else {
        return Ok(Vec::new());
    };
    if value.is_null() {
        return Ok(Vec::new());
    }

    let Some(values) = value.as_array() else {
        return Err(bad_request("roles must be an array of strings"));
    };

    let mut roles = BTreeSet::new();
    for value in values {
        let Some(role) = value.as_str() else {
            return Err(bad_request("roles must be an array of strings"));
        };
        if matches!(
            role,
            "ADMIN" | "FILE_DOWNLOAD" | "PAGE_STREAMING" | "KOBO_SYNC" | "KOREADER_SYNC"
        ) {
            roles.insert(role.to_string());
        }
    }
    Ok(roles.into_iter().collect())
}

fn parse_string_set_optional(value: Option<&Value>) -> Result<Option<Vec<String>>, Response> {
    let Some(value) = value else {
        return Ok(None);
    };
    if value.is_null() {
        return Ok(Some(Vec::new()));
    }

    let Some(values) = value.as_array() else {
        return Err(bad_request("labels must be an array of strings"));
    };

    let mut labels = BTreeSet::new();
    for value in values {
        let Some(label) = value.as_str() else {
            return Err(bad_request("labels must be an array of strings"));
        };
        let label = label.trim();
        if label.is_empty() {
            continue;
        }
        labels.insert(label.to_string());
    }

    Ok(Some(labels.into_iter().collect()))
}

fn parse_age_restriction_optional(
    value: Option<&Value>,
) -> Result<Option<AgeRestrictionPatch>, Response> {
    let Some(value) = value else {
        return Ok(None);
    };
    if value.is_null() {
        return Ok(None);
    }

    let Some(object) = value.as_object() else {
        return Err(bad_request("ageRestriction must be an object"));
    };

    let Some(age) = object.get("age").and_then(Value::as_i64) else {
        return Err(bad_request("ageRestriction.age must be an integer"));
    };
    if age < 0 {
        return Err(bad_request("ageRestriction.age must be >= 0"));
    }

    let Some(restriction) = object.get("restriction").and_then(Value::as_str) else {
        return Err(bad_request(
            "ageRestriction.restriction must be ALLOW_ONLY, EXCLUDE, or NONE",
        ));
    };

    match restriction {
        "ALLOW_ONLY" => Ok(Some(AgeRestrictionPatch {
            age,
            allow_only: true,
        })),
        "EXCLUDE" => Ok(Some(AgeRestrictionPatch {
            age,
            allow_only: false,
        })),
        "NONE" => Ok(None),
        _ => Err(bad_request(
            "ageRestriction.restriction must be ALLOW_ONLY, EXCLUDE, or NONE",
        )),
    }
}

fn parse_shared_libraries_patch(value: Option<&Value>) -> Result<SharedLibrariesPatch, Response> {
    let Some(value) = value else {
        return Err(bad_request("sharedLibraries is required"));
    };
    let Some(object) = value.as_object() else {
        return Err(bad_request("sharedLibraries must be an object"));
    };

    let Some(all) = object.get("all").and_then(Value::as_bool) else {
        return Err(bad_request("sharedLibraries.all must be a boolean"));
    };

    let library_ids = if all {
        Vec::new()
    } else {
        let Some(ids) = object.get("libraryIds").and_then(Value::as_array) else {
            return Err(bad_request(
                "sharedLibraries.libraryIds must be an array of strings",
            ));
        };

        let mut normalized = BTreeSet::new();
        for value in ids {
            let Some(library_id) = value.as_str() else {
                return Err(bad_request(
                    "sharedLibraries.libraryIds must be an array of strings",
                ));
            };
            let library_id = library_id.trim();
            if library_id.is_empty() {
                continue;
            }
            normalized.insert(library_id.to_string());
        }
        normalized.into_iter().collect::<Vec<_>>()
    };

    Ok(SharedLibrariesPatch { all, library_ids })
}

async fn parse_shared_libraries_create(
    value: Option<&Value>,
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
) -> Result<SharedLibrariesPatch, Response> {
    let Some(value) = value else {
        return Ok(SharedLibrariesPatch {
            all: true,
            library_ids: Vec::new(),
        });
    };
    let parsed = parse_shared_libraries_patch(Some(value))?;
    resolve_shared_libraries(tx, parsed).await
}

async fn resolve_shared_libraries(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    shared_libraries: SharedLibrariesPatch,
) -> Result<SharedLibrariesPatch, Response> {
    if shared_libraries.all {
        return Ok(SharedLibrariesPatch {
            all: true,
            library_ids: Vec::new(),
        });
    }

    let rows = match sqlx::query(
        "SELECT ID \
                                  FROM LIBRARY \
                                  ORDER BY ID",
    )
    .fetch_all(&mut **tx)
    .await
    {
        Ok(rows) => rows,
        Err(_) => return Err(StatusCode::INTERNAL_SERVER_ERROR.into_response()),
    };

    let existing = rows
        .into_iter()
        .map(|row| row.get::<String, _>("ID"))
        .collect::<BTreeSet<_>>();

    let library_ids = shared_libraries
        .library_ids
        .into_iter()
        .filter(|library_id| existing.contains(library_id))
        .collect::<Vec<_>>();

    Ok(SharedLibrariesPatch {
        all: false,
        library_ids,
    })
}

async fn load_user_sharing_labels(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    user_id: &str,
) -> Result<(Vec<String>, Vec<String>), sqlx::Error> {
    let rows = sqlx::query(
        "SELECT LABEL, ALLOW \
                     FROM USER_SHARING \
                     WHERE USER_ID = ? \
                     ORDER BY LABEL",
    )
    .bind(user_id)
    .fetch_all(&mut **tx)
    .await?;

    let mut allow = Vec::new();
    let mut exclude = Vec::new();
    for row in rows {
        let label = row.get::<String, _>("LABEL");
        if row.get::<bool, _>("ALLOW") {
            allow.push(label);
        } else {
            exclude.push(label);
        }
    }
    Ok((allow, exclude))
}

fn password_from_request(body: &Value) -> Option<&str> {
    body.get("password")?
        .as_str()
        .filter(|password| !password.trim().is_empty())
}

fn api_key_comment_from_request(body: &Value) -> Option<String> {
    let comment = body.get("comment")?.as_str()?.trim();
    if comment.is_empty() {
        None
    } else {
        Some(comment.to_string())
    }
}

async fn authenticated_user(headers: &HeaderMap, auth_db: &AuthDatabaseState) -> Option<AuthUser> {
    match persisted_api_key_user(headers, auth_db.database_file.as_path())
        .await
        .unwrap_or(AuthOutcome::Missing)
    {
        AuthOutcome::Valid(user) => return Some(user),
        AuthOutcome::Invalid => return None,
        AuthOutcome::Missing => {}
    }

    if let Some(user) = auth_token_user(headers) {
        return Some(user);
    }

    match persisted_basic_user(headers, auth_db.database_file.as_path())
        .await
        .unwrap_or(AuthOutcome::Missing)
    {
        AuthOutcome::Valid(user) => Some(user),
        AuthOutcome::Invalid | AuthOutcome::Missing => None,
    }
}

fn authentication_activity_page_payload(
    rows: Vec<PersistedAuthenticationActivity>,
    unpaged: bool,
) -> Value {
    let content = rows
        .iter()
        .map(authentication_activity_payload)
        .collect::<Vec<_>>();
    let number_of_elements = content.len() as u64;
    let page_size = if unpaged { number_of_elements } else { 20 };
    let total_pages = if unpaged {
        1
    } else if number_of_elements == 0 {
        0
    } else {
        number_of_elements.div_ceil(page_size)
    };

    json!({
        "content": content,
        "pageable": {
            "pageNumber": 0,
            "pageSize": page_size,
            "sort": {
                "empty": false,
                "sorted": true,
                "unsorted": false
            },
            "offset": 0,
            "paged": !unpaged,
            "unpaged": unpaged
        },
        "last": true,
        "totalElements": number_of_elements,
        "totalPages": total_pages,
        "first": true,
        "size": page_size,
        "number": 0,
        "sort": {
            "empty": false,
            "sorted": true,
            "unsorted": false
        },
        "numberOfElements": number_of_elements,
        "empty": number_of_elements == 0
    })
}

fn authentication_activity_payload(activity: &PersistedAuthenticationActivity) -> Value {
    json!({
        "userId": activity.user_id,
        "email": activity.email,
        "ip": activity.ip,
        "userAgent": activity.user_agent,
        "success": activity.success,
        "error": activity.error,
        "dateTime": sqlite_datetime_to_utc(&activity.date_time),
        "source": activity.source,
        "apiKeyId": activity.api_key_id,
        "apiKeyComment": activity.api_key_comment,
    })
}

fn sqlite_datetime_to_utc(value: &str) -> String {
    if value.ends_with('Z') || value.contains('T') {
        value.to_string()
    } else if let Some((date, time)) = value.split_once(' ') {
        format!("{date}T{time}Z")
    } else {
        value.to_string()
    }
}

fn query_value<'a>(query: &'a str, key: &str) -> Option<&'a str> {
    query.split('&').find_map(|pair| {
        let mut parts = pair.splitn(2, '=');
        let name = parts.next().unwrap_or_default();
        if name != key {
            return None;
        }
        Some(parts.next().unwrap_or_default())
    })
}

fn query_bool(query: &str, key: &str) -> bool {
    query_value(query, key)
        .map(|value| value.eq_ignore_ascii_case("true"))
        .unwrap_or(false)
}
