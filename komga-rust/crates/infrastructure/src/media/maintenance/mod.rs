mod archive;
mod conversion;
mod hashed_pages;
mod hashing;
mod library_flags;
pub(crate) mod page_hashing;
pub(crate) mod persistence;
pub(crate) mod updates;

pub(crate) use conversion::{convert_book, find_books_to_convert, repair_extension};
pub(crate) use hashed_pages::{HashedPageToDelete, remove_hashed_pages};
pub(crate) use hashing::{find_duplicate_pages_to_delete, hash_book, hash_book_pages};
pub(crate) use library_flags::load_library_hashing_flags;
