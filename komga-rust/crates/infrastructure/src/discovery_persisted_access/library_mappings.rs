use anyhow::Context;
use std::collections::{BTreeMap, BTreeSet, HashMap};

use sqlx::{Row, SqlitePool};

pub(super) async fn load_persisted_library_ids(pool: &SqlitePool) -> anyhow::Result<Vec<String>> {
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
    .fetch_all(pool)
    .await
    .context("query persisted browse-library ids")?;

    Ok(rows
        .into_iter()
        .map(|row| row.get::<String, _>("ID"))
        .collect())
}

pub(super) async fn load_collection_memberships(
    pool: &SqlitePool,
) -> anyhow::Result<BTreeMap<String, BTreeSet<String>>> {
    let rows = sqlx::query(
        r#"SELECT SERIES_ID, COLLECTION_ID
         FROM COLLECTION_SERIES"#,
    )
    .fetch_all(pool)
    .await
    .context("query series collection memberships")?;

    let mut memberships = BTreeMap::<String, BTreeSet<String>>::new();
    for row in rows {
        memberships
            .entry(row.get::<String, _>("SERIES_ID"))
            .or_default()
            .insert(row.get::<String, _>("COLLECTION_ID"));
    }
    Ok(memberships)
}

pub(super) async fn load_collection_ordering(
    pool: &SqlitePool,
    collection_id: &str,
) -> anyhow::Result<HashMap<String, i64>> {
    let rows = sqlx::query(
        r#"SELECT SERIES_ID, NUMBER
         FROM COLLECTION_SERIES
         WHERE COLLECTION_ID = ?"#,
    )
    .bind(collection_id)
    .fetch_all(pool)
    .await
    .context("query collection ordering")?;

    let mut ordering = HashMap::new();
    for row in rows {
        ordering.insert(
            row.get::<String, _>("SERIES_ID"),
            row.get::<i64, _>("NUMBER"),
        );
    }

    Ok(ordering)
}

pub(super) async fn load_readlist_memberships(
    pool: &SqlitePool,
) -> anyhow::Result<BTreeMap<String, BTreeSet<String>>> {
    let rows = sqlx::query(
        r#"SELECT BOOK_ID, READLIST_ID
         FROM READLIST_BOOK"#,
    )
    .fetch_all(pool)
    .await
    .context("query readlist memberships")?;

    let mut memberships = BTreeMap::<String, BTreeSet<String>>::new();
    for row in rows {
        memberships
            .entry(row.get::<String, _>("BOOK_ID"))
            .or_default()
            .insert(row.get::<String, _>("READLIST_ID"));
    }
    Ok(memberships)
}

pub(super) async fn load_readlist_ordering(
    pool: &SqlitePool,
    readlist_id: &str,
) -> anyhow::Result<HashMap<String, i64>> {
    let rows = sqlx::query(
        r#"SELECT BOOK_ID, NUMBER
         FROM READLIST_BOOK
         WHERE READLIST_ID = ?"#,
    )
    .bind(readlist_id)
    .fetch_all(pool)
    .await
    .context("query readlist ordering")?;

    let mut ordering = HashMap::new();
    for row in rows {
        ordering.insert(row.get::<String, _>("BOOK_ID"), row.get::<i64, _>("NUMBER"));
    }

    Ok(ordering)
}
