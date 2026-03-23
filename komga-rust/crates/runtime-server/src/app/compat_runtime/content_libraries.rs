use axum::Json;
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Response};
use komga_application::discovery::{DiscoveryQueries, LibraryListQuery};
use komga_domain::discovery::{
    AgeRestrictionKind as DomainAgeRestrictionKind, DiscoveryError,
    DiscoveryQueryContext as DomainDiscoveryQueryContext, LibraryReadModel,
    QueryRestrictions as DomainQueryRestrictions,
};
use komga_persistence::read_models::{LibraryRow, SqliteDiscoveryAdapter};
use reqwest::header::{AUTHORIZATION, COOKIE};
use serde_json::{Value, json};

use crate::app::CompatProfile;
use crate::app::discovery_auth::{
    AgeRestrictionKind, DiscoveryAuthState, DiscoveryQueryContext, QueryRestrictions,
};
use crate::app::placeholder_auth::{
    PlaceholderUser, require_auth, resolved_auth_user, user_is_admin, user_shared_all_libraries,
    user_shared_library_ids,
};
use crate::app::snapshots::snapshot_json;

use super::{DiscoveryOwnershipRoute, DiscoveryShape, discovery_ownership_route, mark_native};

pub(super) async fn response(
    profile: CompatProfile,
    headers: HeaderMap,
    auth_state: DiscoveryAuthState,
) -> Response {
    if let Some(response) = require_auth(&headers) {
        return response;
    }

    if discovery_ownership_route(profile, &headers, DiscoveryShape::Libraries)
        == DiscoveryOwnershipRoute::NativeOwned
    {
        let context = match auth_state.resolve_query_context(&headers, None) {
            Some(context) => context,
            None => return StatusCode::UNAUTHORIZED.into_response(),
        };

        return native_owned_libraries_response(context).await;
    }

    let user =
        resolved_auth_user(&headers).expect("authorized libraries request should resolve user");

    if profile == CompatProfile::JavaLiveLocaldb {
        return match fetch_java_live_libraries(user).await {
            Ok(libraries) => Json(libraries).into_response(),
            Err(message) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": message })),
            )
                .into_response(),
        };
    }

    Json(snapshot_libraries_for_user(profile, user)).into_response()
}

async fn native_owned_libraries_response(context: DiscoveryQueryContext) -> Response {
    let mut adapter = SqliteDiscoveryAdapter::default();
    adapter.insert_library(LibraryRow::new("1", "default").with_root("/library1"));

    let queries = DiscoveryQueries::new(adapter);
    match queries
        .list_libraries(&to_domain_query_context(context.clone()), LibraryListQuery {})
        .await
    {
        Ok(libraries) => {
            let mut response = Json(libraries_payload(libraries, context.is_admin)).into_response();
            mark_native(&mut response);
            response
        }
        Err(error) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": discovery_error_message(error) })),
        )
            .into_response(),
    }
}

fn libraries_payload(libraries: Vec<LibraryReadModel>, is_admin: bool) -> Value {
    Value::Array(
        libraries
            .into_iter()
            .map(|library| {
                let root = if is_admin {
                    library.root
                } else {
                    String::new()
                };
                json!({
                    "id": library.id,
                    "name": library.name,
                    "root": root,
                    "importComicInfoBook": true,
                    "importComicInfoSeries": true,
                    "importComicInfoCollection": true,
                    "importComicInfoReadList": true,
                    "importComicInfoSeriesAppendVolume": true,
                    "importEpubBook": true,
                    "importEpubSeries": true,
                    "importMylarSeries": true,
                    "importLocalArtwork": true,
                    "importBarcodeIsbn": true,
                    "scanForceModifiedTime": false,
                    "scanInterval": "EVERY_6H",
                    "scanOnStartup": false,
                    "scanCbx": true,
                    "scanPdf": true,
                    "scanEpub": true,
                    "scanDirectoryExclusions": [],
                    "repairExtensions": false,
                    "convertToCbz": false,
                    "emptyTrashAfterScan": false,
                    "seriesCover": "FIRST",
                    "hashFiles": true,
                    "hashPages": false,
                    "hashKoreader": false,
                    "analyzeDimensions": true,
                    "oneshotsDirectory": Value::Null,
                    "unavailable": false,
                })
            })
            .collect(),
    )
}

fn to_domain_query_context(context: DiscoveryQueryContext) -> DomainDiscoveryQueryContext {
    DomainDiscoveryQueryContext {
        user_id: context.user_id,
        is_admin: context.is_admin,
        authorized_library_ids: context.authorized_library_ids,
        restrictions: context.restrictions.map(to_domain_restrictions),
    }
}

