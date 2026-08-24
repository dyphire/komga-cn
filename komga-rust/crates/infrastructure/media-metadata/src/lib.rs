mod book_metadata;
mod codecs;
mod comicinfo;
mod read_progress;
mod refresh;
mod thumbnail_writer;
mod thumbnails;

pub use book_metadata::SqliteBookMetadataPort;
pub use codecs::parse_thumbnail_type;
pub use comicinfo::{
    ComicInfoDocument, load_comicinfo_bytes_for_media, load_comicinfo_bytes_from_path,
    parse_comicinfo_xml,
};
pub use read_progress::{
    delete_persisted_read_progress, load_book_page_count, load_book_progression,
    load_book_read_progress_completed, persist_book_progression, persist_read_progress,
    read_progress_completed_by_book_ids,
};
pub use refresh::generate_book_thumbnail;
pub use refresh::{
    TransientMetadataProviderInference, aggregate_series_metadata,
    infer_transient_comicinfo_provider_metadata, infer_transient_epub_provider_metadata,
    refresh_book_local_artwork, refresh_book_metadata, refresh_series_local_artwork,
    refresh_series_metadata,
};
pub use thumbnail_writer::ThumbnailWriter;
pub use thumbnails::{
    delete_book_thumbnail, delete_collection_thumbnail, delete_readlist_thumbnail,
    delete_series_thumbnail, insert_book_thumbnail, insert_collection_thumbnail,
    insert_readlist_thumbnail, insert_series_thumbnail, load_book_thumbnail_by_id,
    load_collection_thumbnail_by_id, load_persisted_book_thumbnails,
    load_persisted_collection_thumbnails, load_persisted_readlist_name,
    load_persisted_readlist_thumbnails, load_persisted_series_thumbnails,
    load_readlist_thumbnail_by_id, load_selected_book_thumbnail, load_selected_series_thumbnail,
    load_series_thumbnail_by_id, persisted_collection_exists, persisted_readlist_exists,
    select_book_thumbnail, select_collection_thumbnail, select_readlist_thumbnail,
    select_series_thumbnail,
};
