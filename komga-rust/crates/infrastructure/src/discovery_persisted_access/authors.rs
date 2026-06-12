use std::collections::BTreeSet;

use sqlx::{QueryBuilder, Row, Sqlite, SqlitePool};
use unicode_normalization::{UnicodeNormalization, char::is_combining_mark};

use super::models::{AuthorEntry, AuthorsScope};

pub(super) async fn load_persisted_author_names(
    pool: &SqlitePool,
    search: &str,
    authorized_library_ids: Option<&[String]>,
) -> Result<Vec<String>, String> {
    if let Some(authorized_library_ids) = authorized_library_ids
        && authorized_library_ids.is_empty()
    {
        return Ok(Vec::new());
    }

    let mut query = QueryBuilder::<Sqlite>::new(
        r#"SELECT DISTINCT a.NAME
         FROM BOOK_METADATA_AUTHOR a
         JOIN BOOK b ON b.ID = a.BOOK_ID"#,
    );

    if let Some(authorized_library_ids) = authorized_library_ids {
        query.push(r#" WHERE b.LIBRARY_ID IN ("#);
        let mut separated = query.separated(",");
        for library_id in authorized_library_ids {
            separated.push_bind(library_id);
        }
        separated.push_unseparated(")");
    }

    query.push(r#" ORDER BY lower(a.NAME), a.NAME"#);

    let rows = query
        .build()
        .fetch_all(pool)
        .await
        .map_err(|error| format!("query persisted author names: {error}"))?;

    let search = author_search_key(search);
    Ok(rows
        .into_iter()
        .map(|row| row.get::<String, _>("NAME"))
        .filter(|name| search.is_empty() || author_search_key(name).contains(&search))
        .collect())
}

pub(super) async fn load_persisted_author_roles(
    pool: &SqlitePool,
    authorized_library_ids: Option<&[String]>,
) -> Result<Vec<String>, String> {
    if let Some(authorized_library_ids) = authorized_library_ids
        && authorized_library_ids.is_empty()
    {
        return Ok(Vec::new());
    }

    let mut query = QueryBuilder::<Sqlite>::new(
        r#"SELECT DISTINCT a.ROLE
         FROM BOOK_METADATA_AUTHOR a
         JOIN BOOK b ON b.ID = a.BOOK_ID"#,
    );

    if let Some(authorized_library_ids) = authorized_library_ids {
        query.push(r#" WHERE b.LIBRARY_ID IN ("#);
        let mut separated = query.separated(",");
        for library_id in authorized_library_ids {
            separated.push_bind(library_id);
        }
        separated.push_unseparated(")");
    }

    query.push(r#" ORDER BY lower(a.ROLE), a.ROLE"#);

    let rows = query
        .build()
        .fetch_all(pool)
        .await
        .map_err(|error| format!("query persisted author roles: {error}"))?;

    Ok(rows
        .into_iter()
        .map(|row| row.get::<String, _>("ROLE"))
        .collect())
}

fn author_search_key(value: &str) -> String {
    value
        .nfd()
        .filter(|ch| !is_combining_mark(*ch))
        .flat_map(|ch| ch.to_lowercase())
        .collect()
}

pub(super) async fn load_persisted_authors_by_scope(
    pool: &SqlitePool,
    scope: &AuthorsScope,
    authorized_library_ids: Option<&[String]>,
) -> Result<Vec<AuthorEntry>, String> {
    let mut query = QueryBuilder::<Sqlite>::new(
        r#"SELECT a.NAME, a.ROLE
         FROM BOOK_METADATA_AUTHOR a
         JOIN BOOK b ON b.ID = a.BOOK_ID"#,
    );

    let mut has_where = false;
    let mut push_condition = |query: &mut QueryBuilder<Sqlite>| {
        if has_where {
            query.push(r#" AND "#);
        } else {
            query.push(r#" WHERE "#);
            has_where = true;
        }
    };

    match scope {
        AuthorsScope::All => {}
        AuthorsScope::Libraries(library_ids) => {
            if library_ids.is_empty() {
                return Ok(Vec::new());
            }
            push_condition(&mut query);
            query.push(r#"b.LIBRARY_ID IN ("#);
            let mut separated = query.separated(",");
            for library_id in library_ids {
                separated.push_bind(library_id);
            }
            separated.push_unseparated(")");
        }
        AuthorsScope::Collection(collection_id) => {
            query.push(r#" JOIN COLLECTION_SERIES cs ON cs.SERIES_ID = b.SERIES_ID"#);
            push_condition(&mut query);
            query.push(r#"cs.COLLECTION_ID = "#);
            query.push_bind(collection_id);
        }
        AuthorsScope::Series(series_id) => {
            push_condition(&mut query);
            query.push(r#"b.SERIES_ID = "#);
            query.push_bind(series_id);
        }
        AuthorsScope::ReadList(readlist_id) => {
            query.push(r#" JOIN READLIST_BOOK rb ON rb.BOOK_ID = b.ID"#);
            push_condition(&mut query);
            query.push(r#"rb.READLIST_ID = "#);
            query.push_bind(readlist_id);
        }
    }

    if let Some(authorized_library_ids) = authorized_library_ids {
        if authorized_library_ids.is_empty() {
            return Ok(Vec::new());
        }
        push_condition(&mut query);
        query.push("b.LIBRARY_ID IN (");
        let mut separated = query.separated(",");
        for library_id in authorized_library_ids {
            separated.push_bind(library_id);
        }
        separated.push_unseparated(")");
    }

    query.push(r#" ORDER BY lower(a.NAME), lower(a.ROLE), a.NAME, a.ROLE, b.ID"#);

    let rows = query
        .build()
        .fetch_all(pool)
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
