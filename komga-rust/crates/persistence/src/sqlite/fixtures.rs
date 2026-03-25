use sqlx::SqlitePool;

use crate::read_models::{
    BookRow, CollectionRow, LibraryRow, ReadListRow, ReadProgressRow, SeriesRow,
};

pub async fn insert_minimal_library(
    pool: &SqlitePool,
    id: &str,
    name: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query("INSERT INTO libraries (id, name) VALUES (?1, ?2)")
        .bind(id)
        .bind(name)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn insert_minimal_series(
    pool: &SqlitePool,
    id: &str,
    library_id: &str,
    title: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query("INSERT INTO series (id, library_id, title) VALUES (?1, ?2, ?3)")
        .bind(id)
        .bind(library_id)
        .bind(title)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn insert_minimal_book(
    pool: &SqlitePool,
    id: &str,
    series_id: &str,
    library_id: &str,
    title: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query("INSERT INTO books (id, series_id, library_id, title) VALUES (?1, ?2, ?3, ?4)")
        .bind(id)
        .bind(series_id)
        .bind(library_id)
        .bind(title)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn series_defaults(
    pool: &SqlitePool,
    series_id: &str,
) -> Result<(String, String, String, String), sqlx::Error> {
    sqlx::query_as::<_, (String, String, String, String)>(
        "SELECT created, last_modified, file_last_modified, url FROM series WHERE id = ?1",
    )
    .bind(series_id)
    .fetch_one(pool)
    .await
}

pub async fn book_defaults(
    pool: &SqlitePool,
    book_id: &str,
) -> Result<(String, String, String, String, i64, String), sqlx::Error> {
    sqlx::query_as::<_, (String, String, String, String, i64, String)>(
        "SELECT created, last_modified, file_last_modified, media_status, number_sort, url FROM books WHERE id = ?1",
    )
    .bind(book_id)
    .fetch_one(pool)
    .await
}

pub async fn count_series_label(
    pool: &SqlitePool,
    series_id: &str,
    label: &str,
) -> Result<i64, sqlx::Error> {
    sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM series_labels WHERE series_id = ?1 AND label = ?2",
    )
    .bind(series_id)
    .bind(label)
    .fetch_one(pool)
    .await
}

pub async fn count_book_tag(
    pool: &SqlitePool,
    book_id: &str,
    tag: &str,
) -> Result<i64, sqlx::Error> {
    sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM book_tags WHERE book_id = ?1 AND tag = ?2")
        .bind(book_id)
        .bind(tag)
        .fetch_one(pool)
        .await
}

pub async fn insert_library(pool: &SqlitePool, row: LibraryRow) -> Result<(), sqlx::Error> {
    sqlx::query("INSERT INTO libraries (id, name, root) VALUES (?1, ?2, ?3)")
        .bind(row.id)
        .bind(row.name)
        .bind(row.root)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn insert_series(pool: &SqlitePool, row: SeriesRow) -> Result<(), sqlx::Error> {
    let SeriesRow {
        id,
        library_id,
        title,
        labels,
        genres,
        tags,
        language,
        publisher,
        age_rating,
        release_date,
        status,
        complete,
        read_status,
        authors,
        deleted,
        oneshot,
        created,
        last_modified,
        file_last_modified,
        url,
    } = row;

    sqlx::query(
        "INSERT INTO series (id, library_id, title, age_rating, language, publisher, release_date, status, complete, read_status, deleted, oneshot, created, last_modified, file_last_modified, url) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16)",
    )
    .bind(&id)
    .bind(library_id)
    .bind(title)
    .bind(age_rating)
    .bind(language)
    .bind(publisher)
    .bind(release_date)
    .bind(status)
    .bind(complete)
    .bind(read_status)
    .bind(deleted)
    .bind(oneshot)
    .bind(created)
    .bind(last_modified)
    .bind(file_last_modified)
    .bind(url)
    .execute(pool)
    .await?;

    for label in labels {
        sqlx::query("INSERT INTO series_labels (series_id, label) VALUES (?1, ?2)")
            .bind(&id)
            .bind(label)
            .execute(pool)
            .await?;
    }

    for genre in genres {
        sqlx::query("INSERT INTO series_genres (series_id, genre) VALUES (?1, ?2)")
            .bind(&id)
            .bind(genre)
            .execute(pool)
            .await?;
    }

    for tag in tags {
        sqlx::query("INSERT INTO series_tags (series_id, tag) VALUES (?1, ?2)")
            .bind(&id)
            .bind(tag)
            .execute(pool)
            .await?;
    }

    for author in authors {
        sqlx::query("INSERT INTO series_authors (series_id, author) VALUES (?1, ?2)")
            .bind(&id)
            .bind(author)
            .execute(pool)
            .await?;
    }

    Ok(())
}

pub async fn insert_collection(pool: &SqlitePool, row: CollectionRow) -> Result<(), sqlx::Error> {
    let CollectionRow {
        id,
        name,
        ordered,
        series_ids,
        created_date,
        last_modified_date,
    } = row;

    sqlx::query(
        "INSERT INTO collections (id, name, ordered, created_date, last_modified_date) VALUES (?1, ?2, ?3, ?4, ?5)",
    )
    .bind(&id)
    .bind(name)
    .bind(ordered)
    .bind(created_date)
    .bind(last_modified_date)
    .execute(pool)
    .await?;

    for (index, series_id) in series_ids.iter().enumerate() {
        sqlx::query(
            "INSERT INTO collection_series (collection_id, series_id, position) VALUES (?1, ?2, ?3)",
        )
        .bind(&id)
        .bind(series_id)
        .bind(index as i64)
        .execute(pool)
        .await?;
    }

    Ok(())
}

pub async fn insert_read_list(pool: &SqlitePool, row: ReadListRow) -> Result<(), sqlx::Error> {
    let ReadListRow {
        id,
        name,
        summary,
        ordered,
        book_ids,
        created_date,
        last_modified_date,
    } = row;

    sqlx::query(
        "INSERT INTO readlists (id, name, summary, ordered, created_date, last_modified_date) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
    )
    .bind(&id)
    .bind(name)
    .bind(summary)
    .bind(ordered)
    .bind(created_date)
    .bind(last_modified_date)
    .execute(pool)
    .await?;

    for (index, book_id) in book_ids.iter().enumerate() {
        sqlx::query(
            "INSERT INTO readlist_books (readlist_id, book_id, position) VALUES (?1, ?2, ?3)",
        )
        .bind(&id)
        .bind(book_id)
        .bind(index as i64)
        .execute(pool)
        .await?;
    }

    Ok(())
}

pub async fn insert_book(pool: &SqlitePool, row: BookRow) -> Result<(), sqlx::Error> {
    let BookRow {
        id,
        series_id,
        library_id,
        title,
        url,
        created,
        last_modified,
        file_last_modified,
        size_bytes,
        media_status,
        media_profile,
        media_type,
        media_pages_count,
        metadata_release_date,
        number_sort,
        deleted,
        oneshot,
        tags,
        read_status,
        authors,
    } = row;

    sqlx::query(
        "INSERT INTO books (id, series_id, library_id, title, url, created, last_modified, file_last_modified, size_bytes, media_status, media_profile, media_type, media_pages_count, metadata_release_date, number_sort, read_status, deleted, oneshot) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18)",
    )
    .bind(&id)
    .bind(series_id)
    .bind(library_id)
    .bind(title)
    .bind(url)
    .bind(created)
    .bind(last_modified)
    .bind(file_last_modified)
    .bind(size_bytes as i64)
    .bind(media_status)
    .bind(media_profile)
    .bind(media_type)
    .bind(media_pages_count as i64)
    .bind(metadata_release_date)
    .bind(number_sort)
    .bind(read_status)
    .bind(deleted)
    .bind(oneshot)
    .execute(pool)
    .await?;

    for tag in tags {
        sqlx::query("INSERT INTO book_tags (book_id, tag) VALUES (?1, ?2)")
            .bind(&id)
            .bind(tag)
            .execute(pool)
            .await?;
    }

    for author in authors {
        sqlx::query("INSERT INTO book_authors (book_id, author) VALUES (?1, ?2)")
            .bind(&id)
            .bind(author)
            .execute(pool)
            .await?;
    }

    Ok(())
}

pub async fn insert_read_progress(
    pool: &SqlitePool,
    row: ReadProgressRow,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO read_progress (book_id, user_id, page, completed, read_date, created, last_modified, device_id, device_name) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
    )
    .bind(row.book_id)
    .bind(row.user_id)
    .bind(row.page)
    .bind(row.completed)
    .bind(row.read_date)
    .bind(row.created)
    .bind(row.last_modified)
    .bind(row.device_id)
    .bind(row.device_name)
    .execute(pool)
    .await?;

    Ok(())
}
