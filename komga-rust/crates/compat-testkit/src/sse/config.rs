use crate::cases::SetupStep;
use anyhow::Context;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Deserialize)]
pub struct SseHarnessConfig {
    pub output_dir: String,
    pub cases: Vec<SseCaseConfig>,
}

#[derive(Debug, Deserialize)]
pub struct SseCaseConfig {
    pub id: String,
    pub path: String,
    #[serde(default)]
    pub audience: SseAudience,
    pub headers: Option<BTreeMap<String, String>>,
    #[allow(dead_code)]
    pub setup: Option<Vec<SetupStep>>,
    #[serde(default = "default_ignore_heartbeats")]
    pub ignore_heartbeats: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum SseAudience {
    #[default]
    Any,
    Admin,
    User,
}

impl SseHarnessConfig {
    pub fn load_default() -> anyhow::Result<Self> {
        Self::load_from(
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/compat/sse-cases.toml"),
        )
    }

    pub fn load_from(path: impl Into<PathBuf>) -> anyhow::Result<Self> {
        let path = path.into();
        let content = fs::read_to_string(&path)
            .with_context(|| format!("failed to read SSE compat cases from {}", path.display()))?;
        toml::from_str(&content)
            .with_context(|| format!("failed to parse SSE compat cases from {}", path.display()))
    }
}

impl SseCaseConfig {
    pub fn header_allowlist(&self) -> BTreeSet<String> {
        let mut allowlist = BTreeSet::new();
        allowlist.insert("content-type".to_string());

        if let Some(headers) = &self.headers {
            for header in headers.keys() {
                allowlist.insert(header.to_ascii_lowercase());
            }
        }

        allowlist
    }
}

fn default_ignore_heartbeats() -> bool {
    true
}
