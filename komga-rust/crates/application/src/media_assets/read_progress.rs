use std::future::Future;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BookReadProgressMutation {
    pub page: u64,
    pub completed: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SeriesTachiyomiProgress {
    pub books_count: usize,
    pub books_read_count: usize,
    pub books_unread_count: usize,
    pub books_in_progress_count: usize,
    pub last_read_continuous_index: usize,
}

pub trait ReadProgressPort {
    fn persisted_book_exists(&self, book_id: &str) -> impl Future<Output = Result<bool, String>>;

    fn load_book_page_count(
        &self,
        book_id: &str,
    ) -> impl Future<Output = Result<Option<u64>, String>>;

    fn persist_read_progress(
        &self,
        book_id: &str,
        user_id: &str,
        mutation: BookReadProgressMutation,
    ) -> impl Future<Output = Result<(), String>>;

    fn delete_read_progress(
        &self,
        book_id: &str,
        user_id: &str,
    ) -> impl Future<Output = Result<(), String>>;

    fn persist_book_progression(
        &self,
        book_id: &str,
        user_id: &str,
        progression: f64,
    ) -> impl Future<Output = Result<(), String>>;

    fn load_book_progression(
        &self,
        book_id: &str,
        user_id: &str,
    ) -> impl Future<Output = Result<Option<f64>, String>>;
}

pub trait BookProgressionPort: ReadProgressPort {}

impl<T> BookProgressionPort for T where T: ReadProgressPort {}

pub struct ReadProgressService<P> {
    port: P,
}

impl<P> ReadProgressService<P>
where
    P: ReadProgressPort,
{
    pub fn new(port: P) -> Self {
        Self { port }
    }

    pub async fn mutate_book_progress(
        &self,
        book_id: &str,
        user_id: &str,
        mutation: BookReadProgressMutation,
    ) -> Result<bool, String> {
        if !self.port.persisted_book_exists(book_id).await? {
            return Ok(false);
        }

        self.port
            .persist_read_progress(book_id, user_id, mutation)
            .await?;
        Ok(true)
    }

    pub async fn delete_book_progress(&self, book_id: &str, user_id: &str) -> Result<bool, String> {
        if !self.port.persisted_book_exists(book_id).await? {
            return Ok(false);
        }

        self.port.delete_read_progress(book_id, user_id).await?;
        Ok(true)
    }

    pub async fn persist_book_progression(
        &self,
        book_id: &str,
        user_id: &str,
        progression: f64,
    ) -> Result<bool, String> {
        if !self.port.persisted_book_exists(book_id).await? {
            return Ok(false);
        }

        self.port
            .persist_book_progression(book_id, user_id, progression)
            .await?;
        Ok(true)
    }

    pub async fn load_book_progression(
        &self,
        book_id: &str,
        user_id: &str,
    ) -> Result<Option<Option<f64>>, String> {
        if !self.port.persisted_book_exists(book_id).await? {
            return Ok(None);
        }

        self.port
            .load_book_progression(book_id, user_id)
            .await
            .map(Some)
    }

    pub async fn resolved_page_count(&self, book_id: &str) -> Result<Option<u64>, String> {
        if !self.port.persisted_book_exists(book_id).await? {
            return Ok(None);
        }

        self.port
            .load_book_page_count(book_id)
            .await
            .map(|count| Some(count.unwrap_or(1).max(1)))
    }
}
