use komga_application::identity_access::{AuthUser, user_response_role_names};
use serde_json::{Value, json};

pub(crate) fn user_payload_json(user: &AuthUser) -> Value {
    let mut payload = json!({
        "id": user.id,
        "email": user.email,
        "roles": user_response_role_names(user),
        "sharedAllLibraries": user.shared_all_libraries,
        "sharedLibrariesIds": user.shared_library_ids,
        "labelsAllow": user.labels_allow,
        "labelsExclude": user.labels_exclude,
    });
    if let Some(age_restriction) = &user.age_restriction {
        payload["ageRestriction"] = json!({
            "age": age_restriction.age,
            "restriction": age_restriction.restriction.persisted_name(),
        });
    }
    payload
}
