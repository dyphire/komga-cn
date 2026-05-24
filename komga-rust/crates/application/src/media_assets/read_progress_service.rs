use std::sync::Arc;

use super::{ProgressWriterPort, ReadProgressSurfacePort};

pub struct ReadProgressService {
    reader: Arc<dyn ReadProgressSurfacePort>,
    writer: Arc<dyn ProgressWriterPort>,
}

impl ReadProgressService {
    pub fn new(
        reader: Arc<dyn ReadProgressSurfacePort>,
        writer: Arc<dyn ProgressWriterPort>,
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
            .filter(|(_, number_sort)| *number_sort <= last_number_sort_read)
            .map(|(book_id, _)| book_id)
            .collect::<Vec<_>>();

        self.mark_books_complete(book_ids, user_id).await?;
        self.writer
            .refresh_series_read_progress(series_id, user_id)
            .await
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
                .persist_read_progress(&book_id, user_id, page_count, true, None)
                .await?;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::Mutex;

    use async_trait::async_trait;
    use serde_json::Value;

    use super::*;

    #[derive(Default)]
    struct TestReadProgressSurface {
        series_book_numbers: HashMap<String, Vec<(String, f64)>>,
        completed_by_book: HashMap<String, Option<bool>>,
        page_count_by_book: HashMap<String, Option<u64>>,
    }

    #[async_trait]
    impl ReadProgressSurfacePort for TestReadProgressSurface {
        async fn series_book_ids(&self, series_id: &str) -> Result<Vec<String>, String> {
            Ok(self
                .series_book_numbers
                .get(series_id)
                .cloned()
                .unwrap_or_default()
                .into_iter()
                .map(|(book_id, _)| book_id)
                .collect())
        }

        async fn series_book_number_sorts(
            &self,
            series_id: &str,
        ) -> Result<Vec<(String, f64)>, String> {
            Ok(self
                .series_book_numbers
                .get(series_id)
                .cloned()
                .unwrap_or_default())
        }

        async fn book_read_progress_completed(
            &self,
            book_id: &str,
            _user_id: &str,
        ) -> Result<Option<bool>, String> {
            Ok(self.completed_by_book.get(book_id).copied().flatten())
        }

        async fn book_page_count(&self, book_id: &str) -> Result<Option<u64>, String> {
            Ok(self.page_count_by_book.get(book_id).copied().flatten())
        }
    }

    #[derive(Default)]
    struct TestProgressWriter {
        persisted: Mutex<Vec<(String, u64, bool)>>,
        deleted_books: Mutex<Vec<String>>,
        refreshed_series: Mutex<Vec<String>>,
        deleted_series: Mutex<Vec<String>>,
    }

    impl TestProgressWriter {
        fn persisted(&self) -> Vec<(String, u64, bool)> {
            self.persisted.lock().unwrap().clone()
        }

        fn refreshed_series(&self) -> Vec<String> {
            self.refreshed_series.lock().unwrap().clone()
        }
    }

    #[async_trait]
    impl ProgressWriterPort for TestProgressWriter {
        async fn persist_read_progress(
            &self,
            book_id: &str,
            _user_id: &str,
            page: u64,
            completed: bool,
            _locator: Option<Value>,
        ) -> Result<(), String> {
            self.persisted
                .lock()
                .unwrap()
                .push((book_id.to_string(), page, completed));
            Ok(())
        }

        async fn persist_book_progression(
            &self,
            _book_id: &str,
            _user_id: &str,
            _progression: f64,
            _use_locator_position_for_page: bool,
            _modified: Option<String>,
            _device_id: Option<String>,
            _device_name: Option<String>,
            _locator: Option<Value>,
        ) -> Result<(), String> {
            unreachable!("book progression writes are not part of series read-progress service")
        }

        async fn delete_read_progress(&self, book_id: &str, _user_id: &str) -> Result<(), String> {
            self.deleted_books.lock().unwrap().push(book_id.to_string());
            Ok(())
        }

        async fn persist_readlist_tachiyomi_progress(
            &self,
            _ordered_book_ids: &[String],
            _user_id: &str,
            _last_book_read: usize,
        ) -> Result<Option<()>, String> {
            unreachable!("readlist writes are not part of series read-progress service")
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
                    ("book-1".to_string(), 1.0),
                    ("book-2".to_string(), 2.0),
                    ("book-3".to_string(), 3.0),
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

        assert_eq!(writer.persisted(), vec![("book-1".to_string(), 12, true)]);
        assert_eq!(writer.refreshed_series(), vec!["series-1".to_string()]);
    }
}
