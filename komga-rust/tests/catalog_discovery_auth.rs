use axum::http::{HeaderMap, HeaderValue};

#[test]
fn maps_principal_to_authorized_library_scope() {
    let principal = komga_rust::app::discovery_auth::DiscoveryPrincipal {
        user_id: "limited-user".to_string(),
        roles: vec!["USER".to_string()],
        shared_all_libraries: false,
        shared_library_ids: vec!["1".to_string(), "2".to_string()],
        restrictions: komga_rust::app::discovery_auth::ContentRestrictions::default(),
    };

    let requested = vec!["2".to_string(), "3".to_string()];
    let context = komga_rust::app::discovery_auth::to_query_context(&principal, Some(&requested));

    assert_eq!(context.user_id.as_deref(), Some("limited-user"));
    assert!(!context.is_admin);
    assert_eq!(context.authorized_library_ids, Some(vec!["2".to_string()]));
}

#[test]
fn maps_restrictions_to_query_context() {
    let principal = komga_rust::app::discovery_auth::DiscoveryPrincipal {
        user_id: "restricted-user".to_string(),
        roles: vec!["USER".to_string(), "PAGE_STREAMING".to_string()],
        shared_all_libraries: true,
        shared_library_ids: vec![],
        restrictions: komga_rust::app::discovery_auth::ContentRestrictions {
            age: Some(16),
            age_restriction: Some(komga_rust::app::discovery_auth::AgeRestrictionKind::Exclude),
            labels_allow: vec!["safe".to_string()],
            labels_exclude: vec!["nsfw".to_string()],
        },
    };

    let context = komga_rust::app::discovery_auth::to_query_context(&principal, None);
    let restrictions = context
        .restrictions
        .expect("restrictions should be projected");

    assert_eq!(context.user_id.as_deref(), Some("restricted-user"));
    assert_eq!(restrictions.age, Some(16));
    assert_eq!(
        restrictions.age_restriction,
        Some(komga_rust::app::discovery_auth::AgeRestrictionKind::Exclude)
    );
    assert_eq!(restrictions.labels_allow, vec!["safe".to_string()]);
    assert_eq!(restrictions.labels_exclude, vec!["nsfw".to_string()]);
}

#[test]
fn native_owned_requests_do_not_use_placeholder_users() {
    let auth_state = komga_rust::app::discovery_auth::DiscoveryAuthState::default();
    let mut unknown_headers = HeaderMap::new();
    unknown_headers.insert(
        "X-Auth-Token",
        HeaderValue::from_static("some-random-token"),
    );

    assert!(
        auth_state
            .resolve_query_context(&unknown_headers, None)
            .is_none(),
        "unregistered tokens must not resolve to any principal context"
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
    auth_state.register_session_principal("komga-limited-token", principal);

    let mut known_headers = HeaderMap::new();
    known_headers.insert(
        "X-Auth-Token",
        HeaderValue::from_static("komga-limited-token"),
    );
    let context = auth_state
        .resolve_query_context(&known_headers, None)
        .expect("registered token should resolve to query context");

    assert_eq!(context.user_id.as_deref(), Some("1PXGX4XP02A26"));
    assert_eq!(context.authorized_library_ids, Some(vec!["1".to_string()]));
}
