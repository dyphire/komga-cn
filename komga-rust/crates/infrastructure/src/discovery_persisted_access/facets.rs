use super::*;

pub async fn load_persisted_genres(
    database_file: &FsPath,
    library_id: Option<&str>,
) -> Result<Vec<String>, String> {
    common::load_persisted_library_strings(
        database_file,
        library_id,
        "genres",
        "SELECT g.GENRE AS VALUE \
         FROM SERIES_METADATA_GENRE g \
         JOIN SERIES s ON s.ID = g.SERIES_ID \
         WHERE s.LIBRARY_ID = ? \
         ORDER BY lower(g.GENRE), g.GENRE, s.ID",
        "SELECT g.GENRE AS VALUE \
         FROM SERIES_METADATA_GENRE g \
         JOIN SERIES s ON s.ID = g.SERIES_ID \
         ORDER BY lower(g.GENRE), g.GENRE, s.ID",
    )
    .await
}

pub async fn load_persisted_tags(
    database_file: &FsPath,
    library_id: Option<&str>,
) -> Result<Vec<String>, String> {
    let pool = connect_pool(database_file, 1)
        .await
        .map_err(|error| format!("open tags db: {error}"))?;

    let rows = if let Some(library_id) = library_id {
        sqlx::query(
            "SELECT TAG \
             FROM ( SELECT st.TAG AS TAG \
             FROM SERIES_METADATA_TAG st \
             JOIN SERIES s ON s.ID = st.SERIES_ID \
             WHERE s.LIBRARY_ID = ? \
             UNION SELECT bt.TAG AS TAG \
             FROM BOOK_METADATA_TAG bt \
             JOIN BOOK b ON b.ID = bt.BOOK_ID \
             WHERE b.LIBRARY_ID = ? ) \
             ORDER BY lower(TAG), TAG",
        )
        .bind(library_id)
        .bind(library_id)
        .fetch_all(&pool)
        .await
    } else {
        sqlx::query(
            "SELECT TAG \
             FROM ( SELECT st.TAG AS TAG \
             FROM SERIES_METADATA_TAG st \
             JOIN SERIES s ON s.ID = st.SERIES_ID \
             UNION SELECT bt.TAG AS TAG \
             FROM BOOK_METADATA_TAG bt \
             JOIN BOOK b ON b.ID = bt.BOOK_ID ) \
             ORDER BY lower(TAG), TAG",
        )
        .fetch_all(&pool)
        .await
    }
    .map_err(|error| format!("query persisted tags: {error}"))?;

    Ok(rows
        .into_iter()
        .map(|row| row.get::<String, _>("TAG"))
        .collect())
}

pub async fn load_persisted_languages(
    database_file: &FsPath,
    library_id: Option<&str>,
) -> Result<Vec<String>, String> {
    common::load_persisted_library_strings(
        database_file,
        library_id,
        "languages",
        "SELECT DISTINCT sm.LANGUAGE AS VALUE \
         FROM SERIES_METADATA sm \
         JOIN SERIES s ON s.ID = sm.SERIES_ID \
         WHERE s.LIBRARY_ID = ? \
         ORDER BY lower(sm.LANGUAGE), sm.LANGUAGE",
        "SELECT DISTINCT sm.LANGUAGE AS VALUE \
         FROM SERIES_METADATA sm \
         JOIN SERIES s ON s.ID = sm.SERIES_ID \
         ORDER BY lower(sm.LANGUAGE), sm.LANGUAGE",
    )
    .await
}

pub async fn load_persisted_publishers(
    database_file: &FsPath,
    library_id: Option<&str>,
) -> Result<Vec<String>, String> {
    common::load_persisted_library_strings(
        database_file,
        library_id,
        "publishers",
        "SELECT DISTINCT sm.PUBLISHER AS VALUE \
         FROM SERIES_METADATA sm \
         JOIN SERIES s ON s.ID = sm.SERIES_ID \
         WHERE s.LIBRARY_ID = ? \
         ORDER BY lower(sm.PUBLISHER), sm.PUBLISHER",
        "SELECT DISTINCT sm.PUBLISHER AS VALUE \
         FROM SERIES_METADATA sm \
         JOIN SERIES s ON s.ID = sm.SERIES_ID \
         ORDER BY lower(sm.PUBLISHER), sm.PUBLISHER",
    )
    .await
}

