use super::*;

pub async fn load_persisted_library_ids(database_file: &FsPath) -> Result<Vec<String>, String> {
    let pool = connect_read_pool(database_file)
        .await
        .map_err(|error| format!("open persisted browse-library db: {error}"))?;

    let rows = sqlx::query(
        r#"SELECT LIBRARY_ID AS ID
         FROM (
             SELECT DISTINCT LIBRARY_ID
             FROM SERIES
             WHERE DELETED_DATE IS NULL
             UNION
             SELECT DISTINCT LIBRARY_ID
             FROM BOOK
             WHERE DELETED_DATE IS NULL
         )
         ORDER BY ID COLLATE NOCASE ASC, ID ASC"#,
    )
    .fetch_all(&pool)
    .await
    .map_err(|error| format!("query persisted browse-library ids: {error}"))?;

    Ok(rows
        .into_iter()
        .map(|row| row.get::<String, _>("ID"))
        .collect())
}

pub async fn load_collection_memberships(
    database_file: &FsPath,
) -> Result<BTreeMap<String, BTreeSet<String>>, String> {
    let pool = connect_read_pool(database_file)
        .await
        .map_err(|error| format!("open series collection db: {error}"))?;

    let rows = sqlx::query(
        r#"SELECT SERIES_ID, COLLECTION_ID
         FROM COLLECTION_SERIES"#,
    )
    .fetch_all(&pool)
    .await
    .map_err(|error| format!("query series collection memberships: {error}"))?;

    let mut memberships = BTreeMap::<String, BTreeSet<String>>::new();
    for row in rows {
        memberships
            .entry(row.get::<String, _>("SERIES_ID"))
            .or_default()
            .insert(row.get::<String, _>("COLLECTION_ID"));
    }
    Ok(memberships)
}

pub async fn load_collection_ordering(
    database_file: &FsPath,
    collection_id: &str,
) -> Result<HashMap<String, i64>, String> {
    let pool = connect_read_pool(database_file)
        .await
        .map_err(|error| format!("open collection ordering db: {error}"))?;

    let rows = sqlx::query(
        r#"SELECT SERIES_ID, NUMBER
         FROM COLLECTION_SERIES
         WHERE COLLECTION_ID = ?"#,
    )
    .bind(collection_id)
    .fetch_all(&pool)
    .await
    .map_err(|error| format!("query collection ordering: {error}"))?;

    let mut ordering = HashMap::new();
    for row in rows {
        ordering.insert(
            row.get::<String, _>("SERIES_ID"),
            row.get::<i64, _>("NUMBER"),
        );
    }

    Ok(ordering)
}

pub async fn load_readlist_memberships(
    database_file: &FsPath,
) -> Result<BTreeMap<String, BTreeSet<String>>, String> {
    let pool = connect_read_pool(database_file)
        .await
        .map_err(|error| format!("open readlist memberships db: {error}"))?;

    let rows = sqlx::query(
        r#"SELECT BOOK_ID, READLIST_ID
         FROM READLIST_BOOK"#,
    )
    .fetch_all(&pool)
    .await
    .map_err(|error| format!("query readlist memberships: {error}"))?;

    let mut memberships = BTreeMap::<String, BTreeSet<String>>::new();
    for row in rows {
        memberships
            .entry(row.get::<String, _>("BOOK_ID"))
            .or_default()
            .insert(row.get::<String, _>("READLIST_ID"));
    }
    Ok(memberships)
}

pub async fn load_readlist_ordering(
    database_file: &FsPath,
    readlist_id: &str,
) -> Result<HashMap<String, i64>, String> {
    let pool = connect_read_pool(database_file)
        .await
        .map_err(|error| format!("open readlist ordering db: {error}"))?;

    let rows = sqlx::query(
        r#"SELECT BOOK_ID, NUMBER
         FROM READLIST_BOOK
         WHERE READLIST_ID = ?"#,
    )
    .bind(readlist_id)
    .fetch_all(&pool)
    .await
    .map_err(|error| format!("query readlist ordering: {error}"))?;

    let mut ordering = HashMap::new();
    for row in rows {
        ordering.insert(row.get::<String, _>("BOOK_ID"), row.get::<i64, _>("NUMBER"));
    }

    Ok(ordering)
}
