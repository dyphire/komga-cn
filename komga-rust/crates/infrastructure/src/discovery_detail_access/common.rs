use sqlx::SqlitePool;

pub(super) use crate::parsing::clamp_kotlin_int_u32;

pub(super) async fn table_has_rows(
    pool: &SqlitePool,
    table: &str,
    label: &str,
) -> Result<bool, String> {
    let sql = format!("SELECT 1 AS FOUND FROM {table} LIMIT 1");
    let row = sqlx::query(sqlx::AssertSqlSafe(sql))
        .fetch_optional(pool)
        .await
        .map_err(|error| format!("query {label} existence: {error}"))?;
    Ok(row.is_some())
}

pub(super) async fn replace_ordered_children(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    child_table: &str,
    parent_id_column: &str,
    child_id_column: &str,
    parent_id: &str,
    child_ids: &[String],
) -> Result<(), sqlx::Error> {
    sqlx::query(sqlx::AssertSqlSafe(format!(
        "DELETE FROM {child_table} WHERE {parent_id_column} = ?"
    )))
    .bind(parent_id)
    .execute(&mut **tx)
    .await?;

    for (index, child_id) in child_ids.iter().enumerate() {
        sqlx::query(sqlx::AssertSqlSafe(format!(
            "INSERT INTO {child_table} ({parent_id_column}, {child_id_column}, NUMBER) VALUES (?, ?, ?)"
        )))
        .bind(parent_id)
        .bind(child_id)
        .bind(index as i64)
        .execute(&mut **tx)
        .await?;
    }

    Ok(())
}

pub(super) async fn delete_parent_with_children(
    pool: &SqlitePool,
    thumbnail_table: &str,
    child_table: &str,
    parent_table: &str,
    parent_id_column: &str,
    parent_id: &str,
    label: &str,
) -> Result<bool, String> {
    let mut tx = pool
        .begin()
        .await
        .map_err(|error| format!("begin {label} delete tx: {error}"))?;

    sqlx::query(sqlx::AssertSqlSafe(format!(
        "DELETE FROM {thumbnail_table} WHERE {parent_id_column} = ?"
    )))
    .bind(parent_id)
    .execute(&mut *tx)
    .await
    .map_err(|error| format!("delete persisted {label} thumbnails: {error}"))?;

    sqlx::query(sqlx::AssertSqlSafe(format!(
        "DELETE FROM {child_table} WHERE {parent_id_column} = ?"
    )))
    .bind(parent_id)
    .execute(&mut *tx)
    .await
    .map_err(|error| format!("delete persisted {label} children: {error}"))?;

    let deleted = sqlx::query(sqlx::AssertSqlSafe(format!(
        "DELETE FROM {parent_table} WHERE ID = ?"
    )))
    .bind(parent_id)
    .execute(&mut *tx)
    .await
    .map_err(|error| format!("delete persisted {label}: {error}"))?
    .rows_affected()
        > 0;

    if !deleted {
        tx.rollback()
            .await
            .map_err(|error| format!("rollback {label} delete tx: {error}"))?;
        return Ok(false);
    }

    tx.commit()
        .await
        .map_err(|error| format!("commit {label} delete tx: {error}"))?;
    Ok(true)
}
