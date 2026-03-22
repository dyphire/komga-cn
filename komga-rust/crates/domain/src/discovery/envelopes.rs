#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PageEnvelope<T> {
    pub content: Vec<T>,
    pub page: usize,
    pub size: usize,
    pub total_elements: usize,
    pub total_pages: usize,
}

impl<T> PageEnvelope<T> {
    pub fn from_slice(content: Vec<T>, page: usize, size: usize, total_elements: usize) -> Self {
        let safe_size = size.max(1);
        let total_pages = if total_elements == 0 {
            0
        } else {
            ((total_elements - 1) / safe_size) + 1
        };
        Self {
            content,
            page,
            size,
            total_elements,
            total_pages,
        }
    }
}
