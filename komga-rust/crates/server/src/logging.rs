use std::cell::RefCell;
use std::fmt;
use std::fs::{File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

use time::OffsetDateTime;
use tracing::Dispatch;
use tracing_appender::non_blocking::{NonBlocking, WorkerGuard};
use tracing_subscriber::EnvFilter;
use tracing_subscriber::fmt::MakeWriter;
use tracing_subscriber::fmt::time::UtcTime;
use tracing_subscriber::layer::SubscriberExt;

use komga_config::env_config::RuntimeConfig;

pub const DEFAULT_ENV_FILTER: &str = "info,hyper=warn,h2=warn,sqlx=warn";
pub const SHARED_EVENT_FIELDS: &[&str] = &[
    "event",
    "request_id",
    "method",
    "route",
    "path",
    "status_code",
    "outcome",
    "latency_ms",
    "first_byte_ms",
    "user_id",
    "task_id",
    "task_type",
    "worker_id",
    "attempt",
    "error",
];
pub const STDERR_OUTPUT_CONTRACT: &str = "compact human-readable logs to stderr";
pub const FILE_OUTPUT_CONTRACT: &str = "newline-delimited JSON logs to the active logfile path";
const LOG_FILE_ROTATION_ENV: &str = "KOMGA_RUST_LOG_FILE_ROTATION";

static GLOBAL_LOGGING_RUNTIME: OnceLock<InstalledLoggingRuntime> = OnceLock::new();
static GLOBAL_LOGGING_INIT_LOCK: Mutex<()> = Mutex::new(());

thread_local! {
    static TEST_DISPLAY_CAPTURE: RefCell<Option<SharedBuffer>> = const { RefCell::new(None) };
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LoggingConfig {
    pub env_filter: String,
    pub active_log_file: PathBuf,
    pub rotation: FileRotation,
}

impl LoggingConfig {
    pub fn from_runtime_config(config: &RuntimeConfig) -> Result<Self, LoggingInitError> {
        Ok(Self {
            env_filter: std::env::var("RUST_LOG")
                .ok()
                .filter(|value| !value.trim().is_empty())
                .unwrap_or_else(|| DEFAULT_ENV_FILTER.to_string()),
            active_log_file: config.log_file.clone(),
            rotation: FileRotation::from_env()?,
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FileRotation {
    Never,
    Minutely,
    Hourly,
    Daily,
}

impl FileRotation {
    fn from_env() -> Result<Self, LoggingInitError> {
        std::env::var(LOG_FILE_ROTATION_ENV)
            .ok()
            .filter(|value| !value.trim().is_empty())
            .map(|value| Self::parse(value.as_str()))
            .transpose()?
            .map_or(Ok(Self::Daily), Ok)
    }

    fn parse(value: &str) -> Result<Self, LoggingInitError> {
        match value.trim().to_ascii_lowercase().as_str() {
            "never" => Ok(Self::Never),
            "minutely" | "minute" => Ok(Self::Minutely),
            "hourly" | "hour" => Ok(Self::Hourly),
            "daily" | "day" => Ok(Self::Daily),
            other => Err(LoggingInitError::InvalidRotation(other.to_string())),
        }
    }

    fn period_key(self, instant: OffsetDateTime) -> Option<String> {
        match self {
            Self::Never => None,
            Self::Minutely => Some(format!(
                "{:04}-{:02}-{:02}-{:02}-{:02}",
                instant.year(),
                u8::from(instant.month()),
                instant.day(),
                instant.hour(),
                instant.minute()
            )),
            Self::Hourly => Some(format!(
                "{:04}-{:02}-{:02}-{:02}",
                instant.year(),
                u8::from(instant.month()),
                instant.day(),
                instant.hour()
            )),
            Self::Daily => Some(format!(
                "{:04}-{:02}-{:02}",
                instant.year(),
                u8::from(instant.month()),
                instant.day()
            )),
        }
    }
}

#[derive(Debug)]
pub enum LoggingInitError {
    Io(io::Error),
    InvalidFilter(tracing_subscriber::filter::ParseError),
    InvalidRotation(String),
    Install(tracing::dispatcher::SetGlobalDefaultError),
}

pub struct CapturedTestLogs {
    pub display_output: String,
    pub structured_output: String,
}

impl fmt::Display for LoggingInitError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => write!(f, "logging I/O setup failed: {error}"),
            Self::InvalidFilter(error) => write!(f, "invalid logging filter: {error}"),
            Self::InvalidRotation(value) => write!(
                f,
                "invalid log rotation {value:?}; expected one of never|minutely|hourly|daily"
            ),
            Self::Install(error) => {
                write!(f, "failed to install global tracing subscriber: {error}")
            }
        }
    }
}

impl std::error::Error for LoggingInitError {}

impl From<io::Error> for LoggingInitError {
    fn from(value: io::Error) -> Self {
        Self::Io(value)
    }
}

impl From<tracing_subscriber::filter::ParseError> for LoggingInitError {
    fn from(value: tracing_subscriber::filter::ParseError) -> Self {
        Self::InvalidFilter(value)
    }
}

impl From<tracing::dispatcher::SetGlobalDefaultError> for LoggingInitError {
    fn from(value: tracing::dispatcher::SetGlobalDefaultError) -> Self {
        Self::Install(value)
    }
}

pub fn init_global(config: &RuntimeConfig) -> Result<bool, LoggingInitError> {
    let _lock = GLOBAL_LOGGING_INIT_LOCK
        .lock()
        .expect("logging init lock should not be poisoned");

    if GLOBAL_LOGGING_RUNTIME.get().is_some() {
        return Ok(false);
    }

    let runtime =
        InstalledLoggingRuntime::from_config(&LoggingConfig::from_runtime_config(config)?)?;
    tracing::dispatcher::set_global_default(runtime.dispatch.clone())?;
    let already_installed = GLOBAL_LOGGING_RUNTIME.set(runtime).is_err();
    Ok(!already_installed)
}

pub fn capture_for_test(
    config: &RuntimeConfig,
    action: impl FnOnce(),
) -> Result<String, LoggingInitError> {
    Ok(capture_outputs_for_test(config, action)?.structured_output)
}

pub fn capture_outputs_for_test(
    config: &RuntimeConfig,
    action: impl FnOnce(),
) -> Result<CapturedTestLogs, LoggingInitError> {
    let display_buffer = SharedBuffer::default();
    let shared_buffer = SharedBuffer::default();
    let dispatch = build_capture_dispatch(
        &LoggingConfig::from_runtime_config(config)?,
        display_buffer.clone(),
        shared_buffer.clone(),
    )?;
    let _guard = TestDisplayCaptureGuard::install(display_buffer.clone());
    tracing::dispatcher::with_default(&dispatch, action);
    Ok(CapturedTestLogs {
        display_output: display_buffer.into_string(),
        structured_output: shared_buffer.into_string(),
    })
}

pub fn emit_display(output: &str) -> io::Result<()> {
    if write_test_display_capture(output.as_bytes())? {
        return Ok(());
    }

    let mut stderr = io::stderr().lock();
    stderr.write_all(output.as_bytes())?;
    stderr.flush()
}

struct InstalledLoggingRuntime {
    dispatch: Dispatch,
    _guards: LoggingGuards,
}

impl InstalledLoggingRuntime {
    fn from_config(config: &LoggingConfig) -> Result<Self, LoggingInitError> {
        let stderr = tracing_appender::non_blocking(io::stderr());
        let logfile = StableFileAppender::new(config.active_log_file.clone(), config.rotation)?;
        let file = tracing_appender::non_blocking(logfile);
        let dispatch = build_global_dispatch(config, stderr.0.clone(), file.0.clone())?;

        Ok(Self {
            dispatch,
            _guards: LoggingGuards {
                _stderr: stderr.1,
                _file: file.1,
            },
        })
    }
}

struct LoggingGuards {
    _stderr: WorkerGuard,
    _file: WorkerGuard,
}

type LogClock = Arc<dyn Fn() -> OffsetDateTime + Send + Sync>;

/// Keeps `RuntimeConfig.log_file` as the stable active path while archiving elapsed
/// periods beside it. `tracing_appender::rolling::*` swaps the write target to a
/// suffixed file, which would break `/actuator/logfile` because that endpoint reads
/// the fixed active path from runtime state.
pub struct StableFileAppender {
    active_log_file: PathBuf,
    clock: LogClock,
    state: Mutex<StableFileAppenderState>,
}

struct StableFileAppenderState {
    rotation: FileRotation,
    active_period_key: Option<String>,
    file: File,
}

impl StableFileAppender {
    pub fn new(active_log_file: PathBuf, rotation: FileRotation) -> io::Result<Self> {
        Self::new_with_clock(active_log_file, rotation, OffsetDateTime::now_utc)
    }

    pub fn new_with_clock<F>(
        active_log_file: PathBuf,
        rotation: FileRotation,
        clock: F,
    ) -> io::Result<Self>
    where
        F: Fn() -> OffsetDateTime + Send + Sync + 'static,
    {
        let clock: LogClock = Arc::new(clock);
        ensure_log_parent_exists(&active_log_file)?;
        rotate_stale_startup_file_if_needed(&active_log_file, rotation, &clock)?;
        let now = clock.as_ref()();
        let file = open_active_log_file(&active_log_file)?;
        let active_period_key = rotation.period_key(now);

        Ok(Self {
            active_log_file,
            clock,
            state: Mutex::new(StableFileAppenderState {
                rotation,
                active_period_key,
                file,
            }),
        })
    }

    pub fn active_path(&self) -> &Path {
        self.active_log_file.as_path()
    }

    fn rotate_if_needed(&self, state: &mut StableFileAppenderState) -> io::Result<()> {
        let Some(next_period_key) = state.rotation.period_key(self.clock.as_ref()()) else {
            return Ok(());
        };
        let Some(current_period_key) = state.active_period_key.as_ref() else {
            state.active_period_key = Some(next_period_key);
            return Ok(());
        };
        if current_period_key == &next_period_key {
            return Ok(());
        }

        state.file.flush()?;
        rotate_active_file_to_archive(&self.active_log_file, current_period_key)?;
        state.file = open_active_log_file(&self.active_log_file)?;
        state.active_period_key = Some(next_period_key);
        Ok(())
    }
}

impl Write for StableFileAppender {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let mut state = self
            .state
            .lock()
            .expect("stable logfile appender state should not be poisoned");
        self.rotate_if_needed(&mut state)?;
        state.file.write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.state
            .lock()
            .expect("stable logfile appender state should not be poisoned")
            .file
            .flush()
    }
}

fn ensure_log_parent_exists(active_log_file: &Path) -> io::Result<()> {
    if let Some(parent) = active_log_file.parent() {
        std::fs::create_dir_all(parent)?;
    }
    Ok(())
}

fn open_active_log_file(active_log_file: &Path) -> io::Result<File> {
    OpenOptions::new()
        .create(true)
        .append(true)
        .open(active_log_file)
}

fn rotate_stale_startup_file_if_needed(
    active_log_file: &Path,
    rotation: FileRotation,
    clock: &LogClock,
) -> io::Result<()> {
    if rotation == FileRotation::Never || !active_log_file.exists() {
        return Ok(());
    }

    let metadata = std::fs::metadata(active_log_file)?;
    if metadata.len() == 0 {
        return Ok(());
    }

    let Some(current_period_key) = rotation.period_key(clock.as_ref()()) else {
        return Ok(());
    };
    let modified_at = OffsetDateTime::from(metadata.modified()?);
    let Some(existing_period_key) = rotation.period_key(modified_at) else {
        return Ok(());
    };
    if existing_period_key == current_period_key {
        return Ok(());
    }

    rotate_active_file_to_archive(active_log_file, &existing_period_key)
}

fn rotate_active_file_to_archive(
    active_log_file: &Path,
    archived_period_key: &str,
) -> io::Result<()> {
    if !active_log_file.exists() {
        return Ok(());
    }
    let archive_path = next_archive_path(active_log_file, archived_period_key);
    std::fs::rename(active_log_file, archive_path)
}

fn next_archive_path(active_log_file: &Path, archived_period_key: &str) -> PathBuf {
    let file_name = active_log_file
        .file_name()
        .expect("active logfile should always have a filename")
        .to_string_lossy();
    let parent = active_log_file
        .parent()
        .expect("active logfile should always have a parent directory");
    let base_name = format!("{file_name}.{archived_period_key}");
    let mut attempt = 0usize;
    loop {
        let candidate = if attempt == 0 {
            parent.join(&base_name)
        } else {
            parent.join(format!("{base_name}.{attempt}"))
        };
        if !candidate.exists() {
            return candidate;
        }
        attempt += 1;
    }
}

fn build_global_dispatch(
    config: &LoggingConfig,
    stderr_writer: NonBlocking,
    file_writer: NonBlocking,
) -> Result<Dispatch, LoggingInitError> {
    build_dispatch(config, stderr_writer, file_writer)
}

fn build_capture_dispatch(
    config: &LoggingConfig,
    display_writer: SharedBuffer,
    structured_writer: SharedBuffer,
) -> Result<Dispatch, LoggingInitError> {
    build_dispatch(config, display_writer, structured_writer)
}

fn build_dispatch<DisplayWriter, StructuredWriter>(
    config: &LoggingConfig,
    display_writer: DisplayWriter,
    structured_writer: StructuredWriter,
) -> Result<Dispatch, LoggingInitError>
where
    DisplayWriter: for<'writer> MakeWriter<'writer> + Send + Sync + 'static,
    StructuredWriter: for<'writer> MakeWriter<'writer> + Send + Sync + 'static,
{
    let filter = EnvFilter::try_new(config.env_filter.as_str())?;
    let timer = UtcTime::rfc_3339();
    let display_layer = tracing_subscriber::fmt::layer()
        .with_target(false)
        .with_timer(timer.clone())
        .compact()
        .with_writer(display_writer);
    let structured_layer = tracing_subscriber::fmt::layer()
        .json()
        .with_current_span(false)
        .with_span_list(false)
        .with_timer(timer)
        .with_writer(structured_writer);

    Ok(Dispatch::new(
        tracing_subscriber::registry()
            .with(filter)
            .with(display_layer)
            .with(structured_layer),
    ))
}

struct TestDisplayCaptureGuard {
    previous: Option<SharedBuffer>,
}

impl TestDisplayCaptureGuard {
    fn install(writer: SharedBuffer) -> Self {
        let previous = TEST_DISPLAY_CAPTURE.with(|slot| slot.replace(Some(writer)));
        Self { previous }
    }
}

impl Drop for TestDisplayCaptureGuard {
    fn drop(&mut self) {
        let previous = self.previous.take();
        TEST_DISPLAY_CAPTURE.with(|slot| {
            slot.replace(previous);
        });
    }
}

#[derive(Clone, Default)]
struct SharedBuffer {
    inner: Arc<Mutex<Vec<u8>>>,
}

impl SharedBuffer {
    fn into_string(self) -> String {
        let bytes = self
            .inner
            .lock()
            .expect("logging capture buffer should not be poisoned")
            .clone();
        String::from_utf8(bytes).expect("logging capture buffer should contain UTF-8")
    }

    fn write_all(&self, buf: &[u8]) -> io::Result<()> {
        self.inner
            .lock()
            .expect("logging capture buffer should not be poisoned")
            .extend_from_slice(buf);
        Ok(())
    }
}

fn write_test_display_capture(buf: &[u8]) -> io::Result<bool> {
    TEST_DISPLAY_CAPTURE.with(|slot| {
        let maybe_writer = slot.borrow().clone();
        if let Some(writer) = maybe_writer {
            writer.write_all(buf)?;
            return Ok(true);
        }

        Ok(false)
    })
}

impl<'a> MakeWriter<'a> for SharedBuffer {
    type Writer = SharedBufferWriter;

    fn make_writer(&'a self) -> Self::Writer {
        SharedBufferWriter {
            inner: Arc::clone(&self.inner),
        }
    }
}

struct SharedBufferWriter {
    inner: Arc<Mutex<Vec<u8>>>,
}

impl Write for SharedBufferWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.inner
            .lock()
            .expect("logging capture buffer should not be poisoned")
            .extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
