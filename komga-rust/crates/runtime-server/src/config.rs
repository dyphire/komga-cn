#[path = "config/cli.rs"]
mod cli;
#[path = "config/error.rs"]
mod error;
#[path = "config/profile.rs"]
mod profile;
#[path = "config/shadow.rs"]
mod shadow;

pub use cli::{RuntimeCli, RuntimeConfig};
pub use error::ConfigError;
pub use profile::{CompatProfile, PlatformProfile, RuntimeMode};
pub use shadow::{ShadowPolicy, WriterDecision, WriterKind};
