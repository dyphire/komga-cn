use std::collections::{BTreeMap, BTreeSet, HashMap};

use sqlx::{QueryBuilder, Row, Sqlite, SqlitePool};

pub mod authors;
pub mod books;
pub mod browse;
mod common;
pub mod facets;
pub mod library_mappings;
pub mod models;
pub mod runtime_queries;
pub mod search;
pub mod series;

use models::{
    AuthorEntry, AuthorsScope, BookBrowseEntry, BookPosterSummary, BookSummary, BookTagsScope,
    SeriesSummary,
};
