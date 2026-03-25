use std::collections::HashMap;

use komga_domain::discovery::{DiscoveryError, DiscoveryQueryContext};
use sqlx::{QueryBuilder, Sqlite, SqlitePool};

use super::super::filters::{
    SqlValue, SqlxWhereState, append_clause_sqlx, append_in_clause_sqlx, effective_library_ids,
};
use super::map_sqlx_error;
use crate::read_models::libraries::PersistedLibraryReadModel;

#[derive(sqlx::FromRow)]
struct SqlxPersistedLibraryRow {
    id: String,
    name: String,
    root: String,
    import_comicinfo_book: bool,
    import_comicinfo_series: bool,
    import_comicinfo_collection: bool,
    import_comicinfo_readlist: bool,
    import_comicinfo_series_append_volume: bool,
    import_epub_book: bool,
    import_epub_series: bool,
    import_mylar_series: bool,
    import_local_artwork: bool,
    import_barcode_isbn: bool,
    scan_force_modified_time: bool,
    scan_interval: String,
    scan_startup: bool,
    scan_cbx: bool,
    scan_pdf: bool,
    scan_epub: bool,
    repair_extensions: bool,
    convert_to_cbz: bool,
    empty_trash_after_scan: bool,
    series_cover: String,
    hash_files: bool,
    hash_pages: bool,
    hash_koreader: bool,
    analyze_dimensions: bool,
    oneshots_directory: Option<String>,
    unavailable_date: Option<String>,
}

impl From<SqlxPersistedLibraryRow> for PersistedLibraryReadModel {
    fn from(value: SqlxPersistedLibraryRow) -> Self {
        Self {
            id: value.id,
            name: value.name,
            root: value.root,
            import_comicinfo_book: value.import_comicinfo_book,
            import_comicinfo_series: value.import_comicinfo_series,
            import_comicinfo_collection: value.import_comicinfo_collection,
            import_comicinfo_readlist: value.import_comicinfo_readlist,
            import_comicinfo_series_append_volume: value.import_comicinfo_series_append_volume,
            import_epub_book: value.import_epub_book,
            import_epub_series: value.import_epub_series,
            import_mylar_series: value.import_mylar_series,
            import_local_artwork: value.import_local_artwork,
            import_barcode_isbn: value.import_barcode_isbn,
            scan_force_modified_time: value.scan_force_modified_time,
            scan_interval: value.scan_interval,
            scan_on_startup: value.scan_startup,
            scan_cbx: value.scan_cbx,
            scan_pdf: value.scan_pdf,
            scan_epub: value.scan_epub,
            scan_directory_exclusions: vec![],
            repair_extensions: value.repair_extensions,
            convert_to_cbz: value.convert_to_cbz,
            empty_trash_after_scan: value.empty_trash_after_scan,
            series_cover: value.series_cover,
            hash_files: value.hash_files,
            hash_pages: value.hash_pages,
            hash_koreader: value.hash_koreader,
            analyze_dimensions: value.analyze_dimensions,
            oneshots_directory: value.oneshots_directory,
            unavailable: value.unavailable_date.is_some(),
        }
    }
}

#[derive(sqlx::FromRow)]
struct SqlxLibraryExclusionRow {
    library_id: String,
    exclusion: String,
}

pub(in crate::read_models) async fn list_persisted_libraries_sqlx(
    pool: SqlitePool,
    context: &DiscoveryQueryContext,
) -> Result<Vec<PersistedLibraryReadModel>, DiscoveryError> {
    let allowed = effective_library_ids(context, None);
    if allowed.as_ref().is_some_and(Vec::is_empty) {
        return Ok(vec![]);
    }

    let mut builder = QueryBuilder::<Sqlite>::new(
        "SELECT \
            ID AS id, \
            NAME AS name, \
            ROOT AS root, \
            IMPORT_COMICINFO_BOOK AS import_comicinfo_book, \
            IMPORT_COMICINFO_SERIES AS import_comicinfo_series, \
            IMPORT_COMICINFO_COLLECTION AS import_comicinfo_collection, \
            IMPORT_COMICINFO_READLIST AS import_comicinfo_readlist, \
            IMPORT_COMICINFO_SERIES_APPEND_VOLUME AS import_comicinfo_series_append_volume, \
            IMPORT_EPUB_BOOK AS import_epub_book, \
            IMPORT_EPUB_SERIES AS import_epub_series, \
            IMPORT_MYLAR_SERIES AS import_mylar_series, \
            IMPORT_LOCAL_ARTWORK AS import_local_artwork, \
            IMPORT_BARCODE_ISBN AS import_barcode_isbn, \
            SCAN_FORCE_MODIFIED_TIME AS scan_force_modified_time, \
            SCAN_INTERVAL AS scan_interval, \
            SCAN_STARTUP AS scan_startup, \
            SCAN_CBX AS scan_cbx, \
            SCAN_PDF AS scan_pdf, \
            SCAN_EPUB AS scan_epub, \
            REPAIR_EXTENSIONS AS repair_extensions, \
            CONVERT_TO_CBZ AS convert_to_cbz, \
            EMPTY_TRASH_AFTER_SCAN AS empty_trash_after_scan, \
            SERIES_COVER AS series_cover, \
            HASH_FILES AS hash_files, \
            HASH_PAGES AS hash_pages, \
            HASH_KOREADER AS hash_koreader, \
            ANALYZE_DIMENSIONS AS analyze_dimensions, \
            ONESHOTS_DIRECTORY AS oneshots_directory, \
            UNAVAILABLE_DATE AS unavailable_date \
         FROM LIBRARY",
    );
    let mut state = SqlxWhereState::default();
    if let Some(allowed_ids) = allowed.as_ref() {
        append_in_clause_sqlx("ID", allowed_ids, &mut builder, &mut state);
    }
    builder.push(" ORDER BY NAME COLLATE NOCASE ASC, ID ASC");

    let rows = builder
        .build_query_as::<SqlxPersistedLibraryRow>()
        .fetch_all(&pool)
        .await
        .map_err(map_sqlx_error)?;
    let mut libraries = rows
        .into_iter()
        .map(PersistedLibraryReadModel::from)
        .collect::<Vec<_>>();

    attach_library_exclusions(&pool, &mut libraries).await?;

    Ok(libraries)
}

