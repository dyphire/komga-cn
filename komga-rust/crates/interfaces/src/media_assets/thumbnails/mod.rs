mod books;
mod collections;
mod readlists;
mod series;
pub(crate) mod shared;

pub(crate) use books::{
    book_thumbnail, book_thumbnail_by_id, book_thumbnail_delete, book_thumbnail_select,
    book_thumbnail_upload, book_thumbnails,
};
pub(crate) use collections::{
    collection_thumbnail, collection_thumbnail_by_id, collection_thumbnail_delete,
    collection_thumbnail_select, collection_thumbnail_upload, collection_thumbnails,
};
pub(crate) use readlists::{
    readlist_thumbnail, readlist_thumbnail_by_id, readlist_thumbnail_delete,
    readlist_thumbnail_select, readlist_thumbnail_upload, readlist_thumbnails,
};
pub(crate) use series::{
    series_thumbnail, series_thumbnail_by_id, series_thumbnail_delete, series_thumbnail_select,
    series_thumbnail_upload, series_thumbnails,
};
