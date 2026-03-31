use std::path::PathBuf;

use super::env_config::RuntimeConfig;
use super::profile::RuntimeMode;

const ISOLATION_BLOCK_REASON: &str = "isolated mode requires explicit isolation or opt-in";
const SEARCH_INDEX_OWNERSHIP_REASON: &str =
    "search index ownership remains with external writer in isolated mode";

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WriterOwnershipPolicy {
    pub isolation_root: Option<PathBuf>,
    pub allow_isolated_writes: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WriterKind {
    MainDatabase,
    TasksDatabase,
    SearchIndex,
    FilesystemScanOutput,
    SidecarOutput,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WriterDecision {
    Allowed,
    Isolated,
    Blocked { reason: &'static str },
}

impl WriterDecision {
    pub fn allows_write(self) -> bool {
        matches!(self, Self::Allowed | Self::Isolated)
    }
}

impl RuntimeConfig {
    pub fn writer_decision(&self, writer: WriterKind) -> WriterDecision {
        match self.mode {
            RuntimeMode::Snapshot | RuntimeMode::Localdb => WriterDecision::Allowed,
            RuntimeMode::Canary => WriterDecision::Allowed,
            RuntimeMode::Isolated => {
                if matches!(writer, WriterKind::SearchIndex) {
                    return WriterDecision::Blocked {
                        reason: SEARCH_INDEX_OWNERSHIP_REASON,
                    };
                }

                if self.writer_ownership_policy.allow_isolated_writes {
                    if self.writer_ownership_policy.isolation_root.is_some() {
                        WriterDecision::Isolated
                    } else {
                        WriterDecision::Blocked {
                            reason: ISOLATION_BLOCK_REASON,
                        }
                    }
                } else {
                    match writer {
                        WriterKind::MainDatabase
                        | WriterKind::TasksDatabase
                        | WriterKind::SearchIndex
                        | WriterKind::FilesystemScanOutput
                        | WriterKind::SidecarOutput => WriterDecision::Blocked {
                            reason: ISOLATION_BLOCK_REASON,
                        },
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::RuntimeProfile;

    fn runtime_config_for(
        mode: RuntimeMode,
        allow_isolated_writes: bool,
        isolation_root: Option<&str>,
    ) -> RuntimeConfig {
        let mut config = RuntimeConfig::for_runtime_profile(RuntimeProfile::SnapshotAligned);
        config.mode = mode;
        config.writer_ownership_policy = WriterOwnershipPolicy {
            isolation_root: isolation_root.map(PathBuf::from),
            allow_isolated_writes,
        };
        config
    }

    #[test]
    fn snapshot_and_localdb_modes_allow_all_writers() {
        for mode in [RuntimeMode::Snapshot, RuntimeMode::Localdb] {
            let config = runtime_config_for(mode, false, None);
            for writer in [
                WriterKind::MainDatabase,
                WriterKind::TasksDatabase,
                WriterKind::SearchIndex,
                WriterKind::FilesystemScanOutput,
                WriterKind::SidecarOutput,
            ] {
                assert_eq!(config.writer_decision(writer), WriterDecision::Allowed);
            }
        }
    }

    #[test]
    fn canary_mode_allows_all_writers() {
        let config = runtime_config_for(RuntimeMode::Canary, false, None);
        for writer in [
            WriterKind::MainDatabase,
            WriterKind::TasksDatabase,
            WriterKind::SearchIndex,
            WriterKind::FilesystemScanOutput,
            WriterKind::SidecarOutput,
        ] {
            assert_eq!(config.writer_decision(writer), WriterDecision::Allowed);
        }
    }

    #[test]
    fn isolated_mode_without_opt_in_blocks_all_writers() {
        let config = runtime_config_for(RuntimeMode::Isolated, false, None);
        for writer in [
            WriterKind::MainDatabase,
            WriterKind::TasksDatabase,
            WriterKind::SearchIndex,
            WriterKind::FilesystemScanOutput,
            WriterKind::SidecarOutput,
        ] {
            let expected_reason = if writer == WriterKind::SearchIndex {
                "search index ownership remains with external writer in isolated mode"
            } else {
                "isolated mode requires explicit isolation or opt-in"
            };
            assert_eq!(
                config.writer_decision(writer),
                WriterDecision::Blocked {
                    reason: expected_reason,
                },
            );
        }
    }

    #[test]
    fn isolated_mode_with_opt_in_isolates_non_search_writers_and_still_blocks_search() {
        let config = runtime_config_for(RuntimeMode::Isolated, true, Some("/tmp/komga-isolated"));

        assert_eq!(
            config.writer_decision(WriterKind::MainDatabase),
            WriterDecision::Isolated,
        );
        assert_eq!(
            config.writer_decision(WriterKind::TasksDatabase),
            WriterDecision::Isolated,
        );
        assert_eq!(
            config.writer_decision(WriterKind::FilesystemScanOutput),
            WriterDecision::Isolated,
        );
        assert_eq!(
            config.writer_decision(WriterKind::SidecarOutput),
            WriterDecision::Isolated,
        );
        assert_eq!(
            config.writer_decision(WriterKind::SearchIndex),
            WriterDecision::Blocked {
                reason: "search index ownership remains with external writer in isolated mode",
            },
        );
    }
}
