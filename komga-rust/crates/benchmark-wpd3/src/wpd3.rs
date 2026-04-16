use std::env;
use std::error::Error as StdError;
use std::fmt;

const DEFAULT_MANIFEST: &str = include_str!("../../../benchmark/wpd3.scenarios.toml");

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Manifest {
    pub suite: String,
    pub baseline: String,
    pub scenarios: Vec<Scenario>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Scenario {
    pub name: String,
    pub java_benchmark: String,
    pub java_case: String,
    pub method: String,
    pub path: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeConfig {
    pub base_url: String,
    pub auth: AuthMode,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuthMode {
    Anonymous,
    Cookie(String),
    Basic { username: String, password: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Wpd3Error {
    InvalidManifest(String),
    InvalidEnvironment(String),
}

impl fmt::Display for Wpd3Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidManifest(message) => write!(f, "invalid benchmark manifest: {message}"),
            Self::InvalidEnvironment(message) => {
                write!(f, "invalid benchmark environment: {message}")
            }
        }
    }
}

impl StdError for Wpd3Error {}

pub fn run_from_env() -> Result<String, Wpd3Error> {
    let runtime = RuntimeConfig::from_env()?;
    let manifest = Manifest::parse(DEFAULT_MANIFEST)?;
    Ok(render_plan(&manifest, &runtime))
}

pub fn render_plan(manifest: &Manifest, runtime: &RuntimeConfig) -> String {
    let mut lines = vec![
        format!("suite: {}", manifest.suite),
        format!("baseline: {}", manifest.baseline),
        format!("base_url: {}", runtime.base_url),
        format!("auth: {}", runtime.auth_label()),
        String::from("scenarios:"),
    ];

    for scenario in &manifest.scenarios {
        lines.push(format!(
            "  - {name} | {benchmark}::{case} | {method} {path}",
            name = scenario.name,
            benchmark = scenario.java_benchmark,
            case = scenario.java_case,
            method = scenario.method,
            path = scenario.path,
        ));
    }

    lines.join("\n")
}

impl Manifest {
    pub fn parse(input: &str) -> Result<Self, Wpd3Error> {
        let mut suite = None;
        let mut baseline = None;
        let mut scenarios = Vec::new();
        let mut current: Option<ScenarioBuilder> = None;

        for raw_line in input.lines() {
            let line = raw_line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }

            if line == "[[scenario]]" {
                if let Some(builder) = current.take() {
                    scenarios.push(builder.build()?);
                }
                current = Some(ScenarioBuilder::default());
                continue;
            }

            let (key, value) = parse_assignment(line)?;
            let target = current.as_mut();

            match (target, key) {
                (None, "suite") => suite = Some(value),
                (None, "baseline") => baseline = Some(value),
                (Some(builder), "name") => builder.name = Some(value),
                (Some(builder), "java_benchmark") => builder.java_benchmark = Some(value),
                (Some(builder), "java_case") => builder.java_case = Some(value),
                (Some(builder), "method") => builder.method = Some(value),
                (Some(builder), "path") => builder.path = Some(value),
                (None, other) => {
                    return Err(Wpd3Error::InvalidManifest(format!(
                        "unexpected top-level key '{other}'"
                    )));
                }
                (Some(_), other) => {
                    return Err(Wpd3Error::InvalidManifest(format!(
                        "unexpected scenario key '{other}'"
                    )));
                }
            }
        }

        if let Some(builder) = current.take() {
            scenarios.push(builder.build()?);
        }

        if scenarios.is_empty() {
            return Err(Wpd3Error::InvalidManifest(String::from(
                "missing scenarios",
            )));
        }

        Ok(Self {
            suite: suite
                .ok_or_else(|| Wpd3Error::InvalidManifest(String::from("missing suite")))?,
            baseline: baseline
                .ok_or_else(|| Wpd3Error::InvalidManifest(String::from("missing baseline")))?,
            scenarios,
        })
    }
}

impl RuntimeConfig {
    pub fn from_env() -> Result<Self, Wpd3Error> {
        let base_url = env::var("KOMGA_WPD3_BASE_URL").map_err(|_| {
            Wpd3Error::InvalidEnvironment(String::from("KOMGA_WPD3_BASE_URL is required"))
        })?;

        let cookie = optional_env("KOMGA_WPD3_COOKIE");
        let username = optional_env("KOMGA_WPD3_USERNAME");
        let password = optional_env("KOMGA_WPD3_PASSWORD");
        Self::from_components(Some(base_url), cookie, username, password)
    }

    fn auth_label(&self) -> String {
        match &self.auth {
            AuthMode::Anonymous => String::from("anonymous"),
            AuthMode::Cookie(_) => String::from("cookie"),
            AuthMode::Basic { username, .. } => format!("basic:{username}"),
        }
    }
}

fn optional_env(name: &str) -> Option<String> {
    env::var(name)
        .ok()
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
}

fn normalize_base_url(value: &str) -> Result<String, Wpd3Error> {
    let trimmed = value.trim().trim_end_matches('/');
    if trimmed.starts_with("http://") || trimmed.starts_with("https://") {
        Ok(trimmed.to_owned())
    } else {
        Err(Wpd3Error::InvalidEnvironment(String::from(
            "KOMGA_WPD3_BASE_URL must start with http:// or https://",
        )))
    }
}

fn parse_assignment(line: &str) -> Result<(&str, String), Wpd3Error> {
    let (key, raw_value) = line
        .split_once('=')
        .ok_or_else(|| Wpd3Error::InvalidManifest(format!("missing '=' in line '{line}'")))?;
    let key = key.trim();
    let value = raw_value.trim();

    if key.is_empty() {
        return Err(Wpd3Error::InvalidManifest(format!(
            "missing key in line '{line}'"
        )));
    }

    if !(value.starts_with('"') && value.ends_with('"') && value.len() >= 2) {
        return Err(Wpd3Error::InvalidManifest(format!(
            "value for '{key}' must be a quoted string"
        )));
    }

    Ok((key, value[1..value.len() - 1].to_owned()))
}

#[derive(Default)]
struct ScenarioBuilder {
    name: Option<String>,
    java_benchmark: Option<String>,
    java_case: Option<String>,
    method: Option<String>,
    path: Option<String>,
}

impl ScenarioBuilder {
    fn build(self) -> Result<Scenario, Wpd3Error> {
        Ok(Scenario {
            name: self
                .name
                .ok_or_else(|| Wpd3Error::InvalidManifest(String::from("scenario missing name")))?,
            java_benchmark: self.java_benchmark.ok_or_else(|| {
                Wpd3Error::InvalidManifest(String::from("scenario missing java_benchmark"))
            })?,
            java_case: self.java_case.ok_or_else(|| {
                Wpd3Error::InvalidManifest(String::from("scenario missing java_case"))
            })?,
            method: self.method.ok_or_else(|| {
                Wpd3Error::InvalidManifest(String::from("scenario missing method"))
            })?,
            path: self
                .path
                .ok_or_else(|| Wpd3Error::InvalidManifest(String::from("scenario missing path")))?,
        })
    }
}

impl RuntimeConfig {
    pub(crate) fn from_components(
        base_url: Option<String>,
        cookie: Option<String>,
        username: Option<String>,
        password: Option<String>,
    ) -> Result<Self, Wpd3Error> {
        let base_url = base_url.ok_or_else(|| {
            Wpd3Error::InvalidEnvironment(String::from("KOMGA_WPD3_BASE_URL is required"))
        })?;
        let base_url = normalize_base_url(&base_url)?;

        let auth = match (cookie, username, password) {
            (Some(cookie), None, None) => AuthMode::Cookie(cookie),
            (None, Some(username), Some(password)) => AuthMode::Basic { username, password },
            (None, None, None) => AuthMode::Anonymous,
            (Some(_), Some(_), _) | (Some(_), _, Some(_)) => {
                return Err(Wpd3Error::InvalidEnvironment(String::from(
                    "KOMGA_WPD3_COOKIE cannot be combined with username/password",
                )));
            }
            (None, Some(_), None) | (None, None, Some(_)) => {
                return Err(Wpd3Error::InvalidEnvironment(String::from(
                    "KOMGA_WPD3_USERNAME and KOMGA_WPD3_PASSWORD must be set together",
                )));
            }
        };

        Ok(Self { base_url, auth })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_default_manifest_with_expected_names() {
        let manifest = Manifest::parse(DEFAULT_MANIFEST).expect("default manifest should parse");

        let names: Vec<&str> = manifest
            .scenarios
            .iter()
            .map(|scenario| scenario.name.as_str())
            .collect();

        assert_eq!(manifest.suite, "wp-d3");
        assert_eq!(manifest.baseline, "java");
        assert_eq!(names, vec!["browse", "dashboard", "unsorted"]);
    }

    #[test]
    fn rejects_missing_base_url() {
        let error = RuntimeConfig::from_components(None, None, None, None)
            .expect_err("missing base url must fail");

        assert!(matches!(error, Wpd3Error::InvalidEnvironment(_)));
    }

    #[test]
    fn renders_deterministic_plan() {
        let manifest = Manifest::parse(DEFAULT_MANIFEST).expect("default manifest should parse");
        let runtime = RuntimeConfig::from_components(
            Some(String::from("https://komga.example.org/")),
            Some(String::from("session=abc")),
            None,
            None,
        )
        .expect("runtime config should parse");

        let plan = render_plan(&manifest, &runtime);

        assert!(plan.contains("suite: wp-d3"));
        assert!(plan.contains("base_url: https://komga.example.org"));
        assert!(plan.contains("auth: cookie"));
        assert!(plan.contains("  - browse | BrowseBenchmark::browseSeries | GET /api/v1/libraries/{libraryId}/series?sort=metadata.titleSort,asc"));
    }
}
