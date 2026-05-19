use komga_application::identity_access::AuthUser;

pub(super) fn user_can_access_sync_book(
    user: &AuthUser,
    library_id: &str,
    age_rating: Option<u16>,
    sharing_labels: &[String],
) -> bool {
    user_can_access_library(user, library_id)
        && user_allows_content(user, age_rating, sharing_labels)
}

fn user_can_access_library(user: &AuthUser, library_id: &str) -> bool {
    user.shared_all_libraries
        || user.roles.iter().any(|role| role == "ADMIN")
        || user
            .shared_library_ids
            .iter()
            .any(|shared_library_id| shared_library_id == library_id)
}

fn user_allows_content(
    user: &AuthUser,
    age_rating: Option<u16>,
    sharing_labels: &[String],
) -> bool {
    if user.age_restriction.is_none()
        && user.labels_allow.is_empty()
        && user.labels_exclude.is_empty()
    {
        return true;
    }

    let labels = sharing_labels
        .iter()
        .map(|value| value.trim().to_ascii_lowercase())
        .filter(|value| !value.is_empty())
        .collect::<Vec<_>>();

    let age_allowed = user.age_restriction.as_ref().and_then(|restriction| {
        restriction
            .restriction
            .eq_ignore_ascii_case("ALLOW_ONLY")
            .then(|| age_rating.is_some_and(|age| age <= restriction.age as u16))
    });
    let label_allowed = if user.labels_allow.is_empty() {
        None
    } else {
        Some(
            user.labels_allow
                .iter()
                .map(|value| value.trim().to_ascii_lowercase())
                .filter(|value| !value.is_empty())
                .any(|candidate| labels.contains(&candidate)),
        )
    };

    let allowed = match (age_allowed, label_allowed) {
        (None, label_allowed) => label_allowed != Some(false),
        (age_allowed, None) => age_allowed != Some(false),
        (age_allowed, label_allowed) => age_allowed != Some(false) || label_allowed != Some(false),
    };
    if !allowed {
        return false;
    }

    let age_denied = user.age_restriction.as_ref().is_some_and(|restriction| {
        restriction.restriction.eq_ignore_ascii_case("EXCLUDE")
            && age_rating.is_some_and(|age| age >= restriction.age as u16)
    });
    let label_denied = if user.labels_exclude.is_empty() {
        false
    } else {
        user.labels_exclude
            .iter()
            .map(|value| value.trim().to_ascii_lowercase())
            .filter(|value| !value.is_empty())
            .any(|candidate| labels.contains(&candidate))
    };

    !age_denied && !label_denied
}

pub(super) fn normalized_sharing_labels(labels: &str) -> Vec<String> {
    labels
        .split(',')
        .map(str::trim)
        .filter(|label| !label.is_empty())
        .map(|label| label.to_ascii_lowercase())
        .collect()
}

#[cfg(test)]
mod tests {
    use komga_application::identity_access::AuthUserAgeRestriction;

    use super::*;

    fn user_with_all_access() -> AuthUser {
        AuthUser {
            id: "user-1".to_string(),
            email: "test@example.com".to_string(),
            password: String::new(),
            roles: vec!["USER".to_string()],
            shared_all_libraries: true,
            shared_library_ids: vec![],
            age_restriction: None,
            labels_allow: vec![],
            labels_exclude: vec![],
        }
    }

    #[test]
    fn unrestricted_user_can_access_any_book() {
        let user = user_with_all_access();
        assert!(user_can_access_sync_book(&user, "lib-1", Some(18), &[]));
    }

    #[test]
    fn user_without_library_access_is_denied() {
        let mut user = user_with_all_access();
        user.shared_all_libraries = false;
        user.shared_library_ids = vec!["lib-other".to_string()];
        assert!(!user_can_access_sync_book(&user, "lib-1", None, &[]));
    }

    #[test]
    fn admin_bypasses_library_restriction() {
        let mut user = user_with_all_access();
        user.shared_all_libraries = false;
        user.roles = vec!["ADMIN".to_string()];
        assert!(user_can_access_sync_book(&user, "lib-1", None, &[]));
    }

    #[test]
    fn age_allow_only_blocks_higher_rating() {
        let mut user = user_with_all_access();
        user.age_restriction = Some(AuthUserAgeRestriction {
            age: 12,
            restriction: "ALLOW_ONLY".to_string(),
        });
        assert!(!user_can_access_sync_book(&user, "lib-1", Some(16), &[]));
        assert!(user_can_access_sync_book(&user, "lib-1", Some(10), &[]));
    }

    #[test]
    fn age_exclude_blocks_at_threshold() {
        let mut user = user_with_all_access();
        user.age_restriction = Some(AuthUserAgeRestriction {
            age: 18,
            restriction: "EXCLUDE".to_string(),
        });
        assert!(!user_can_access_sync_book(&user, "lib-1", Some(18), &[]));
        assert!(user_can_access_sync_book(&user, "lib-1", Some(16), &[]));
    }

    #[test]
    fn label_allow_filters_correctly() {
        let mut user = user_with_all_access();
        user.labels_allow = vec!["kids".to_string()];
        let labels = vec!["kids".to_string()];
        assert!(user_can_access_sync_book(&user, "lib-1", None, &labels));
        let wrong_labels = vec!["adult".to_string()];
        assert!(!user_can_access_sync_book(
            &user,
            "lib-1",
            None,
            &wrong_labels
        ));
    }

    #[test]
    fn label_exclude_filters_correctly() {
        let mut user = user_with_all_access();
        user.labels_exclude = vec!["nsfw".to_string()];
        let labels = vec!["nsfw".to_string()];
        assert!(!user_can_access_sync_book(&user, "lib-1", None, &labels));
        assert!(user_can_access_sync_book(&user, "lib-1", None, &[]));
    }

    #[test]
    fn normalized_sharing_labels_splits_and_lowercases() {
        let result = normalized_sharing_labels("Kids, ADULT , ,Fantasy");
        assert_eq!(result, vec!["kids", "adult", "fantasy"]);
    }
}
