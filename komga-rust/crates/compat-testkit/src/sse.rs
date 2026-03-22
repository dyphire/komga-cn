#[path = "sse/comparison.rs"]
mod comparison;
#[path = "sse/config.rs"]
mod config;
#[path = "sse/parser.rs"]
mod parser;
#[path = "sse/report.rs"]
mod report;

pub use comparison::compare_event_logs;
pub use config::{SseAudience, SseCaseConfig, SseHarnessConfig};
pub use parser::{
    NormalizedEvent, NormalizedEventLog, SseParseOptions, parse_event_log,
    parse_event_log_with_options,
};
pub use report::{SerializableEvent, SerializableEventLog, SseDiffReport, write_diff_report};