fn to_domain_restrictions(restrictions: QueryRestrictions) -> DomainQueryRestrictions {
    DomainQueryRestrictions {
        age: restrictions.age,
        age_restriction: restrictions
            .age_restriction
            .map(to_domain_age_restriction_kind),
        labels_allow: restrictions.labels_allow,
        labels_exclude: restrictions.labels_exclude,
    }
}

fn to_domain_age_restriction_kind(kind: AgeRestrictionKind) -> DomainAgeRestrictionKind {
    match kind {
        AgeRestrictionKind::AllowOnly => DomainAgeRestrictionKind::AllowOnly,
        AgeRestrictionKind::Exclude => DomainAgeRestrictionKind::Exclude,
    }
}

fn discovery_error_message(error: DiscoveryError) -> String {
    match error {
        DiscoveryError::NonNativeRequestShape(details) => {
            format!("native libraries query shape rejected: {details:?}")
        }
        DiscoveryError::InvalidRequest(message) => message,
        DiscoveryError::Persistence(message) => message,
    }
}

fn snapshot_libraries_for_user(profile: CompatProfile, user: PlaceholderUser) -> Value {
    let snapshot = if user_is_admin(user) {
        "libraries-list-admin.json"
    } else {
        "libraries-list-user.json"
    };

    let mut libraries = snapshot_json(snapshot, profile);

    if !user_shared_all_libraries(user) {
        let allowed_ids = user_shared_library_ids(user);
        if let Some(entries) = libraries.as_array_mut() {
            entries.retain(|library| {
                library
                    .get("id")
                    .and_then(|value| value.as_str())
                    .is_some_and(|id| allowed_ids.contains(&id))
            });
        }
    }

    libraries
}

async fn fetch_java_live_libraries(user: PlaceholderUser) -> Result<Value, String> {
    let base_url = std::env::var("KOMGA_RUST_JAVA_LIVE_BASE_URL")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| "http://127.0.0.1:8080".to_string());
    let bootstrap_url = format!("{}/api/v2/users/me", base_url.trim_end_matches('/'));
    let libraries_url = format!("{}/api/v1/libraries", base_url.trim_end_matches('/'));
    let client = reqwest::Client::builder()
        .build()
        .map_err(|error| format!("java live admin client build failed: {error}"))?;

    let bootstrap = client
        .get(bootstrap_url)
        .header(AUTHORIZATION, java_live_basic_auth_header(user))
        .header("X-Auth-Token", "")
        .send()
        .await
        .map_err(|error| format!("java live libraries bootstrap failed: {error}"))?;

    if !bootstrap.status().is_success() {
        return Err(format!(
            "java live libraries bootstrap returned HTTP {}",
            bootstrap.status().as_u16()
        ));
    }

    let bootstrap_headers = bootstrap.headers();
    let libraries_request = client.get(libraries_url);
    let libraries = match extract_java_live_session_cookie(bootstrap_headers) {
        Some(cookie) => libraries_request.header(COOKIE, cookie),
        None => {
            let token = extract_java_live_session_token(bootstrap_headers).ok_or_else(|| {
                "java live libraries bootstrap missing KOMGA-SESSION cookie and X-Auth-Token"
                    .to_string()
            })?;
            libraries_request.header("X-Auth-Token", token)
        }
    }
    .send()
    .await
    .map_err(|error| format!("java live libraries fetch failed: {error}"))?;

    if !libraries.status().is_success() {
        return Err(format!(
            "java live libraries returned HTTP {}",
            libraries.status().as_u16()
        ));
    }

    libraries
        .json::<Value>()
        .await
        .map_err(|error| format!("java live libraries JSON decode failed: {error}"))
}

fn java_live_basic_auth_header(user: PlaceholderUser) -> &'static str {
    if user_is_admin(user) {
        "Basic YWRtaW5AZXhhbXBsZS5vcmc6YWRtaW4="
    } else if user_shared_all_libraries(user) {
        "Basic dXNlckBleGFtcGxlLm9yZzp1c2Vy"
    } else {
        "Basic bGltaXRlZEBleGFtcGxlLm9yZzpsaW1pdGVk"
    }
}

fn extract_java_live_session_cookie(headers: &reqwest::header::HeaderMap) -> Option<String> {
    headers
        .get_all(header::SET_COOKIE)
        .iter()
        .find_map(|value| {
            value.to_str().ok().and_then(|cookie| {
                cookie
                    .split(';')
                    .map(str::trim)
                    .find(|part| part.starts_with("KOMGA-SESSION="))
                    .map(str::to_string)
            })
        })
}

fn extract_java_live_session_token(headers: &reqwest::header::HeaderMap) -> Option<String> {
    headers
        .get("X-Auth-Token")
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}
