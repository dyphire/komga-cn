use std::sync::Arc;

use super::{
    ProgressWriterPort, ReadProgressSurfacePort, ReadlistTachiyomiCounters, SeriesTachiyomiProgress,
};

#[async_trait::async_trait]
pub trait SeriesReadProgressWriterPort: Send + Sync {
    async fn persist_read_progress(
        &self,
        book_id: &str,
        user_id: &str,
        page: u64,
        completed: bool,
    ) -> Result<(), String>;

    async fn delete_read_progress(&self, book_id: &str, user_id: &str) -> Result<(), String>;

    async fn refresh_series_read_progress(
        &self,
        series_id: &str,
        user_id: &str,
    ) -> Result<(), String>;

    async fn delete_series_read_progress(
        &self,
        series_id: &str,
        user_id: &str,
    ) -> Result<(), String>;
}

#[async_trait::async_trait]
impl<T> SeriesReadProgressWriterPort for T
where
    T: ProgressWriterPort + ?Sized,
{
    async fn persist_read_progress(
        &self,
        book_id: &str,
        user_id: &str,
        page: u64,
        completed: bool,
    ) -> Result<(), String> {
        ProgressWriterPort::persist_read_progress(self, book_id, user_id, page, completed, None)
            .await
    }

    async fn delete_read_progress(&self, book_id: &str, user_id: &str) -> Result<(), String> {
        ProgressWriterPort::delete_read_progress(self, book_id, user_id).await
    }

    async fn refresh_series_read_progress(
        &self,
        series_id: &str,
        user_id: &str,
    ) -> Result<(), String> {
        ProgressWriterPort::refresh_series_read_progress(self, series_id, user_id).await
    }

    async fn delete_series_read_progress(
        &self,
        series_id: &str,
        user_id: &str,
    ) -> Result<(), String> {
        ProgressWriterPort::delete_series_read_progress(self, series_id, user_id).await
    }
}

pub struct ReadProgressService {
    reader: Arc<dyn ReadProgressSurfacePort>,
    writer: Arc<dyn SeriesReadProgressWriterPort>,
}

impl ReadProgressService {
    pub fn new(
        reader: Arc<dyn ReadProgressSurfacePort>,
        writer: Arc<dyn SeriesReadProgressWriterPort>,
    ) -> Self {
        Self { reader, writer }
    }

    pub async fn mark_series_complete(&self, series_id: &str, user_id: &str) -> Result<(), String> {
        let book_ids = self.reader.series_book_ids(series_id).await?;
        self.mark_books_complete(book_ids, user_id).await?;
        self.writer
            .refresh_series_read_progress(series_id, user_id)
            .await
    }

    pub async fn delete_series_progress(
        &self,
        series_id: &str,
        user_id: &str,
    ) -> Result<(), String> {
        let book_ids = self.reader.series_book_ids(series_id).await?;
        for book_id in book_ids {
            self.writer.delete_read_progress(&book_id, user_id).await?;
        }
        self.writer
            .delete_series_read_progress(series_id, user_id)
            .await
    }

    pub async fn mark_series_tachiyomi_progress(
        &self,
        series_id: &str,
        user_id: &str,
        last_number_sort_read: f64,
    ) -> Result<(), String> {
        let book_ids = self
            .reader
            .series_book_number_sorts(series_id)
            .await?
            .into_iter()
            .filter(|book| book.number_sort <= last_number_sort_read)
            .map(|book| book.book_id)
            .collect::<Vec<_>>();

        self.mark_books_complete(book_ids, user_id).await?;
        self.writer
            .refresh_series_read_progress(series_id, user_id)
            .await
    }

    pub async fn mark_readlist_tachiyomi_progress(
        &self,
        ordered_book_ids: &[String],
        user_id: &str,
        last_book_read: usize,
    ) -> Result<(), String> {
        let book_ids = ordered_book_ids
            .iter()
            .take(last_book_read)
            .cloned()
            .collect::<Vec<_>>();

        self.mark_books_complete(book_ids, user_id).await
    }

