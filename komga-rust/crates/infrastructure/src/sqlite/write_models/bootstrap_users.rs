use std::path::Path;

use sqlx::Row;

use crate::sqlite::connect_write_pool;

#[derive(Clone, Debug)]
pub struct PersistedBootstrapUser {
    pub id: String,
    pub email: String,
}

#[derive(Clone, Debug)]
pub struct InitialBootstrapUserWriteModel {
    pub id: String,
    pub email: String,
    pub hashed_password: String,
    pub roles: Vec<String>,
}

pub async fn list_persisted_user_emails(database_file: &Path) -> Result<Vec<String>, sqlx::Error> {
    let pool = connect_write_pool(database_file).await?;
    let rows = sqlx::query(
        r#"SELECT EMAIL
           FROM USER
           ORDER BY EMAIL"#,
    )
    .fetch_all(&pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|row| row.get::<String, _>("EMAIL"))
        .collect())
}

pub async fn load_persisted_user_by_email(
    database_file: &Path,
    email: &str,
) -> Result<Option<PersistedBootstrapUser>, sqlx::Error> {
    let pool = connect_write_pool(database_file).await?;
    let row = sqlx::query(
        r#"SELECT ID, EMAIL
           FROM USER
           WHERE LOWER(EMAIL) = LOWER(?)
           LIMIT 1"#,
    )
    .bind(email)
    .fetch_optional(&pool)
    .await?;

    Ok(row.map(|row| PersistedBootstrapUser {
        id: row.get::<String, _>("ID"),
        email: row.get::<String, _>("EMAIL"),
    }))
}

pub async fn update_persisted_user_passwords(
    database_file: &Path,
    updates: &[(String, String)],
) -> Result<(), sqlx::Error> {
    let pool = connect_write_pool(database_file).await?;
    let mut tx = pool.begin().await?;

    for (user_id, hashed_password) in updates {
        let rows_affected = sqlx::query(
            r#"UPDATE USER
               SET PASSWORD = ?
               WHERE ID = ?"#,
        )
        .bind(hashed_password)
        .bind(user_id)
        .execute(&mut *tx)
        .await?
        .rows_affected();

        if rows_affected == 0 {
            return Err(sqlx::Error::RowNotFound);
        }
    }

    tx.commit().await?;
    Ok(())
}

pub async fn persist_initial_bootstrap_users(
    database_file: &Path,
    users: &[InitialBootstrapUserWriteModel],
) -> Result<(), sqlx::Error> {
    let pool = connect_write_pool(database_file).await?;
    let mut tx = pool.begin().await?;

    for user in users {
        sqlx::query(
            r#"INSERT INTO USER (ID, EMAIL, PASSWORD, SHARED_ALL_LIBRARIES, AGE_RESTRICTION, AGE_RESTRICTION_ALLOW_ONLY)
               VALUES (?, ?, ?, ?, ?, ?)"#,
        )
        .bind(&user.id)
        .bind(&user.email)
        .bind(&user.hashed_password)
        .bind(true)
        .bind(None::<i64>)
        .bind(None::<bool>)
        .execute(&mut *tx)
        .await?;

        for role in &user.roles {
            sqlx::query(r#"INSERT INTO USER_ROLE (USER_ID, ROLE) VALUES (?, ?)"#)
                .bind(&user.id)
                .bind(role)
                .execute(&mut *tx)
                .await?;
        }
    }

    tx.commit().await?;
    Ok(())
}
