use std::collections::BTreeMap;

use serde::Serialize;

use super::common::KotlinLocalDateTime;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryEventDto {
    pub id: String,
    #[serde(rename = "type")]
    pub event_type: String,
    pub timestamp: KotlinLocalDateTime,
    pub book_id: Option<String>,
    pub series_id: Option<String>,
    pub properties: BTreeMap<String, String>,
}
