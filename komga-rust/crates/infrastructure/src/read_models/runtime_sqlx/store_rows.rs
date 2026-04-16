use sqlx::SqlitePool;

use super::super::{BookRow, LibraryRow, SeriesRow};

pub(super) async fn insert_library_row(
    pool: &SqlitePool,
    row: LibraryRow,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"INSERT INTO libraries (id, name, root)
VALUES (?1, ?2, ?3)"#,
    )
    .bind(row.id)
    .bind(row.name)
    .bind(row.root)
    .execute(pool)
    .await?;
    Ok(())
}

pub(super) async fn insert_series_row(
    pool: &SqlitePool,
    row: SeriesRow,
) -> Result<(), sqlx::Error> {
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
        r#"INSERT INTO series (id, library_id, title, age_rating, language, publisher,
   release_date, status, complete, read_status, deleted, oneshot, created, last_modified,
   file_last_modified, url)
VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16)"#,
    )
    .bind(&id)
    .bind(&library_id)
    .bind(&title)
    .bind(age_rating)
    .bind(&language)
    .bind(&publisher)
    .bind(release_date)
    .bind(&status)
    .bind(complete)
    .bind(&read_status)
    .bind(deleted)
    .bind(oneshot)
    .bind(&created)
    .bind(&last_modified)
    .bind(&file_last_modified)
    .bind(&url)
    .execute(pool)
    .await?;

    for label in labels {
        sqlx::query(
            r#"INSERT INTO series_labels (series_id, label)
VALUES (?1, ?2)"#,
        )
        .bind(&id)
        .bind(label)
        .execute(pool)
        .await?;
    }

    for genre in genres {
        sqlx::query(
            r#"INSERT INTO series_genres (series_id, genre)
VALUES (?1, ?2)"#,
        )
        .bind(&id)
        .bind(genre)
        .execute(pool)
        .await?;
    }

    for tag in tags {
        sqlx::query(
            r#"INSERT INTO series_tags (series_id, tag)
VALUES (?1, ?2)"#,
        )
        .bind(&id)
        .bind(tag)
        .execute(pool)
        .await?;
    }

    for author in authors {
        sqlx::query(
            r#"INSERT INTO series_authors (series_id, author)
VALUES (?1, ?2)"#,
        )
        .bind(&id)
        .bind(author)
        .execute(pool)
        .await?;
    }

    Ok(())
}

pub(super) async fn insert_book_row(pool: &SqlitePool, row: BookRow) -> Result<(), sqlx::Error> {
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
        r#"INSERT INTO books (id, series_id, library_id, title, url, created, last_modified,
   file_last_modified, size_bytes, media_status, media_profile, media_type,
   media_pages_count, metadata_release_date, number_sort, read_status, deleted, oneshot)
VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18)"#,
    )
    .bind(&id)
    .bind(&series_id)
    .bind(&library_id)
    .bind(&title)
    .bind(&url)
    .bind(&created)
    .bind(&last_modified)
    .bind(&file_last_modified)
    .bind(size_bytes as i64)
    .bind(&media_status)
    .bind(&media_profile)
    .bind(&media_type)
    .bind(media_pages_count as i64)
    .bind(metadata_release_date)
    .bind(number_sort)
    .bind(&read_status)
    .bind(deleted)
    .bind(oneshot)
    .execute(pool)
    .await?;

    for tag in tags {
        sqlx::query(
            r#"INSERT INTO book_tags (book_id, tag)
VALUES (?1, ?2)"#,
        )
        .bind(&id)
        .bind(tag)
        .execute(pool)
        .await?;
    }

    for author in authors {
        sqlx::query(
            r#"INSERT INTO book_authors (book_id, author)
VALUES (?1, ?2)"#,
        )
        .bind(&id)
        .bind(author)
        .execute(pool)
        .await?;
    }

    Ok(())
}
