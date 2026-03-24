pub mod cases;
pub mod contract_matrix;
pub mod diff_writer;
pub mod normalize;
pub mod runtime;
pub mod sse;

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NormalizedBody {
    Json(serde_json::Value),
    Text(String),
    Empty,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ComparisonMode {
    Json,
    Xml,
    BinaryMetadata,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NormalizedResponse {
    pub status: u16,
    pub headers: BTreeMap<String, Vec<String>>,
    pub body: NormalizedBody,
}

#[derive(Debug, Clone, Serialize)]
pub struct DiffReport {
    pub case_id: String,
    pub matches: bool,
    pub differences: Vec<String>,
    pub java: SerializableResponse,
    pub rust: SerializableResponse,
}

#[derive(Debug, Clone, Serialize)]
pub struct SerializableResponse {
    pub status: u16,
    pub headers: BTreeMap<String, Vec<String>>,
    pub body: serde_json::Value,
}

impl From<&NormalizedResponse> for SerializableResponse {
    fn from(value: &NormalizedResponse) -> Self {
        let body = match &value.body {
            NormalizedBody::Json(json) => json.clone(),
            NormalizedBody::Text(text) => serde_json::Value::String(text.clone()),
            NormalizedBody::Empty => serde_json::Value::Null,
        };

        Self {
            status: value.status,
            headers: value.headers.clone(),
            body,
        }
    }
}
