use super::*;

pub async fn load_persisted_genres(
    database_file: &FsPath,
    library_ids: Option<&[String]>,
    collection_id: Option<&str>,
) -> Result<Vec<String>, String> {
    common::load_persisted_scoped_strings(
        database_file,
        library_ids,
        collection_id,
        "genres",
        "SELECT DISTINCT g.GENRE AS VALUE FROM SERIES_METADATA_GENRE g JOIN SERIES s ON s.ID = g.SERIES_ID",
        " JOIN COLLECTION_SERIES cs ON cs.SERIES_ID = s.ID",
        "s.LIBRARY_ID",
        None,
        "lower(g.GENRE), g.GENRE, s.ID",
    )
    .await
}

pub async fn load_persisted_tags(
    database_file: &FsPath,
    library_ids: Option<&[String]>,
    collection_id: Option<&str>,
) -> Result<Vec<String>, String> {
    if let Some(library_ids) = library_ids
        && library_ids.is_empty()
    {
        return Ok(Vec::new());
    }

    let pool = connect_pool(database_file, 1)
        .await
        .map_err(|error| format!("open tags db: {error}"))?;

    let rows = if let Some(collection_id) = collection_id {
        let mut query = QueryBuilder::<Sqlite>::new(
            "SELECT TAG \
              FROM ( SELECT st.TAG AS TAG \
              FROM SERIES_METADATA_TAG st \
              JOIN COLLECTION_SERIES cs ON cs.SERIES_ID = st.SERIES_ID \
              JOIN SERIES s ON s.ID = st.SERIES_ID \
              WHERE cs.COLLECTION_ID = ",
        );
        query.push_bind(collection_id);
        if let Some(library_ids) = library_ids.filter(|ids| !ids.is_empty()) {
            query.push(" AND s.LIBRARY_ID IN (");
            let mut separated = query.separated(",");
            for library_id in library_ids {
                separated.push_bind(library_id);
            }
            separated.push_unseparated(")");
        }
        query.push(
            " UNION SELECT bt.TAG AS TAG \
              FROM BOOK_METADATA_TAG bt \
              JOIN BOOK b ON b.ID = bt.BOOK_ID \
              JOIN COLLECTION_SERIES cs ON cs.SERIES_ID = b.SERIES_ID \
              WHERE cs.COLLECTION_ID = ",
        );
        query.push_bind(collection_id);
        if let Some(library_ids) = library_ids.filter(|ids| !ids.is_empty()) {
            query.push(" AND b.LIBRARY_ID IN (");
            let mut separated = query.separated(",");
            for library_id in library_ids {
                separated.push_bind(library_id);
            }
            separated.push_unseparated(")");
        }
        query.push(" ) ORDER BY lower(TAG), TAG");
        query.build().fetch_all(&pool).await
    } else if let Some(library_ids) = library_ids.filter(|ids| !ids.is_empty()) {
        let mut query = QueryBuilder::<Sqlite>::new(
            "SELECT TAG FROM ( \
             SELECT st.TAG AS TAG \
             FROM SERIES_METADATA_TAG st \
             JOIN SERIES s ON s.ID = st.SERIES_ID \
             WHERE s.LIBRARY_ID IN (",
        );
        let mut separated = query.separated(",");
        for library_id in library_ids {
            separated.push_bind(library_id);
        }
        separated.push_unseparated(
            ") \
             UNION SELECT bt.TAG AS TAG \
             FROM BOOK_METADATA_TAG bt \
             JOIN BOOK b ON b.ID = bt.BOOK_ID \
             WHERE b.LIBRARY_ID IN (",
        );
        let mut separated = query.separated(",");
        for library_id in library_ids {
            separated.push_bind(library_id);
        }
        separated.push_unseparated(") ) ORDER BY lower(TAG), TAG");
        query.build().fetch_all(&pool).await
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
    library_ids: Option<&[String]>,
    collection_id: Option<&str>,
) -> Result<Vec<String>, String> {
    common::load_persisted_scoped_strings(
        database_file,
        library_ids,
        collection_id,
        "languages",
        "SELECT DISTINCT sm.LANGUAGE AS VALUE FROM SERIES_METADATA sm JOIN SERIES s ON s.ID = sm.SERIES_ID",
        " JOIN COLLECTION_SERIES cs ON cs.SERIES_ID = s.ID",
        "s.LIBRARY_ID",
        Some("sm.LANGUAGE <> ''"),
        "lower(sm.LANGUAGE), sm.LANGUAGE",
    )
    .await
}

pub async fn load_persisted_publishers(
    database_file: &FsPath,
    library_ids: Option<&[String]>,
    collection_id: Option<&str>,
) -> Result<Vec<String>, String> {
    common::load_persisted_scoped_strings(
        database_file,
        library_ids,
        collection_id,
        "publishers",
        "SELECT DISTINCT sm.PUBLISHER AS VALUE FROM SERIES_METADATA sm JOIN SERIES s ON s.ID = sm.SERIES_ID",
        " JOIN COLLECTION_SERIES cs ON cs.SERIES_ID = s.ID",
        "s.LIBRARY_ID",
        Some("sm.PUBLISHER <> ''"),
        "lower(sm.PUBLISHER), sm.PUBLISHER",
    )
    .await
}

pub async fn load_persisted_age_ratings(
    database_file: &FsPath,
    library_ids: Option<&[String]>,
    collection_id: Option<&str>,
) -> Result<Vec<String>, String> {
    if let Some(library_ids) = library_ids
        && library_ids.is_empty()
    {
        return Ok(Vec::new());
    }

    let pool = connect_pool(database_file, 1)
        .await
        .map_err(|error| format!("open age-ratings db: {error}"))?;

    let mut query = QueryBuilder::<Sqlite>::new(
        "SELECT DISTINCT sm.AGE_RATING AS VALUE FROM SERIES_METADATA sm JOIN SERIES s ON s.ID = sm.SERIES_ID",
    );
    let mut has_where = false;
    if let Some(collection_id) = collection_id {
        query.push(" JOIN COLLECTION_SERIES cs ON cs.SERIES_ID = s.ID WHERE cs.COLLECTION_ID = ");
        query.push_bind(collection_id);
        has_where = true;
    }
    if let Some(library_ids) = library_ids.filter(|ids| !ids.is_empty()) {
        query.push(if has_where { " AND " } else { " WHERE " });
        query.push("s.LIBRARY_ID IN (");
        let mut separated = query.separated(",");
        for library_id in library_ids {
            separated.push_bind(library_id);
        }
        separated.push_unseparated(")");
    }
    query.push(" ORDER BY sm.AGE_RATING");

    let rows = query
        .build()
        .fetch_all(&pool)
        .await
        .map_err(|error| format!("query persisted age-ratings: {error}"))?;

    Ok(rows
        .into_iter()
        .map(|row| match row.get::<Option<i64>, _>("VALUE") {
            Some(value) => value.max(0).to_string(),
            None => "None".to_string(),
        })
        .collect())
}

pub async fn load_persisted_sharing_labels(
    database_file: &FsPath,
    library_ids: Option<&[String]>,
    collection_id: Option<&str>,
) -> Result<Vec<String>, String> {
    common::load_persisted_scoped_strings(
        database_file,
        library_ids,
        collection_id,
        "sharing-labels",
        "SELECT DISTINCT sms.LABEL AS VALUE FROM SERIES_METADATA_SHARING sms JOIN SERIES s ON s.ID = sms.SERIES_ID",
        " JOIN COLLECTION_SERIES cs ON cs.SERIES_ID = s.ID",
        "s.LIBRARY_ID",
        None,
        "lower(sms.LABEL), sms.LABEL",
    )
    .await
}

pub async fn load_persisted_series_release_dates(
    database_file: &FsPath,
    library_ids: Option<&[String]>,
    collection_id: Option<&str>,
) -> Result<Vec<String>, String> {
    let values = common::load_persisted_scoped_strings(
        database_file,
        library_ids,
        collection_id,
        "series-release-dates",
        "SELECT DISTINCT bma.RELEASE_DATE AS VALUE FROM BOOK_METADATA_AGGREGATION bma JOIN SERIES s ON s.ID = bma.SERIES_ID",
        " JOIN COLLECTION_SERIES cs ON cs.SERIES_ID = s.ID",
        "s.LIBRARY_ID",
        Some("bma.RELEASE_DATE IS NOT NULL AND bma.RELEASE_DATE <> ''"),
        "bma.RELEASE_DATE DESC",
    )
    .await?;

    let mut years = Vec::new();
    for value in values {
        let year = value
            .split('-')
            .next()
            .unwrap_or(value.as_str())
            .to_string();
        if !years.contains(&year) {
            years.push(year);
        }
    }

    Ok(years)
}

pub async fn load_persisted_series_tags(
    database_file: &FsPath,
    library_ids: Option<&[String]>,
    collection_id: Option<&str>,
) -> Result<Vec<String>, String> {
    common::load_persisted_scoped_strings(
        database_file,
        library_ids,
        collection_id,
        "series tags",
        "SELECT DISTINCT st.TAG AS VALUE FROM SERIES_METADATA_TAG st JOIN SERIES s ON s.ID = st.SERIES_ID",
        " JOIN COLLECTION_SERIES cs ON cs.SERIES_ID = st.SERIES_ID",
        "s.LIBRARY_ID",
        None,
        "lower(st.TAG), st.TAG",
    )
    .await
}
