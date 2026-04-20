use std::path::PathBuf;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SqlitePoolSnapshot {
    pub path: PathBuf,
    pub max_connections: u32,
    pub min_connections: u32,
    pub total_connections: u32,
    pub idle_connections: u32,
    pub in_use_connections: u32,
    pub is_closed: bool,
}