pub async fn load_persisted_age_ratings(
    database_file: &FsPath,
    library_id: Option<&str>,
) -> Result<Vec<u16>, String> {
    let pool = connect_pool(database_file, 1)
        .await
        .map_err(|error| format!("open age-ratings db: {error}"))?;

    let rows = if let Some(library_id) = library_id {
        sqlx::query(
            "SELECT DISTINCT sm.AGE_RATING AS VALUE \
             FROM SERIES_METADATA sm \
             JOIN SERIES s ON s.ID = sm.SERIES_ID \
             WHERE s.LIBRARY_ID = ? \
             AND sm.AGE_RATING IS NOT NULL \
             ORDER BY sm.AGE_RATING",
        )
        .bind(library_id)
        .fetch_all(&pool)
        .await
    } else {
        sqlx::query(
            "SELECT DISTINCT sm.AGE_RATING AS VALUE \
             FROM SERIES_METADATA sm \
             JOIN SERIES s ON s.ID = sm.SERIES_ID \
             WHERE sm.AGE_RATING IS NOT NULL \
             ORDER BY sm.AGE_RATING",
        )
        .fetch_all(&pool)
        .await
    }
    .map_err(|error| format!("query persisted age-ratings: {error}"))?;

    Ok(rows
        .into_iter()
        .filter_map(|row| row.get::<Option<i64>, _>("VALUE"))
        .map(|value| value.max(0) as u16)
        .collect())
}

pub async fn load_persisted_sharing_labels(
    database_file: &FsPath,
    library_id: Option<&str>,
) -> Result<Vec<String>, String> {
    common::load_persisted_library_strings(
        database_file,
        library_id,
        "sharing-labels",
        "SELECT DISTINCT sms.LABEL AS VALUE \
         FROM SERIES_METADATA_SHARING sms \
         JOIN SERIES s ON s.ID = sms.SERIES_ID \
         WHERE s.LIBRARY_ID = ? \
         ORDER BY lower(sms.LABEL), sms.LABEL",
        "SELECT DISTINCT sms.LABEL AS VALUE \
         FROM SERIES_METADATA_SHARING sms \
         JOIN SERIES s ON s.ID = sms.SERIES_ID \
         ORDER BY lower(sms.LABEL), sms.LABEL",
    )
    .await
}

pub async fn load_persisted_series_release_dates(
    database_file: &FsPath,
    library_id: Option<&str>,
) -> Result<Vec<String>, String> {
    common::load_persisted_library_strings(
        database_file,
        library_id,
        "series-release-dates",
        "SELECT DISTINCT bm.RELEASE_DATE AS VALUE \
         FROM BOOK_METADATA bm \
         JOIN BOOK b ON b.ID = bm.BOOK_ID \
         WHERE b.LIBRARY_ID = ? \
         AND bm.RELEASE_DATE IS NOT NULL \
         AND bm.RELEASE_DATE <> '' \
         ORDER BY bm.RELEASE_DATE",
        "SELECT DISTINCT bm.RELEASE_DATE AS VALUE \
         FROM BOOK_METADATA bm \
         JOIN BOOK b ON b.ID = bm.BOOK_ID \
         WHERE bm.RELEASE_DATE IS NOT NULL \
         AND bm.RELEASE_DATE <> '' \
         ORDER BY bm.RELEASE_DATE",
    )
    .await
}

pub async fn load_persisted_series_tags(
    database_file: &FsPath,
    library_id: Option<&str>,
    collection_id: Option<&str>,
) -> Result<Vec<String>, String> {
    let pool = connect_pool(database_file, 1)
        .await
        .map_err(|error| format!("open series tags db: {error}"))?;

    let rows = if let Some(library_id) = library_id {
        sqlx::query(
            "SELECT DISTINCT st.TAG \
             FROM SERIES_METADATA_TAG st \
             JOIN SERIES s ON s.ID = st.SERIES_ID \
             WHERE s.LIBRARY_ID = ? \
             ORDER BY lower(st.TAG), st.TAG",
        )
        .bind(library_id)
        .fetch_all(&pool)
        .await
    } else if let Some(collection_id) = collection_id {
        sqlx::query(
            "SELECT DISTINCT st.TAG \
             FROM SERIES_METADATA_TAG st \
             JOIN COLLECTION_SERIES cs ON cs.SERIES_ID = st.SERIES_ID \
             WHERE cs.COLLECTION_ID = ? \
             ORDER BY lower(st.TAG), st.TAG",
        )
        .bind(collection_id)
        .fetch_all(&pool)
        .await
    } else {
        sqlx::query(
            "SELECT DISTINCT TAG \
             FROM SERIES_METADATA_TAG \
             ORDER BY lower(TAG), TAG",
        )
        .fetch_all(&pool)
        .await
    }
    .map_err(|error| format!("query persisted series tags: {error}"))?;

    Ok(rows
        .into_iter()
        .map(|row| row.get::<String, _>("TAG"))
        .collect())
}
