use crate::ComparisonMode;
use anyhow::{Context, bail};
use serde::Deserialize;
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Deserialize)]
pub struct HarnessConfig {
    pub output_dir: String,
    pub header_allowlist: Vec<String>,
    pub cases: Vec<CaseConfig>,
}

#[derive(Debug, Deserialize)]
pub struct CaseConfig {
    pub id: String,
    pub method: String,
    pub path: String,
    pub body: Option<String>,
    #[serde(default = "default_comparison_mode")]
    pub comparison: ComparisonMode,
    pub headers: Option<BTreeMap<String, String>>,
    #[allow(dead_code)]
    pub setup: Option<Vec<SetupStep>>,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
pub struct SetupStep {
    pub name: String,
    pub method: String,
    pub path: String,
    pub headers: Option<BTreeMap<String, String>>,
    pub extract_headers: Option<BTreeMap<String, String>>,
}

impl HarnessConfig {
    pub fn load_default() -> anyhow::Result<Self> {
        bail!("default contract cases were removed; use HarnessConfig::load_from(path)")
    }

    pub fn load_from(path: impl Into<PathBuf>) -> anyhow::Result<Self> {
        let path = path.into();
        let content = fs::read_to_string(&path)
            .with_context(|| format!("failed to read contract cases from {}", path.display()))?;
        toml::from_str(&content)
            .with_context(|| format!("failed to parse contract cases from {}", path.display()))
    }
}

impl CaseConfig {
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

fn default_comparison_mode() -> ComparisonMode {
    ComparisonMode::Json
}
