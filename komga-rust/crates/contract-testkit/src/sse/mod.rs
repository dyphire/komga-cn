mod comparison;
mod config;
mod parser;
mod report;

pub use comparison::compare_event_logs;
pub use config::{SseAudience, SseCaseConfig, SseHarnessConfig};
pub use parser::{
    NormalizedEvent, NormalizedEventLog, SseParseOptions, parse_event_log,
    parse_event_log_with_options,
};
pub use report::{SerializableEvent, SerializableEventLog, SseDiffReport, write_diff_report};