    pub async fn readlist_tachiyomi_counters(
        &self,
        ordered_book_ids: &[String],
        user_id: &str,
    ) -> Result<ReadlistTachiyomiCounters, String> {
        let completed_states = self
            .reader
            .read_progress_completed_by_book_ids(ordered_book_ids, user_id)
            .await?;

        let books_count = ordered_book_ids.len() as u64;
        let books_read_count = completed_states
            .iter()
            .filter(|completed| **completed == Some(true))
            .count() as u64;
        let books_in_progress_count = completed_states
            .iter()
            .filter(|completed| **completed == Some(false))
            .count() as u64;
        let books_unread_count = completed_states
            .iter()
            .filter(|completed| completed.is_none())
            .count() as u64;

        let mut last_read_continuous_index = 0_u64;
        for completed in completed_states {
            if completed == Some(true) {
                last_read_continuous_index += 1;
            } else {
                break;
            }
        }

        Ok(ReadlistTachiyomiCounters {
            books_count,
            books_read_count,
            books_unread_count,
            books_in_progress_count,
            last_read_continuous_index,
        })
    }

    pub async fn series_tachiyomi_progress(
        &self,
        series_id: &str,
        user_id: &str,
    ) -> Result<SeriesTachiyomiProgress, String> {
        let books = self
            .reader
            .series_tachiyomi_progress_books(series_id, user_id)
            .await?;

        Ok(SeriesTachiyomiProgress::from_books(books))
    }

