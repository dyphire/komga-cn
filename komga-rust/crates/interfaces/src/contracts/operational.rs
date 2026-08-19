use serde::Serialize;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingMultiSourceDto<T> {
    pub configuration_source: Option<T>,
    pub database_source: Option<T>,
    pub effective_value: Option<T>,
}

impl<T> SettingMultiSourceDto<T> {
    pub fn new(
        configuration_source: Option<T>,
        database_source: Option<T>,
        effective_value: Option<T>,
    ) -> Self {
        Self {
            configuration_source,
            database_source,
            effective_value,
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingsDto {
    pub delete_empty_collections: bool,
    pub delete_empty_read_lists: bool,
    pub remember_me_duration_days: u64,
    pub thumbnail_size: ThumbnailSizeDto,
    pub task_pool_size: u64,
    pub server_port: SettingMultiSourceDto<u16>,
    pub server_context_path: SettingMultiSourceDto<String>,
    pub kobo_proxy: bool,
    pub kobo_port: Option<u16>,
    pub kepubify_path: SettingMultiSourceDto<String>,
}

#[derive(Debug, Serialize)]
pub enum ThumbnailSizeDto {
    #[serde(rename = "DEFAULT")]
    Default,
    #[serde(rename = "MEDIUM")]
    Medium,
    #[serde(rename = "LARGE")]
    Large,
    #[serde(rename = "XLARGE")]
    XLarge,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OAuth2ClientDto {
    pub name: String,
    pub registration_id: String,
}
