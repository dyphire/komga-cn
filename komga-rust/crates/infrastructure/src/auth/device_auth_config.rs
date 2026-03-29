pub fn configured_api_key() -> Option<String> {
    std::env::var("KOMGA_COMPAT_API_KEY")
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

pub fn configured_api_key_id() -> Option<String> {
    std::env::var("KOMGA_COMPAT_API_KEY_ID")
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

pub fn configured_api_key_comment() -> Option<String> {
    std::env::var("KOMGA_COMPAT_API_KEY_COMMENT")
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}
