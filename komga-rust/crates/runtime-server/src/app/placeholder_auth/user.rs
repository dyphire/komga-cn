use axum::http::{HeaderMap, header};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use serde_json::{Value, json};

#[derive(Clone, Copy)]
pub(in crate::app) struct PlaceholderUser {
    id: &'static str,
    email: &'static str,
    password: &'static str,
    shared_all_libraries: bool,
    shared_library_ids: &'static [&'static str],
}

impl PlaceholderUser {
    pub(super) const fn id(self) -> &'static str {
        self.id
    }
}

const PLACEHOLDER_USERS: &[PlaceholderUser] = &[
    PlaceholderUser {
        id: "admin",
        email: "admin@example.org",
        password: "admin",
        shared_all_libraries: true,
        shared_library_ids: &[],
    },
    PlaceholderUser {
        id: "user",
        email: "user@example.org",
        password: "user",
        shared_all_libraries: true,
        shared_library_ids: &[],
    },
    PlaceholderUser {
        id: "limited",
        email: "limited@example.org",
        password: "limited",
        shared_all_libraries: false,
        shared_library_ids: &["1"],
    },
    PlaceholderUser {
        id: "restricted",
        email: "restricted@example.org",
        password: "restricted",
        shared_all_libraries: true,
        shared_library_ids: &[],
    },
];

pub(in crate::app) enum AuthOutcome {
    Valid(PlaceholderUser),
    Invalid,
    Missing,
}

pub(super) fn placeholder_users() -> &'static [PlaceholderUser] {
    PLACEHOLDER_USERS
}

pub(super) fn default_placeholder_user() -> PlaceholderUser {
    PLACEHOLDER_USERS[0]
}

pub(in crate::app) fn basic_user(headers: &HeaderMap) -> AuthOutcome {
    let Some(value) = headers
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
    else {
        return AuthOutcome::Missing;
    };

    let value = value.trim();
    if value.is_empty() {
        return AuthOutcome::Missing;
    }

    let Some(encoded) = value.strip_prefix("Basic ") else {
        return AuthOutcome::Invalid;
    };

    let decoded = match STANDARD.decode(encoded) {
        Ok(decoded) => decoded,
        Err(_) => return AuthOutcome::Invalid,
    };

    let credentials = match String::from_utf8(decoded) {
        Ok(credentials) => credentials,
        Err(_) => return AuthOutcome::Invalid,
    };

    let Some((username, password)) = credentials.split_once(':') else {
        return AuthOutcome::Invalid;
    };

    PLACEHOLDER_USERS
        .iter()
        .copied()
        .find(|user| user.email == username && user.password == password)
        .map(AuthOutcome::Valid)
        .unwrap_or(AuthOutcome::Invalid)
}

pub(in crate::app) fn api_key_user(headers: &HeaderMap) -> AuthOutcome {
    let Some(value) = headers
        .get("x-api-key")
        .and_then(|value| value.to_str().ok())
    else {
        return AuthOutcome::Missing;
    };

    let value = value.trim();
    if value.is_empty() {
        return AuthOutcome::Invalid;
    }

    if value == configured_api_key().as_str() {
        AuthOutcome::Valid(PLACEHOLDER_USERS[1])
    } else {
        AuthOutcome::Invalid
    }
}

pub(in crate::app) fn user_is_admin(user: PlaceholderUser) -> bool {
    user.id == "admin"
}

pub(in crate::app) fn user_shared_all_libraries(user: PlaceholderUser) -> bool {
    user.shared_all_libraries
}

pub(in crate::app) fn user_shared_library_ids(user: PlaceholderUser) -> &'static [&'static str] {
    user.shared_library_ids
}

pub(in crate::app) fn placeholder_user_json(user: PlaceholderUser) -> Value {
    if user.email == "admin@example.org" {
        json!({
            "id": user.id,
            "email": user.email,
            "roles": ["ADMIN", "FILE_DOWNLOAD", "PAGE_STREAMING", "USER"],
            "sharedAllLibraries": user.shared_all_libraries,
            "sharedLibrariesIds": user.shared_library_ids,
            "labelsAllow": [],
            "labelsExclude": [],
            "ageRestriction": null,
        })
    } else if user.email == "user@example.org" || user.email == "limited@example.org" {
        json!({
            "id": if user.email == "user@example.org" { "0PV32486S7X3J" } else { "1PXGX4XP02A26" },
            "email": user.email,
            "roles": ["FILE_DOWNLOAD", "PAGE_STREAMING", "USER"],
            "sharedAllLibraries": user.shared_all_libraries,
            "sharedLibrariesIds": user.shared_library_ids,
            "labelsAllow": [],
            "labelsExclude": [],
            "ageRestriction": null,
        })
    } else if user.email == "restricted@example.org" {
        json!({
            "id": "2R3STR1CT3D",
            "email": user.email,
            "roles": ["FILE_DOWNLOAD", "PAGE_STREAMING", "USER"],
            "sharedAllLibraries": true,
            "sharedLibrariesIds": [],
            "labelsAllow": [],
            "labelsExclude": ["adult"],
            "ageRestriction": null,
        })
    } else {
        json!({
            "id": user.id,
            "email": user.email,
        })
    }
}

fn configured_api_key() -> String {
    std::env::var("KOMGA_COMPAT_API_KEY").unwrap_or_else(|_| "compat-api-key".to_string())
}
