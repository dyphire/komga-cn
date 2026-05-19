use komga_application::identity_access::{AuthUser, now_sync_marker};
use sqlx::{Row, Sqlite};

use super::access_control::{normalized_sharing_labels, user_can_access_sync_book};

#[derive(Clone)]
struct SyncPointBookSeedRow {
    book_id: String,
    created_date: String,
    last_modified_date: String,
    file_last_modified: String,
    file_size: i64,
    file_hash: Option<String>,
    metadata_last_modified_date: String,
    read_progress_last_modified_date: Option<String>,
    thumbnail_id: Option<String>,
    library_id: String,
    age_rating: Option<u16>,
    sharing_labels: Vec<String>,
}

#[derive(Clone)]
struct OnDeckSeedRow {
    book_id: String,
    library_id: String,
    age_rating: Option<u16>,
    sharing_labels: Vec<String>,
    most_recent_read_date: Option<String>,
}

pub(super) async fn seed_sync_point_books(
    tx: &mut sqlx::Transaction<'_, Sqlite>,
    sync_point_id: &str,
    user: &AuthUser,
) -> Result<(), sqlx::Error> {
    let rows = sqlx::query(
        r#"
        SELECT
            b.ID AS BOOK_ID,
            COALESCE(b.CREATED_DATE, CURRENT_TIMESTAMP) AS BOOK_CREATED_DATE,
            COALESCE(b.LAST_MODIFIED_DATE, b.CREATED_DATE, CURRENT_TIMESTAMP) AS BOOK_LAST_MODIFIED_DATE,
            CASE
                WHEN typeof(b.FILE_LAST_MODIFIED) = 'integer'
                    THEN datetime(b.FILE_LAST_MODIFIED, 'unixepoch')
                ELSE b.FILE_LAST_MODIFIED
            END AS BOOK_FILE_LAST_MODIFIED,
            COALESCE(b.FILE_SIZE, 0) AS BOOK_FILE_SIZE,
            b.FILE_HASH AS BOOK_FILE_HASH,
            COALESCE(
                bm.LAST_MODIFIED_DATE,
                bm.CREATED_DATE,
                b.LAST_MODIFIED_DATE,
                b.CREATED_DATE,
                CURRENT_TIMESTAMP
            ) AS BOOK_METADATA_LAST_MODIFIED_DATE,
            rp.LAST_MODIFIED_DATE AS BOOK_READ_PROGRESS_LAST_MODIFIED_DATE,
            tb.ID AS BOOK_THUMBNAIL_ID,
            b.LIBRARY_ID AS LIBRARY_ID,
            sm.AGE_RATING AS AGE_RATING,
            COALESCE(
                (
                    SELECT GROUP_CONCAT(DISTINCT sms.LABEL)
                    FROM SERIES_METADATA_SHARING sms
                    WHERE sms.SERIES_ID = b.SERIES_ID
                ),
                ''
            ) AS SHARING_LABELS
        FROM BOOK b
        JOIN MEDIA m ON m.BOOK_ID = b.ID
        JOIN BOOK_METADATA bm ON bm.BOOK_ID = b.ID
        JOIN SERIES_METADATA sm ON sm.SERIES_ID = b.SERIES_ID
        LEFT JOIN READ_PROGRESS rp ON rp.BOOK_ID = b.ID AND rp.USER_ID = ?
        LEFT JOIN THUMBNAIL_BOOK tb ON tb.BOOK_ID = b.ID AND tb.SELECTED = TRUE
        WHERE b.DELETED_DATE IS NULL
          AND m.STATUS = 'READY'
          AND m.MEDIA_TYPE = 'application/epub+zip'
        "#,
    )
    .bind(user.id.as_str())
    .fetch_all(&mut **tx)
    .await?;

    let books = rows
        .into_iter()
        .map(|row| SyncPointBookSeedRow {
            book_id: row.get::<String, _>("BOOK_ID"),
            created_date: row.get::<String, _>("BOOK_CREATED_DATE"),
            last_modified_date: row.get::<String, _>("BOOK_LAST_MODIFIED_DATE"),
            file_last_modified: row.get::<String, _>("BOOK_FILE_LAST_MODIFIED"),
            file_size: row.get::<i64, _>("BOOK_FILE_SIZE"),
            file_hash: row.get::<Option<String>, _>("BOOK_FILE_HASH"),
            metadata_last_modified_date: row.get::<String, _>("BOOK_METADATA_LAST_MODIFIED_DATE"),
            read_progress_last_modified_date: row
                .get::<Option<String>, _>("BOOK_READ_PROGRESS_LAST_MODIFIED_DATE"),
            thumbnail_id: row.get::<Option<String>, _>("BOOK_THUMBNAIL_ID"),
            library_id: row.get::<String, _>("LIBRARY_ID"),
            age_rating: row
                .get::<Option<i64>, _>("AGE_RATING")
                .and_then(|value| u16::try_from(value).ok()),
            sharing_labels: normalized_sharing_labels(
                row.get::<String, _>("SHARING_LABELS").as_str(),
            ),
        })
        .filter(|row| {
            user_can_access_sync_book(user, &row.library_id, row.age_rating, &row.sharing_labels)
        })
        .collect::<Vec<_>>();

    if books.is_empty() {
        return Ok(());
    }

    let mut query = sqlx::QueryBuilder::<Sqlite>::new(
        r#"
        INSERT INTO SYNC_POINT_BOOK (
            SYNC_POINT_ID,
            BOOK_ID,
            BOOK_CREATED_DATE,
            BOOK_LAST_MODIFIED_DATE,
            BOOK_FILE_LAST_MODIFIED,
            BOOK_FILE_SIZE,
            BOOK_FILE_HASH,
            BOOK_METADATA_LAST_MODIFIED_DATE,
            BOOK_READ_PROGRESS_LAST_MODIFIED_DATE,
            BOOK_THUMBNAIL_ID
        )
        "#,
    );
    query.push_values(books.iter(), |mut builder, book| {
        builder
            .push_bind(sync_point_id)
            .push_bind(book.book_id.as_str())
            .push_bind(book.created_date.as_str())
            .push_bind(book.last_modified_date.as_str())
            .push_bind(book.file_last_modified.as_str())
            .push_bind(book.file_size)
            .push_bind(book.file_hash.as_deref())
            .push_bind(book.metadata_last_modified_date.as_str())
            .push_bind(book.read_progress_last_modified_date.as_deref())
            .push_bind(book.thumbnail_id.as_deref());
    });
    query.build().execute(&mut **tx).await?;

    Ok(())
}

