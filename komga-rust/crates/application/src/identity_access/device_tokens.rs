use base64::{Engine as _, engine::general_purpose::STANDARD_NO_PAD};
use sha2::{Digest, Sha256};

use crate::random_tokens;

pub fn sanitize_identifier(value: &str) -> String {
    value
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() {
                ch.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect()
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GeneratedKoboDeviceTokens {
    pub access_token: String,
    pub refresh_token: String,
    pub tracking_id: String,
}

pub fn generate_kobo_device_tokens() -> GeneratedKoboDeviceTokens {
    GeneratedKoboDeviceTokens {
        access_token: random_tokens::random_alphanumeric(24),
        refresh_token: random_tokens::random_alphanumeric(24),
        tracking_id: random_uuid_like(),
    }
}

pub fn generated_kobo_api_token(auth_token: &str, authenticated_user_id: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(auth_token.trim().as_bytes());
    hasher.update(b":");
    hasher.update(authenticated_user_id.trim().as_bytes());
    let digest = hasher.finalize();
    format!("KOMGA.{}", STANDARD_NO_PAD.encode(digest))
}

pub fn random_uuid_like() -> String {
    random_tokens::random_uuid_like()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize_identifier_normalizes_and_replaces_non_alnum() {
        assert_eq!(sanitize_identifier("Ab C_1?"), "ab-c-1-");
    }

    #[test]
    fn generated_kobo_api_token_is_non_hardcoded_and_identity_scoped() {
        let token = generated_kobo_api_token("auth-token-a", "user-a");
        assert_ne!(token, "e30=");
        assert!(token.starts_with("KOMGA."));

        let changed_auth_token = generated_kobo_api_token("auth-token-b", "user-a");
        let changed_user_token = generated_kobo_api_token("auth-token-a", "user-b");
        assert_ne!(token, changed_auth_token);
        assert_ne!(token, changed_user_token);
    }
}
