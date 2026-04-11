#[path = "config/cli_args.rs"]
mod cli_args;
#[path = "config/env_config.rs"]
mod env_config;
#[path = "config/error.rs"]
mod error;
#[path = "config/path_resolution.rs"]
mod path_resolution;
#[path = "config/profile.rs"]
mod profile;
#[path = "config/startup_policy.rs"]
mod startup_policy;
#[path = "config/writer_ownership.rs"]
mod writer_ownership;

pub use cli_args::RuntimeCli;
pub(crate) use env_config::AdminActionConfig;
pub use env_config::{OAuth2ClientConfig, RuntimeConfig};
pub use error::ConfigError;
pub use profile::{PlatformProfile, RuntimeMode, RuntimeProfile};
pub use writer_ownership::{WriterDecision, WriterKind, WriterOwnershipPolicy};
