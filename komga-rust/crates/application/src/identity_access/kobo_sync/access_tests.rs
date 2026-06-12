use super::KoboSyncAccessPolicy;
use crate::identity_access::{
    AuthUser, AuthUserAgeRestriction, AuthUserAgeRestrictionKind, AuthUserRole,
};

#[test]
fn sync_access_denies_books_outside_shared_libraries() {
    let mut user = unrestricted_user();
    user.shared_all_libraries = false;
    user.shared_library_ids = vec!["lib-b".to_string()];
    let policy = KoboSyncAccessPolicy::new(&user);

    assert!(!policy.can_access_book("lib-a", None, &[]));
}

#[test]
fn sync_access_allows_admin_outside_shared_libraries() {
    let mut user = unrestricted_user();
    user.shared_all_libraries = false;
    user.shared_library_ids = Vec::new();
    user.roles = vec![AuthUserRole::Admin];
    let policy = KoboSyncAccessPolicy::new(&user);

    assert!(policy.can_access_book("lib-a", None, &[]));
}

#[test]
fn sync_access_uses_allow_age_or_allow_label_rules() {
    let mut user = unrestricted_user();
    user.age_restriction = Some(AuthUserAgeRestriction {
        age: 12,
        restriction: AuthUserAgeRestrictionKind::AllowOnly,
    });
    user.labels_allow = vec!["kids".to_string()];
    let policy = KoboSyncAccessPolicy::new(&user);

    assert!(!policy.can_access_book("lib-a", Some(16), &[]));
    assert!(policy.can_access_book("lib-a", Some(16), &["kids".to_string()]));
    assert!(policy.can_access_book("lib-a", Some(10), &[]));
}

#[test]
fn sync_access_applies_exclude_age_rule() {
    let mut user = unrestricted_user();
    user.age_restriction = Some(AuthUserAgeRestriction {
        age: 18,
        restriction: AuthUserAgeRestrictionKind::Exclude,
    });
    let policy = KoboSyncAccessPolicy::new(&user);

    assert!(!policy.can_access_book("lib-a", Some(18), &[]));
    assert!(policy.can_access_book("lib-a", Some(16), &[]));
}

#[test]
fn sync_access_applies_exclude_labels() {
    let mut user = unrestricted_user();
    user.labels_exclude = vec!["adult".to_string()];
    let policy = KoboSyncAccessPolicy::new(&user);

    assert!(!policy.can_access_book("lib-a", None, &["Adult".to_string()]));
    assert!(policy.can_access_book("lib-a", None, &["kids".to_string()]));
}

fn unrestricted_user() -> AuthUser {
    AuthUser {
        id: "user-1".to_string(),
        email: "user@example.org".to_string(),
        password: String::new(),
        roles: Vec::new(),
        shared_all_libraries: true,
        shared_library_ids: Vec::new(),
        age_restriction: None,
        labels_allow: Vec::new(),
        labels_exclude: Vec::new(),
    }
}
