pub fn configured_api_key() -> Option<String> {
    std::env::var("KOMGA_COMPAT_API_KEY")
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}