pub(in crate::read_models) async fn get_persisted_library_sqlx(
    pool: SqlitePool,
    context: &DiscoveryQueryContext,
    library_id: &str,
) -> Result<Option<PersistedLibraryReadModel>, DiscoveryError> {
    let allowed = effective_library_ids(context, None);
    if allowed.as_ref().is_some_and(Vec::is_empty) {
        return Ok(None);
    }

    let mut builder = QueryBuilder::<Sqlite>::new(
        "SELECT \
            ID AS id, \
            NAME AS name, \
            ROOT AS root, \
            IMPORT_COMICINFO_BOOK AS import_comicinfo_book, \
            IMPORT_COMICINFO_SERIES AS import_comicinfo_series, \
            IMPORT_COMICINFO_COLLECTION AS import_comicinfo_collection, \
            IMPORT_COMICINFO_READLIST AS import_comicinfo_readlist, \
            IMPORT_COMICINFO_SERIES_APPEND_VOLUME AS import_comicinfo_series_append_volume, \
            IMPORT_EPUB_BOOK AS import_epub_book, \
            IMPORT_EPUB_SERIES AS import_epub_series, \
            IMPORT_MYLAR_SERIES AS import_mylar_series, \
            IMPORT_LOCAL_ARTWORK AS import_local_artwork, \
            IMPORT_BARCODE_ISBN AS import_barcode_isbn, \
            SCAN_FORCE_MODIFIED_TIME AS scan_force_modified_time, \
            SCAN_INTERVAL AS scan_interval, \
            SCAN_STARTUP AS scan_startup, \
            SCAN_CBX AS scan_cbx, \
            SCAN_PDF AS scan_pdf, \
            SCAN_EPUB AS scan_epub, \
            REPAIR_EXTENSIONS AS repair_extensions, \
            CONVERT_TO_CBZ AS convert_to_cbz, \
            EMPTY_TRASH_AFTER_SCAN AS empty_trash_after_scan, \
            SERIES_COVER AS series_cover, \
            HASH_FILES AS hash_files, \
            HASH_PAGES AS hash_pages, \
            HASH_KOREADER AS hash_koreader, \
            ANALYZE_DIMENSIONS AS analyze_dimensions, \
            ONESHOTS_DIRECTORY AS oneshots_directory, \
            UNAVAILABLE_DATE AS unavailable_date \
         FROM LIBRARY",
    );
    let mut state = SqlxWhereState::default();
    append_clause_sqlx("ID = ", &mut builder, &mut state);
    builder.push_bind(library_id);
    state.params.push(SqlValue::Text(library_id.to_string()));
    if let Some(allowed_ids) = allowed.as_ref() {
        append_in_clause_sqlx("ID", allowed_ids, &mut builder, &mut state);
    }

    let maybe_row = builder
        .build_query_as::<SqlxPersistedLibraryRow>()
        .fetch_optional(&pool)
        .await
        .map_err(map_sqlx_error)?;

    let Some(row) = maybe_row else {
        return Ok(None);
    };

    let mut library = PersistedLibraryReadModel::from(row);
    let exclusions = sqlx::query_as::<_, SqlxLibraryExclusionRow>(
        "SELECT LIBRARY_ID AS library_id, EXCLUSION AS exclusion \
         FROM LIBRARY_EXCLUSIONS \
         WHERE LIBRARY_ID = ? \
         ORDER BY EXCLUSION COLLATE NOCASE ASC",
    )
    .bind(library_id)
    .fetch_all(&pool)
    .await
    .map_err(map_sqlx_error)?;
    library.scan_directory_exclusions = exclusions.into_iter().map(|row| row.exclusion).collect();

    Ok(Some(library))
}

async fn attach_library_exclusions(
    pool: &SqlitePool,
    libraries: &mut [PersistedLibraryReadModel],
) -> Result<(), DiscoveryError> {
    if libraries.is_empty() {
        return Ok(());
    }

    let library_ids = libraries
        .iter()
        .map(|library| library.id.clone())
        .collect::<Vec<_>>();
    let library_indexes = libraries
        .iter()
        .enumerate()
        .map(|(index, library)| (library.id.clone(), index))
        .collect::<HashMap<_, _>>();

    let mut builder = QueryBuilder::<Sqlite>::new(
        "SELECT LIBRARY_ID AS library_id, EXCLUSION AS exclusion FROM LIBRARY_EXCLUSIONS",
    );
    let mut state = SqlxWhereState::default();
    append_in_clause_sqlx("LIBRARY_ID", &library_ids, &mut builder, &mut state);
    builder.push(" ORDER BY LIBRARY_ID ASC, EXCLUSION COLLATE NOCASE ASC");

    let exclusions = builder
        .build_query_as::<SqlxLibraryExclusionRow>()
        .fetch_all(pool)
        .await
        .map_err(map_sqlx_error)?;

    for row in exclusions {
        let Some(index) = library_indexes.get(&row.library_id).copied() else {
            continue;
        };
        libraries[index].scan_directory_exclusions.push(row.exclusion);
    }

    Ok(())
}
