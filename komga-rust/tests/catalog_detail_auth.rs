use axum::http::{HeaderMap, HeaderValue};

#[test]
fn detail_routes_require_registered_session_principals() {
    let auth_state = komga_rust::app::discovery_auth::DiscoveryAuthState::default();
    let mut headers = HeaderMap::new();
    headers.insert(
        "X-Auth-Token",
        HeaderValue::from_static("unregistered-token"),
    );

    let detail = komga_rust::app::discovery_auth::DetailResourceContext {
        library_id: Some("1".to_string()),
        content: Some(komga_rust::app::discovery_auth::DetailContentContext {
            age_rating: Some(12),
            sharing_labels: vec!["safe".to_string()],
        }),
    };

    assert_eq!(
        auth_state.resolve_detail_query_context(&headers, &detail),
        Err(komga_rust::app::discovery_auth::DetailAccessDenial::Unauthorized),
    );

    let payload = serde_json::json!({
        "id": "1PXGX4XP02A26",
        "roles": ["FILE_DOWNLOAD", "PAGE_STREAMING", "USER"],
        "sharedAllLibraries": false,
        "sharedLibrariesIds": ["1"],
        "labelsAllow": [],
        "labelsExclude": [],
        "ageRestriction": null
    });
    let principal = komga_rust::app::discovery_auth::principal_from_user_payload(&payload)
        .expect("user payload should map to principal");
    auth_state.register_session_principal("registered-token", principal);

    headers.insert("X-Auth-Token", HeaderValue::from_static("registered-token"));
    let context = auth_state
        .resolve_detail_query_context(&headers, &detail)
        .expect("registered session principal should authorize detail route");

    assert_eq!(context.user_id.as_deref(), Some("1PXGX4XP02A26"));
    assert_eq!(context.authorized_library_ids, Some(vec!["1".to_string()]));
}

#[test]
fn detail_query_context_matches_authorized_library_scope() {
    let auth_state = komga_rust::app::discovery_auth::DiscoveryAuthState::default();
    let payload = serde_json::json!({
        "id": "limited-user",
        "roles": ["USER"],
        "sharedAllLibraries": false,
        "sharedLibrariesIds": ["1", "2"],
        "labelsAllow": ["safe"],
        "labelsExclude": ["nsfw"],
        "ageRestriction": {"age": 16, "restriction": "EXCLUDE"}
    });
    let principal = komga_rust::app::discovery_auth::principal_from_user_payload(&payload)
        .expect("user payload should map to principal");
    auth_state.register_session_principal("limited-token", principal);

    let mut headers = HeaderMap::new();
    headers.insert("X-Auth-Token", HeaderValue::from_static("limited-token"));

    let detail = komga_rust::app::discovery_auth::DetailResourceContext {
        library_id: Some("2".to_string()),
        content: Some(komga_rust::app::discovery_auth::DetailContentContext {
            age_rating: Some(12),
            sharing_labels: vec!["SAFE".to_string()],
        }),
    };

    let context = auth_state
        .resolve_detail_query_context(&headers, &detail)
        .expect("authorized detail route should resolve query context");

    assert_eq!(context.user_id.as_deref(), Some("limited-user"));
    assert_eq!(context.authorized_library_ids, Some(vec!["2".to_string()]));

    let restrictions = context
        .restrictions
        .expect("principal restrictions should be projected for detail queries");
    assert_eq!(restrictions.age, Some(16));
    assert_eq!(
        restrictions.age_restriction,
        Some(komga_rust::app::discovery_auth::AgeRestrictionKind::Exclude)
    );
    assert_eq!(restrictions.labels_allow, vec!["safe".to_string()]);
    assert_eq!(restrictions.labels_exclude, vec!["nsfw".to_string()]);
}

#[test]
fn detail_denial_semantics_match_java_truth() {
    let auth_state = komga_rust::app::discovery_auth::DiscoveryAuthState::default();
    let payload = serde_json::json!({
        "id": "restricted-limited-user",
        "roles": ["USER"],
        "sharedAllLibraries": false,
        "sharedLibrariesIds": ["1"],
        "labelsAllow": ["safe"],
        "labelsExclude": ["adult"],
        "ageRestriction": {"age": 16, "restriction": "ALLOW_ONLY"}
    });
    let principal = komga_rust::app::discovery_auth::principal_from_user_payload(&payload)
        .expect("user payload should map to principal");
    auth_state.register_session_principal("restricted-token", principal);

    let mut headers = HeaderMap::new();
    headers.insert("X-Auth-Token", HeaderValue::from_static("restricted-token"));

    let missing_library_detail = komga_rust::app::discovery_auth::DetailResourceContext {
        library_id: None,
        content: Some(komga_rust::app::discovery_auth::DetailContentContext {
            age_rating: Some(12),
            sharing_labels: vec!["safe".to_string()],
        }),
    };
    assert_eq!(
        auth_state.resolve_detail_query_context(&headers, &missing_library_detail),
        Err(komga_rust::app::discovery_auth::DetailAccessDenial::NotFound),
        "limited users should get NOT_FOUND when target library is unresolved",
    );

    let forbidden_library_detail = komga_rust::app::discovery_auth::DetailResourceContext {
        library_id: Some("2".to_string()),
        content: Some(komga_rust::app::discovery_auth::DetailContentContext {
            age_rating: Some(12),
            sharing_labels: vec!["safe".to_string()],
        }),
    };
    assert_eq!(
        auth_state.resolve_detail_query_context(&headers, &forbidden_library_detail),
        Err(komga_rust::app::discovery_auth::DetailAccessDenial::Forbidden),
        "limited users should get FORBIDDEN for out-of-scope libraries",
    );

    let missing_restriction_context = komga_rust::app::discovery_auth::DetailResourceContext {
        library_id: Some("1".to_string()),
        content: None,
    };
    assert_eq!(
        auth_state.resolve_detail_query_context(&headers, &missing_restriction_context),
        Err(komga_rust::app::discovery_auth::DetailAccessDenial::NotFound),
        "restricted users should get NOT_FOUND when restriction context is unresolved",
    );

    let forbidden_restriction_detail = komga_rust::app::discovery_auth::DetailResourceContext {
        library_id: Some("1".to_string()),
        content: Some(komga_rust::app::discovery_auth::DetailContentContext {
            age_rating: Some(18),
            sharing_labels: vec!["adult".to_string()],
        }),
    };
    assert_eq!(
        auth_state.resolve_detail_query_context(&headers, &forbidden_restriction_detail),
        Err(komga_rust::app::discovery_auth::DetailAccessDenial::Forbidden),
        "restricted users should get FORBIDDEN for disallowed content",
    );

    let allowed_detail = komga_rust::app::discovery_auth::DetailResourceContext {
        library_id: Some("1".to_string()),
        content: Some(komga_rust::app::discovery_auth::DetailContentContext {
            age_rating: Some(12),
            sharing_labels: vec!["safe".to_string()],
        }),
    };
    let context = auth_state
        .resolve_detail_query_context(&headers, &allowed_detail)
        .expect("allowed detail should resolve query context");
    assert_eq!(context.authorized_library_ids, Some(vec!["1".to_string()]));
    assert!(context.restrictions.is_some());
}
