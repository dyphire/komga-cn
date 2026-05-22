use std::collections::HashMap;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

pub use komga_application::task_processing::{TaskQueue, TaskQueueAdmin};
use serde_json::Value;
use tokio::sync::watch;

use crate::discovery_auth::state::DiscoveryAuthState;

mod app_state;
mod core;
mod discovery;
mod identity;
mod library_catalog;
mod media_assets;
mod opds;
mod operational;
mod task_queue;

pub use app_state::*;
pub use core::*;
pub use discovery::*;
pub use identity::*;
pub use library_catalog::*;
pub use media_assets::*;
pub use opds::*;
pub use operational::*;
pub use task_queue::*;

#[cfg(test)]
pub(crate) mod tests;
