use super::*;

pub(super) async fn kobo_path_api_key_id(
    auth_token: &str,
    database_file: &FsPath,
) -> Option<String> {
    kobo_path_api_key_metadata(auth_token, database_file)
        .await
        .map(|(id, _)| id)
}

pub(super) async fn kobo_path_api_key_metadata(
    auth_token: &str,
    database_file: &FsPath,
) -> Option<(String, String)> {
    if !valid_kobo_path_token(auth_token) {
        return None;
    }

    let mut metadata_headers = HeaderMap::new();
    metadata_headers.insert("x-api-key", HeaderValue::from_str(auth_token).ok()?);
    persisted_api_key_metadata(&metadata_headers, database_file)
        .await
        .map(|metadata| (metadata.id, metadata.comment))
}

pub(super) fn random_uuid_like() -> String {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let hex = format!("{nanos:032x}");
    format!(
        "{}-{}-{}-{}-{}",
        &hex[0..8],
        &hex[8..12],
        &hex[12..16],
        &hex[16..20],
        &hex[20..32],
    )
}
