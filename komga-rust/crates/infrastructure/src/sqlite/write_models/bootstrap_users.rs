use std::path::Path;

use sqlx::Row;

use crate::sqlite::connect_pool;

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
    let pool = connect_pool(database_file, 1).await?;
    let rows = sqlx::query("SELECT EMAIL\n         FROM USER\n         ORDER BY EMAIL")
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
    let pool = connect_pool(database_file, 1).await?;
    let row = sqlx::query(
        "SELECT ID, EMAIL\n         FROM USER\n         WHERE LOWER(EMAIL) = LOWER(?)\n         LIMIT 1",
    )
    .bind(email)
    .fetch_optional(&pool)
    .await?;

    Ok(row.map(|row| PersistedBootstrapUser {
        id: row.get::<String, _>("ID"),
        email: row.get::<String, _>("EMAIL"),
    }))
}

pub async fn update_persisted_user_password(
    database_file: &Path,
    user_id: &str,
    hashed_password: &str,
) -> Result<bool, sqlx::Error> {
    let pool = connect_pool(database_file, 1).await?;
    let rows_affected =
        sqlx::query("UPDATE USER\n         SET PASSWORD = ?\n         WHERE ID = ?")
            .bind(hashed_password)
            .bind(user_id)
            .execute(&pool)
            .await?
            .rows_affected();

    Ok(rows_affected > 0)
}

pub async fn persist_initial_bootstrap_users(
    database_file: &Path,
    users: &[InitialBootstrapUserWriteModel],
) -> Result<(), sqlx::Error> {
    let pool = connect_pool(database_file, 1).await?;
    let mut tx = pool.begin().await?;

    for user in users {
        sqlx::query(
            "INSERT INTO USER (ID, EMAIL, PASSWORD, SHARED_ALL_LIBRARIES, AGE_RESTRICTION, AGE_RESTRICTION_ALLOW_ONLY)\n             VALUES (?, ?, ?, ?, ?, ?)",
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
            sqlx::query("INSERT INTO USER_ROLE (USER_ID, ROLE) VALUES (?, ?)")
                .bind(&user.id)
                .bind(role)
                .execute(&mut *tx)
                .await?;
        }
    }

    tx.commit().await?;
    Ok(())
}
