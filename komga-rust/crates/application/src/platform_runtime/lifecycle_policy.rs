#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StartupSearchTask {
    RebuildIndex,
    UpgradeIndex,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SearchIndexRecoveryStatus {
    Ready,
    RequiresRebuild,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RuntimeLifecycleError {
    pub message: String,
}

pub trait SearchIndexRecoveryPort {
    fn startup_recover(&self) -> Result<SearchIndexRecoveryStatus, RuntimeLifecycleError>;
    fn reset_for_rebuild(&self) -> Result<(), RuntimeLifecycleError>;
}

pub struct RuntimeLifecyclePolicy;

impl RuntimeLifecyclePolicy {
    pub fn startup_task_for_existing_index(has_existing_index: bool) -> Option<StartupSearchTask> {
        if has_existing_index {
            Some(StartupSearchTask::UpgradeIndex)
        } else {
            Some(StartupSearchTask::RebuildIndex)
        }
    }

    pub fn apply_search_startup_recovery<R>(
        recovery: &R,
    ) -> Result<Option<StartupSearchTask>, RuntimeLifecycleError>
    where
        R: SearchIndexRecoveryPort,
    {
        match recovery.startup_recover()? {
            SearchIndexRecoveryStatus::Ready => Ok(None),
            SearchIndexRecoveryStatus::RequiresRebuild => {
                recovery.reset_for_rebuild()?;
                Ok(Some(StartupSearchTask::RebuildIndex))
            }
        }
    }
}
