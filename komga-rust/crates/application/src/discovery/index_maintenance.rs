use std::error::Error;
use std::fmt::{Display, Formatter};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DiscoveryIndexEntityType {
    Book,
    Series,
    Collection,
    ReadList,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DiscoveryIndexDocument {
    pub entity_type: DiscoveryIndexEntityType,
    pub id: String,
    pub title: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DiscoveryIndexEvent {
    Upsert(DiscoveryIndexDocument),
    Delete {
        entity_type: DiscoveryIndexEntityType,
        id: String,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DiscoveryIndexStartupState {
    Ready,
    RequiresExplicitRebuild,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DiscoveryIndexError {
    message: String,
}

impl DiscoveryIndexError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl Display for DiscoveryIndexError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl Error for DiscoveryIndexError {}

pub trait DiscoveryIndexLifecyclePort {
    fn startup_recover(&self) -> Result<DiscoveryIndexStartupState, DiscoveryIndexError>;

    fn reset_for_rebuild(&self) -> Result<(), DiscoveryIndexError>;

    fn rebuild(&self, docs: &[DiscoveryIndexDocument]) -> Result<(), DiscoveryIndexError>;

    fn apply_event(&self, event: DiscoveryIndexEvent) -> Result<(), DiscoveryIndexError>;
}

pub struct DiscoveryIndexMaintenance<I> {
    lifecycle: I,
}

impl<I> DiscoveryIndexMaintenance<I>
where
    I: DiscoveryIndexLifecyclePort,
{
    pub fn new(lifecycle: I) -> Self {
        Self { lifecycle }
    }

    pub fn rebuild_index(
        &self,
        docs: &[DiscoveryIndexDocument],
    ) -> Result<(), DiscoveryIndexError> {
        self.lifecycle.rebuild(docs)
    }

    pub fn apply(&self, event: DiscoveryIndexEvent) -> Result<(), DiscoveryIndexError> {
        self.lifecycle.apply_event(event)
    }
}
