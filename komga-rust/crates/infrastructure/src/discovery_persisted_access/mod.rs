use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::Path as FsPath;

use crate::sqlite::connect_pool;
use sqlx::{QueryBuilder, Row, Sqlite};

pub mod authors;
pub mod books;
mod common;
pub mod facets;
pub mod library_mappings;
pub mod models;
pub mod runtime_queries;
pub mod series;

use models::{
    AuthorEntry, AuthorsScope, BookBrowseEntry, BookPosterSummary, BookSummary, BookTagsScope,
    SeriesSummary,
};
