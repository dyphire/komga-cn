use super::*;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RuntimeProfile {
    SnapshotAligned,
    LiveLocaldb,
}

#[derive(Clone)]
pub struct AuthDatabaseState {
    pub database_file: PathBuf,
    pub demo_mode: bool,
    pub session_runtime_key: String,
    pub remember_me_runtime_key: String,
    pub runtime_identity: Arc<dyn IdentityService>,
}

#[derive(Clone)]
pub struct RuntimeState {
    pub database_file: PathBuf,
    pub tasks_db_file: PathBuf,
    pub lucene_data_directory: PathBuf,
    pub fonts_data_directory: PathBuf,
    pub log_file: PathBuf,
    pub config_dir: Option<PathBuf>,
    pub bind_address: SocketAddr,
    pub server_context_path: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct StartupTimingSnapshot {
    pub application_started_time_seconds: f64,
    pub application_ready_time_seconds: f64,
}

#[derive(Clone, Default)]
pub struct StartupTimingState {
    snapshot: Arc<Mutex<StartupTimingSnapshot>>,
}

impl StartupTimingState {
    pub fn snapshot(&self) -> StartupTimingSnapshot {
        self.snapshot
            .lock()
            .expect("startup timing snapshot lock should not be poisoned")
            .clone()
    }

    pub fn record_application_started(&self, elapsed: Duration) {
        self.snapshot
            .lock()
            .expect("startup timing snapshot lock should not be poisoned")
            .application_started_time_seconds = elapsed.as_secs_f64();
    }

    pub fn record_application_ready(&self, elapsed: Duration) {
        self.snapshot
            .lock()
            .expect("startup timing snapshot lock should not be poisoned")
            .application_ready_time_seconds = elapsed.as_secs_f64();
    }
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct HttpServerRequestMetricKey {
    pub exception: String,
    pub method: String,
    pub outcome: String,
    pub status: String,
    pub uri: String,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct HttpServerRequestMetricSummary {
    pub count: u64,
    pub total_time_seconds: f64,
    pub max_time_seconds: f64,
}

#[derive(Clone, Default)]
pub struct HttpServerRequestsState {
    metrics: Arc<Mutex<HashMap<HttpServerRequestMetricKey, HttpServerRequestMetricSummary>>>,
}

impl HttpServerRequestsState {
    pub fn record(&self, key: HttpServerRequestMetricKey, elapsed: Duration) {
        let elapsed_seconds = elapsed.as_secs_f64();
        let mut metrics = self
            .metrics
            .lock()
            .expect("http server request metrics lock should not be poisoned");
        let entry = metrics.entry(key).or_default();
        entry.count += 1;
        entry.total_time_seconds += elapsed_seconds;
        entry.max_time_seconds = entry.max_time_seconds.max(elapsed_seconds);
    }

    pub fn snapshot(&self) -> Vec<(HttpServerRequestMetricKey, HttpServerRequestMetricSummary)> {
        self.metrics
            .lock()
            .expect("http server request metrics lock should not be poisoned")
            .iter()
            .map(|(key, value)| (key.clone(), value.clone()))
            .collect()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OperationalBuildMetadata {
    pub version: String,
    pub build_time: String,
    pub git_branch: Option<String>,
    pub git_commit_id: Option<String>,
    pub git_commit_time: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OAuth2ClientConfig {
    pub registration_id: String,
    pub client_name: String,
    pub client_id: String,
    pub client_secret: String,
    pub authorization_uri: String,
    pub token_uri: String,
    pub user_info_uri: Option<String>,
    pub scopes: Vec<String>,
}
