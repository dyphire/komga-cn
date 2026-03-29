use super::*;

pub async fn load_persisted_authors(
    database_file: &FsPath,
    library_id: Option<&str>,
) -> Result<Vec<AuthorEntry>, String> {
    let pool = connect_pool(database_file, 1)
        .await
        .map_err(|error| format!("open authors db: {error}"))?;

    let rows = if let Some(library_id) = library_id {
        sqlx::query(
            "SELECT a.NAME, a.ROLE \
             FROM BOOK_METADATA_AUTHOR a \
             JOIN BOOK b ON b.ID = a.BOOK_ID \
             WHERE b.LIBRARY_ID = ? \
             ORDER BY lower(a.NAME), lower(a.ROLE), a.NAME, a.ROLE, b.ID",
        )
        .bind(library_id)
        .fetch_all(&pool)
        .await
    } else {
        sqlx::query(
            "SELECT a.NAME, a.ROLE \
             FROM BOOK_METADATA_AUTHOR a \
             JOIN BOOK b ON b.ID = a.BOOK_ID \
             ORDER BY lower(a.NAME), lower(a.ROLE), a.NAME, a.ROLE, b.ID",
        )
        .fetch_all(&pool)
        .await
    }
    .map_err(|error| format!("query persisted authors: {error}"))?;

    let mut authors = Vec::with_capacity(rows.len());
    let mut seen = BTreeSet::new();
    for row in rows {
        let name = row.get::<String, _>("NAME");
        let role = row.get::<String, _>("ROLE");
        if seen.insert((name.clone(), role.clone())) {
            authors.push(AuthorEntry { name, role });
        }
    }

    Ok(authors)
}

pub async fn load_persisted_author_names(
    database_file: &FsPath,
    search: &str,
) -> Result<Vec<String>, String> {
    let pool = connect_pool(database_file, 1)
        .await
        .map_err(|error| format!("open author names db: {error}"))?;

    let rows = sqlx::query(
        "SELECT DISTINCT a.NAME \
         FROM BOOK_METADATA_AUTHOR a \
         JOIN BOOK b ON b.ID = a.BOOK_ID \
         ORDER BY lower(a.NAME), a.NAME",
    )
    .fetch_all(&pool)
    .await
    .map_err(|error| format!("query persisted author names: {error}"))?;

    let search = search.to_ascii_lowercase();
    Ok(rows
        .into_iter()
        .map(|row| row.get::<String, _>("NAME"))
        .filter(|name| search.is_empty() || name.to_ascii_lowercase().contains(&search))
        .collect())
}

pub async fn load_persisted_author_roles(database_file: &FsPath) -> Result<Vec<String>, String> {
    let pool = connect_pool(database_file, 1)
        .await
        .map_err(|error| format!("open author roles db: {error}"))?;

    let rows = sqlx::query(
        "SELECT DISTINCT ROLE \
         FROM BOOK_METADATA_AUTHOR \
         ORDER BY lower(ROLE), ROLE",
    )
    .fetch_all(&pool)
    .await
    .map_err(|error| format!("query persisted author roles: {error}"))?;

    Ok(rows
        .into_iter()
        .map(|row| row.get::<String, _>("ROLE"))
        .collect())
}

pub async fn load_persisted_authors_by_scope(
    database_file: &FsPath,
    scope: &AuthorsScope,
) -> Result<Vec<AuthorEntry>, String> {
    let pool = connect_pool(database_file, 1)
        .await
        .map_err(|error| format!("open v2 authors db: {error}"))?;

    let mut query = QueryBuilder::<Sqlite>::new(
        "SELECT a.NAME, a.ROLE \
         FROM BOOK_METADATA_AUTHOR a \
         JOIN BOOK b ON b.ID = a.BOOK_ID",
    );

    match scope {
        AuthorsScope::All => {}
        AuthorsScope::Libraries(library_ids) => {
            query.push(" WHERE b.LIBRARY_ID IN (");
            let mut separated = query.separated(",");
            for library_id in library_ids {
                separated.push_bind(library_id);
            }
            separated.push_unseparated(")");
        }
        AuthorsScope::Collection(collection_id) => {
            query.push(" JOIN COLLECTION_SERIES cs ON cs.SERIES_ID = b.SERIES_ID WHERE cs.COLLECTION_ID = ");
            query.push_bind(collection_id);
        }
        AuthorsScope::Series(series_id) => {
            query.push(" WHERE b.SERIES_ID = ");
            query.push_bind(series_id);
        }
        AuthorsScope::ReadList(readlist_id) => {
            query.push(" JOIN READLIST_BOOK rb ON rb.BOOK_ID = b.ID WHERE rb.READLIST_ID = ");
            query.push_bind(readlist_id);
        }
    }

    query.push(" ORDER BY lower(a.NAME), lower(a.ROLE), a.NAME, a.ROLE, b.ID");

    let rows = query
        .build()
        .fetch_all(&pool)
        .await
        .map_err(|error| format!("query persisted v2 authors: {error}"))?;

    let mut authors = Vec::with_capacity(rows.len());
    let mut seen = BTreeSet::new();
    for row in rows {
        let name = row.get::<String, _>("NAME");
        let role = row.get::<String, _>("ROLE");
        if seen.insert((name.clone(), role.clone())) {
            authors.push(AuthorEntry { name, role });
        }
    }

    Ok(authors)
}
