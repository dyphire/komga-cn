use super::*;

pub(super) fn runtime_profile(config: &RuntimeConfig) -> RuntimeProfile {
    match config.runtime_profile {
        ConfigRuntimeProfile::SnapshotAligned => RuntimeProfile::SnapshotAligned,
        ConfigRuntimeProfile::LiveLocaldb => RuntimeProfile::LiveLocaldb,
    }
}