pub(super) async fn seed_sync_point_ondeck(
    tx: &mut sqlx::Transaction<'_, Sqlite>,
    sync_point_id: &str,
    user: &AuthUser,
) -> Result<(), sqlx::Error> {
    let rows = sqlx::query(
        r#"
        SELECT
            b.ID AS BOOK_ID,
            b.LIBRARY_ID AS LIBRARY_ID,
            sm.AGE_RATING AS AGE_RATING,
            COALESCE(
                (
                    SELECT GROUP_CONCAT(DISTINCT sms.LABEL)
                    FROM SERIES_METADATA_SHARING sms
                    WHERE sms.SERIES_ID = b.SERIES_ID
                ),
                ''
            ) AS SHARING_LABELS,
            rps.MOST_RECENT_READ_DATE AS MOST_RECENT_READ_DATE
        FROM READ_PROGRESS_SERIES rps
        JOIN SERIES s ON s.ID = rps.SERIES_ID
        JOIN BOOK b ON b.SERIES_ID = s.ID
        JOIN MEDIA m ON m.BOOK_ID = b.ID
        JOIN BOOK_METADATA bm ON bm.BOOK_ID = b.ID
        JOIN SERIES_METADATA sm ON sm.SERIES_ID = s.ID
        LEFT JOIN READ_PROGRESS rp ON rp.BOOK_ID = b.ID AND rp.USER_ID = rps.USER_ID
        WHERE rps.USER_ID = ?
          AND b.DELETED_DATE IS NULL
          AND m.STATUS = 'READY'
          AND m.MEDIA_TYPE = 'application/epub+zip'
          AND rps.IN_PROGRESS_COUNT = 0
          AND rps.READ_COUNT != s.BOOK_COUNT
          AND rp.COMPLETED IS NULL
          AND NOT EXISTS (
              SELECT 1
              FROM BOOK b_prev
              JOIN BOOK_METADATA bm_prev ON bm_prev.BOOK_ID = b_prev.ID
              LEFT JOIN READ_PROGRESS rp_prev ON rp_prev.BOOK_ID = b_prev.ID
                                           AND rp_prev.USER_ID = rps.USER_ID
              WHERE b_prev.SERIES_ID = b.SERIES_ID
                AND rp_prev.COMPLETED IS NULL
                AND (
                    COALESCE(bm_prev.NUMBER_SORT, 0) < COALESCE(bm.NUMBER_SORT, 0)
                    OR (
                        COALESCE(bm_prev.NUMBER_SORT, 0) = COALESCE(bm.NUMBER_SORT, 0)
                        AND b_prev.ID < b.ID
                    )
                )
          )
        "#,
    )
    .bind(user.id.as_str())
    .fetch_all(&mut **tx)
    .await?;

    let items = rows
        .into_iter()
        .map(|row| OnDeckSeedRow {
            book_id: row.get::<String, _>("BOOK_ID"),
            library_id: row.get::<String, _>("LIBRARY_ID"),
            age_rating: row
                .get::<Option<i64>, _>("AGE_RATING")
                .and_then(|value| u16::try_from(value).ok()),
            sharing_labels: normalized_sharing_labels(
                row.get::<String, _>("SHARING_LABELS").as_str(),
            ),
            most_recent_read_date: row.get::<Option<String>, _>("MOST_RECENT_READ_DATE"),
        })
        .filter(|row| {
            user_can_access_sync_book(user, &row.library_id, row.age_rating, &row.sharing_labels)
        })
        .collect::<Vec<_>>();

    if items.is_empty() {
        return Ok(());
    }

    let created_date = now_sync_marker();
    let last_modified = items
        .iter()
        .filter_map(|item| item.most_recent_read_date.as_deref())
        .max()
        .unwrap_or(created_date.as_str())
        .to_string();

    sqlx::query(
        r#"
        INSERT INTO SYNC_POINT_READLIST (
            SYNC_POINT_ID,
            READLIST_ID,
            READLIST_NAME,
            READLIST_CREATED_DATE,
            READLIST_LAST_MODIFIED_DATE
        ) VALUES (?, ?, ?, ?, ?)
        "#,
    )
    .bind(sync_point_id)
    .bind("KOMGA-ONDECK")
    .bind("On Deck")
    .bind(created_date)
    .bind(last_modified)
    .execute(&mut **tx)
    .await?;

    let mut query = sqlx::QueryBuilder::<Sqlite>::new(
        "INSERT INTO SYNC_POINT_READLIST_BOOK (SYNC_POINT_ID, READLIST_ID, BOOK_ID) ",
    );
    query.push_values(items.iter(), |mut builder, item| {
        builder
            .push_bind(sync_point_id)
            .push_bind("KOMGA-ONDECK")
            .push_bind(item.book_id.as_str());
    });
    query.build().execute(&mut **tx).await?;

    Ok(())
}
