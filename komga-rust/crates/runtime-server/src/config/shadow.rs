use std::path::PathBuf;

use super::cli::RuntimeConfig;
use super::profile::RuntimeMode;

const SHADOW_BLOCK_REASON: &str = "shadow mode requires explicit isolation or opt-in";
const SEARCH_INDEX_OWNERSHIP_REASON: &str =
    "search index ownership remains with java writer in shadow mode";

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ShadowPolicy {
    pub isolation_root: Option<PathBuf>,
    pub allow_shadow_writes: bool,
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
            RuntimeMode::Canary => WriterDecision::Blocked {
                reason: "canary mode requires explicit cutover wiring",
            },
            RuntimeMode::Shadow => {
                if matches!(writer, WriterKind::SearchIndex) {
                    return WriterDecision::Blocked {
                        reason: SEARCH_INDEX_OWNERSHIP_REASON,
                    };
                }

                if self.shadow_policy.allow_shadow_writes {
                    if self.shadow_policy.isolation_root.is_some() {
                        WriterDecision::Isolated
                    } else {
                        WriterDecision::Blocked {
                            reason: SHADOW_BLOCK_REASON,
                        }
                    }
                } else {
                    match writer {
                        WriterKind::MainDatabase
                        | WriterKind::TasksDatabase
                        | WriterKind::SearchIndex
                        | WriterKind::FilesystemScanOutput
                        | WriterKind::SidecarOutput => WriterDecision::Blocked {
                            reason: SHADOW_BLOCK_REASON,
                        },
                    }
                }
            }
        }
    }
}
