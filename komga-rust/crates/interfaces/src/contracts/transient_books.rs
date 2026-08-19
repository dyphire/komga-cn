use serde::Serialize;

use super::common::KotlinLocalDateTime;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TransientBookDto {
    pub id: String,
    pub name: String,
    pub url: String,
    pub file_last_modified: KotlinLocalDateTime,
    pub size_bytes: u64,
    pub size: String,
    pub status: String,
    pub media_type: String,
    pub pages: Vec<TransientBookPageDto>,
    pub files: Vec<String>,
    pub comment: String,
    pub number: Option<f64>,
    pub series_id: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TransientBookPageDto {
    pub number: u32,
    pub file_name: String,
    pub media_type: String,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub size_bytes: Option<u64>,
    pub size: String,
}

#[derive(Debug, Serialize)]
pub struct TransientBookErrorDto {
    pub error: String,
}