    async fn mark_books_complete(
        &self,
        book_ids: Vec<String>,
        user_id: &str,
    ) -> Result<(), String> {
        for book_id in book_ids {
            if self
                .reader
                .book_read_progress_completed(&book_id, user_id)
                .await?
                == Some(true)
            {
                continue;
            }

            let page_count = self
                .reader
                .book_page_count(&book_id)
                .await?
                .unwrap_or(1)
                .max(1);
            self.writer
                .persist_read_progress(&book_id, user_id, page_count, true)
                .await?;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::Mutex;

    use super::*;
    use crate::media_assets::{SeriesBookNumberSort, SeriesTachiyomiProgressBook};

    #[derive(Default)]
    struct TestReadProgressSurface {
        series_book_numbers: HashMap<String, Vec<SeriesBookNumberSort>>,
        completed_by_book: HashMap<String, Option<bool>>,
        page_count_by_book: HashMap<String, Option<u64>>,
    }

    #[async_trait::async_trait]
    impl ReadProgressSurfacePort for TestReadProgressSurface {
        async fn series_book_ids(&self, series_id: &str) -> Result<Vec<String>, String> {
            Ok(self
                .series_book_numbers
                .get(series_id)
                .cloned()
                .unwrap_or_default()
                .into_iter()
                .map(|book| book.book_id)
                .collect())
        }

        async fn series_book_number_sorts(
            &self,
            series_id: &str,
        ) -> Result<Vec<SeriesBookNumberSort>, String> {
            Ok(self
                .series_book_numbers
                .get(series_id)
                .cloned()
                .unwrap_or_default())
        }

        async fn series_tachiyomi_progress_books(
            &self,
            series_id: &str,
            _user_id: &str,
        ) -> Result<Vec<SeriesTachiyomiProgressBook>, String> {
            Ok(self
                .series_book_numbers
                .get(series_id)
                .cloned()
                .unwrap_or_default()
                .into_iter()
                .map(|book| SeriesTachiyomiProgressBook {
                    number_sort: book.number_sort,
                    completed: self.completed_by_book.get(&book.book_id).copied().flatten(),
                })
                .collect())
        }

        async fn book_read_progress_completed(
            &self,
            book_id: &str,
            _user_id: &str,
        ) -> Result<Option<bool>, String> {
            Ok(self.completed_by_book.get(book_id).copied().flatten())
        }

        async fn read_progress_completed_by_book_ids(
            &self,
            ordered_book_ids: &[String],
            _user_id: &str,
        ) -> Result<Vec<Option<bool>>, String> {
            Ok(ordered_book_ids
                .iter()
                .map(|book_id| self.completed_by_book.get(book_id).copied().flatten())
                .collect())
        }

        async fn book_page_count(&self, book_id: &str) -> Result<Option<u64>, String> {
            Ok(self.page_count_by_book.get(book_id).copied().flatten())
        }
    }

    #[derive(Clone, Debug, Eq, PartialEq)]
    struct PersistedProgressWrite {
        book_id: String,
        page: u64,
        completed: bool,
    }

    #[derive(Default)]
    struct TestProgressWriter {
        persisted: Mutex<Vec<PersistedProgressWrite>>,
        deleted_books: Mutex<Vec<String>>,
        refreshed_series: Mutex<Vec<String>>,
        deleted_series: Mutex<Vec<String>>,
    }

    impl TestProgressWriter {
        fn persisted(&self) -> Vec<PersistedProgressWrite> {
            self.persisted.lock().unwrap().clone()
        }

        fn refreshed_series(&self) -> Vec<String> {
            self.refreshed_series.lock().unwrap().clone()
        }
    }

    #[async_trait::async_trait]
    impl SeriesReadProgressWriterPort for TestProgressWriter {
        async fn persist_read_progress(
            &self,
            book_id: &str,
            _user_id: &str,
            page: u64,
            completed: bool,
        ) -> Result<(), String> {
            self.persisted.lock().unwrap().push(PersistedProgressWrite {
                book_id: book_id.to_string(),
                page,
                completed,
            });
            Ok(())
        }

        async fn delete_read_progress(&self, book_id: &str, _user_id: &str) -> Result<(), String> {
            self.deleted_books.lock().unwrap().push(book_id.to_string());
            Ok(())
        }

        async fn refresh_series_read_progress(
            &self,
            series_id: &str,
            _user_id: &str,
        ) -> Result<(), String> {
            self.refreshed_series
                .lock()
                .unwrap()
                .push(series_id.to_string());
            Ok(())
        }

        async fn delete_series_read_progress(
            &self,
            series_id: &str,
            _user_id: &str,
        ) -> Result<(), String> {
            self.deleted_series
                .lock()
                .unwrap()
                .push(series_id.to_string());
            Ok(())
        }
    }

    #[tokio::test]
    async fn tachiyomi_series_progress_marks_matching_uncompleted_books_and_refreshes_series() {
        let reader = Arc::new(TestReadProgressSurface {
            series_book_numbers: HashMap::from([(
                "series-1".to_string(),
                vec![
                    SeriesBookNumberSort {
                        book_id: "book-1".to_string(),
                        number_sort: 1.0,
                    },
                    SeriesBookNumberSort {
                        book_id: "book-2".to_string(),
                        number_sort: 2.0,
                    },
                    SeriesBookNumberSort {
                        book_id: "book-3".to_string(),
                        number_sort: 3.0,
                    },
                ],
            )]),
            completed_by_book: HashMap::from([
                ("book-1".to_string(), Some(false)),
                ("book-2".to_string(), Some(true)),
                ("book-3".to_string(), Some(false)),
            ]),
            page_count_by_book: HashMap::from([
                ("book-1".to_string(), Some(12)),
                ("book-2".to_string(), Some(24)),
                ("book-3".to_string(), Some(36)),
            ]),
        });
        let writer = Arc::new(TestProgressWriter::default());
        let service = ReadProgressService::new(reader, writer.clone());

        service
            .mark_series_tachiyomi_progress("series-1", "user-1", 2.0)
            .await
            .expect("series tachiyomi progress should persist");

        assert_eq!(
            writer.persisted(),
            vec![PersistedProgressWrite {
                book_id: "book-1".to_string(),
                page: 12,
                completed: true,
            }]
        );
        assert_eq!(writer.refreshed_series(), vec!["series-1".to_string()]);
    }

    #[tokio::test]
    async fn tachiyomi_readlist_progress_marks_visible_prefix_without_persistence_protocol_logic() {
        let reader = Arc::new(TestReadProgressSurface {
            completed_by_book: HashMap::from([
                ("book-1".to_string(), Some(false)),
                ("book-2".to_string(), Some(true)),
                ("book-3".to_string(), Some(false)),
            ]),
            page_count_by_book: HashMap::from([
                ("book-1".to_string(), Some(10)),
                ("book-2".to_string(), Some(11)),
                ("book-3".to_string(), Some(12)),
            ]),
            ..Default::default()
        });
        let writer = Arc::new(TestProgressWriter::default());
        let service = ReadProgressService::new(reader, writer.clone());

        service
            .mark_readlist_tachiyomi_progress(
                &[
                    "book-1".to_string(),
                    "book-2".to_string(),
                    "book-3".to_string(),
                ],
                "user-1",
                2,
            )
            .await
            .expect("readlist tachiyomi progress should persist");

        assert_eq!(
            writer.persisted(),
            vec![PersistedProgressWrite {
                book_id: "book-1".to_string(),
                page: 10,
                completed: true,
            }]
        );
    }

    #[tokio::test]
    async fn tachiyomi_readlist_counters_are_computed_by_application_from_completion_states() {
        let reader = Arc::new(TestReadProgressSurface {
            completed_by_book: HashMap::from([
                ("book-1".to_string(), Some(true)),
                ("book-2".to_string(), Some(true)),
                ("book-3".to_string(), Some(false)),
            ]),
            ..Default::default()
        });
        let writer = Arc::new(TestProgressWriter::default());
        let service = ReadProgressService::new(reader, writer);

        let counters = service
            .readlist_tachiyomi_counters(
                &[
                    "book-1".to_string(),
                    "book-2".to_string(),
                    "book-3".to_string(),
                    "book-4".to_string(),
                ],
                "user-1",
            )
            .await
            .expect("readlist tachiyomi counters should compute");

        assert_eq!(
            counters,
            ReadlistTachiyomiCounters {
                books_count: 4,
                books_read_count: 2,
                books_unread_count: 1,
                books_in_progress_count: 1,
                last_read_continuous_index: 2,
            }
        );
    }

    #[tokio::test]
    async fn tachiyomi_series_progress_is_computed_by_application_from_book_states() {
        let reader = Arc::new(TestReadProgressSurface {
            series_book_numbers: HashMap::from([(
                "series-1".to_string(),
                vec![
                    SeriesBookNumberSort {
                        book_id: "book-1".to_string(),
                        number_sort: 1.0,
                    },
                    SeriesBookNumberSort {
                        book_id: "book-2".to_string(),
                        number_sort: 2.0,
                    },
                    SeriesBookNumberSort {
                        book_id: "book-3".to_string(),
                        number_sort: 3.0,
                    },
                    SeriesBookNumberSort {
                        book_id: "book-4".to_string(),
                        number_sort: 4.0,
                    },
                ],
            )]),
            completed_by_book: HashMap::from([
                ("book-1".to_string(), Some(true)),
                ("book-2".to_string(), Some(true)),
                ("book-3".to_string(), Some(false)),
            ]),
            ..Default::default()
        });
        let writer = Arc::new(TestProgressWriter::default());
        let service = ReadProgressService::new(reader, writer);

        let progress = service
            .series_tachiyomi_progress("series-1", "user-1")
            .await
            .expect("series tachiyomi progress should compute");

        assert_eq!(progress.books_count, 4);
        assert_eq!(progress.books_read_count, 2);
        assert_eq!(progress.books_unread_count, 1);
        assert_eq!(progress.books_in_progress_count, 1);
        assert_eq!(progress.last_read_continuous_number_sort, 2.0);
        assert_eq!(progress.max_number_sort, 4.0);
    }
}
