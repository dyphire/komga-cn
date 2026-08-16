use serde_json::Value;
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;

use super::{
    BookProgressionInput, BookProgressionReaderPort, BookProgressionRecord, ProgressWriterPort,
};

#[async_trait::async_trait]
pub trait BookProgressionWriteReaderPort: Send + Sync {
    async fn book_progression(
        &self,
        book_id: &str,
        user_id: &str,
    ) -> anyhow::Result<Option<BookProgressionRecord>>;
}

#[async_trait::async_trait]
impl<T> BookProgressionWriteReaderPort for T
where
    T: BookProgressionReaderPort + ?Sized,
{
    async fn book_progression(
        &self,
        book_id: &str,
        user_id: &str,
    ) -> anyhow::Result<Option<BookProgressionRecord>> {
        BookProgressionReaderPort::book_progression(self, book_id, user_id).await
    }
}

#[async_trait::async_trait]
pub trait BookProgressionWriterPort: Send + Sync {
    async fn persist_book_progression(&self, input: BookProgressionInput) -> anyhow::Result<()>;
}

#[async_trait::async_trait]
impl<T> BookProgressionWriterPort for T
where
    T: ProgressWriterPort + ?Sized,
{
    async fn persist_book_progression(&self, input: BookProgressionInput) -> anyhow::Result<()> {
        ProgressWriterPort::persist_book_progression(self, input).await
    }
}

pub(crate) struct BookProgressionWriteService<'a, R, W>
where
    R: BookProgressionWriteReaderPort + ?Sized,
    W: BookProgressionWriterPort + ?Sized,
{
    reader: &'a R,
    writer: &'a W,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum BookProgressionConflictPolicy {
    Overwrite,
    RejectStale,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct BookProgressionWrite {
    pub(crate) book_id: String,
    pub(crate) user_id: String,
    pub(crate) page_count: u64,
    pub(crate) source: BookProgressionWriteSource,
    pub(crate) modified: Option<String>,
    pub(crate) device_id: Option<String>,
    pub(crate) device_name: Option<String>,
    pub(crate) conflict_policy: BookProgressionConflictPolicy,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum BookProgressionWriteSource {
    TotalProgression {
        progression: f64,
        total_progression: Option<f64>,
        locator: Option<Value>,
    },
    Position {
        progression: f64,
        position: u64,
        total_progression: Option<f64>,
        locator: Option<Value>,
    },
}

#[derive(Debug)]
pub(crate) enum BookProgressionWriteError {
    Stale,
    Internal(anyhow::Error),
}

#[cfg(test)]
impl PartialEq for BookProgressionWriteError {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Stale, Self::Stale) => true,
            (Self::Internal(left), Self::Internal(right)) => left.to_string() == right.to_string(),
            _ => false,
        }
    }
}

struct ResolvedProgressWrite {
    page: u64,
    completed: bool,
}

impl<'a, R, W> BookProgressionWriteService<'a, R, W>
where
    R: BookProgressionWriteReaderPort + ?Sized,
    W: BookProgressionWriterPort + ?Sized,
{
    pub(crate) fn new(reader: &'a R, writer: &'a W) -> Self {
        Self { reader, writer }
    }

    pub(crate) async fn persist(
        &self,
        write: BookProgressionWrite,
    ) -> Result<(), BookProgressionWriteError> {
        if write.conflict_policy == BookProgressionConflictPolicy::RejectStale
            && let Some(modified) = write.modified.as_deref()
            && self
                .write_is_stale(&write.book_id, &write.user_id, modified)
                .await?
        {
            return Err(BookProgressionWriteError::Stale);
        }

        let resolved = resolve_progress_write(
            write.page_count,
            write.source.progression(),
            write.source.total_progression(),
            write.source.position(),
        );

        self.writer
            .persist_book_progression(BookProgressionInput {
                book_id: write.book_id,
                user_id: write.user_id,
                page: resolved.page,
                completed: resolved.completed,
                modified: write.modified,
                device_id: write.device_id,
                device_name: write.device_name,
                locator: write.source.into_locator(),
            })
            .await
            .map_err(BookProgressionWriteError::Internal)
    }

    async fn write_is_stale(
        &self,
        book_id: &str,
        user_id: &str,
        modified: &str,
    ) -> Result<bool, BookProgressionWriteError> {
        let Ok(new_modified) = OffsetDateTime::parse(modified, &Rfc3339) else {
            return Ok(false);
        };
        let Some(existing_progression) = self
            .reader
            .book_progression(book_id, user_id)
            .await
            .map_err(BookProgressionWriteError::Internal)?
        else {
            return Ok(false);
        };
        let Ok(existing_modified) = OffsetDateTime::parse(&existing_progression.modified, &Rfc3339)
        else {
            return Ok(false);
        };

        Ok(new_modified <= existing_modified)
    }
}

impl BookProgressionWriteSource {
    fn progression(&self) -> f64 {
        match self {
            Self::TotalProgression { progression, .. } | Self::Position { progression, .. } => {
                *progression
            }
        }
    }

    fn position(&self) -> Option<u64> {
        match self {
            Self::TotalProgression { .. } => None,
            Self::Position { position, .. } => Some(*position),
        }
    }

    fn total_progression(&self) -> Option<f64> {
        match self {
            Self::TotalProgression {
                total_progression, ..
            }
            | Self::Position {
                total_progression, ..
            } => *total_progression,
        }
    }

    fn into_locator(self) -> Option<Value> {
        match self {
            Self::TotalProgression { locator, .. } | Self::Position { locator, .. } => locator,
        }
    }
}

fn resolve_progress_write(
    page_count: u64,
    progression: f64,
    total_progression: Option<f64>,
    position: Option<u64>,
) -> ResolvedProgressWrite {
    let page_count = page_count.max(1);
    let effective_progression = total_progression.unwrap_or(progression);
    let page_from_progression = (effective_progression * page_count as f64)
        .round()
        .clamp(0.0, page_count as f64) as u64;
    let page = position
        .filter(|value| *value >= 1)
        .unwrap_or(page_from_progression);

    ResolvedProgressWrite {
        page,
        completed: effective_progression >= 0.99,
    }
}
