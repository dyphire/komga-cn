#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BookDetailQuery {
    pub book_id: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BookSiblingQuery {
    pub book_id: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BookReadlistsQuery {
    pub book_id: String,
}
