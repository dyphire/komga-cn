use super::*;
use std::sync::RwLock;
use std::sync::atomic::AtomicU64;
use std::sync::atomic::Ordering;

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
    pub configuration_bind_address: SocketAddr,
    pub server_context_path: Option<String>,
    pub configuration_server_context_path: Option<String>,
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

#[derive(Default)]
struct HttpServerRequestMetricCell {
    count: AtomicU64,
    total_time_micros: AtomicU64,
    max_time_micros: AtomicU64,
}

impl HttpServerRequestMetricCell {
    fn record(&self, elapsed_micros: u64) {
        self.count.fetch_add(1, Ordering::Relaxed);
        self.total_time_micros
            .fetch_add(elapsed_micros, Ordering::Relaxed);
        let mut current_max = self.max_time_micros.load(Ordering::Relaxed);
        while elapsed_micros > current_max {
            match self.max_time_micros.compare_exchange_weak(
                current_max,
                elapsed_micros,
                Ordering::Relaxed,
                Ordering::Relaxed,
            ) {
                Ok(_) => break,
                Err(observed) => current_max = observed,
            }
        }
    }

    fn snapshot(&self) -> HttpServerRequestMetricSummary {
        HttpServerRequestMetricSummary {
            count: self.count.load(Ordering::Relaxed),
            total_time_seconds: micros_to_seconds(self.total_time_micros.load(Ordering::Relaxed)),
            max_time_seconds: micros_to_seconds(self.max_time_micros.load(Ordering::Relaxed)),
        }
    }
}

#[derive(Clone, Default)]
pub struct HttpServerRequestsState {
    metrics: Arc<RwLock<HashMap<HttpServerRequestMetricKey, Arc<HttpServerRequestMetricCell>>>>,
}

impl HttpServerRequestsState {
    pub fn record(&self, key: HttpServerRequestMetricKey, elapsed: Duration) {
        let elapsed_micros = u64::try_from(elapsed.as_micros()).unwrap_or(u64::MAX);
        if let Some(cell) = self
            .metrics
            .read()
            .expect("http server request metrics lock should not be poisoned")
            .get(&key)
            .cloned()
        {
            cell.record(elapsed_micros);
            return;
        }

        let cell = {
            let mut metrics = self
                .metrics
                .write()
                .expect("http server request metrics lock should not be poisoned");
            metrics
                .entry(key)
                .or_insert_with(|| Arc::new(HttpServerRequestMetricCell::default()))
                .clone()
        };
        cell.record(elapsed_micros);
    }

    pub fn snapshot(&self) -> Vec<(HttpServerRequestMetricKey, HttpServerRequestMetricSummary)> {
        self.metrics
            .read()
            .expect("http server request metrics lock should not be poisoned")
            .iter()
            .map(|(key, cell)| (key.clone(), cell.snapshot()))
            .collect()
    }
}

fn micros_to_seconds(value: u64) -> f64 {
    value as f64 / 1_000_000.0
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
