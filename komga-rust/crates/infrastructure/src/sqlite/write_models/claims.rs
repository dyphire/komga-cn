use std::path::Path;

use sqlx::Row;

use crate::sqlite::connect_pool;

#[derive(Clone, Debug)]
pub struct CreatedClaimedUser {
    pub id: String,
    pub email: String,
}

pub async fn load_persisted_user_count(database_file: &Path) -> Result<i64, sqlx::Error> {
    let pool = connect_pool(database_file, 1).await?;
    let count = sqlx::query("SELECT COUNT(*) AS COUNT FROM USER")
        .fetch_one(&pool)
        .await?
        .get::<i64, _>("COUNT");
    Ok(count)
}

pub async fn persist_initial_admin_user(
    database_file: &Path,
    user_id: &str,
    email: &str,
    hashed_password: &str,
) -> Result<CreatedClaimedUser, sqlx::Error> {
    let pool = connect_pool(database_file, 1).await?;
    let mut tx = pool.begin().await?;

    sqlx::query(
        "INSERT INTO USER ( \
             ID, \
             EMAIL, \
             PASSWORD, \
             SHARED_ALL_LIBRARIES, \
             AGE_RESTRICTION, \
             AGE_RESTRICTION_ALLOW_ONLY \
         ) \
         VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind(user_id)
    .bind(email)
    .bind(hashed_password)
    .bind(true)
    .bind(None::<i64>)
    .bind(None::<bool>)
    .execute(&mut *tx)
    .await?;

    sqlx::query("INSERT INTO USER_ROLE (USER_ID, ROLE) VALUES (?, ?)")
        .bind(user_id)
        .bind("ADMIN")
        .execute(&mut *tx)
        .await?;

    tx.commit().await?;

    Ok(CreatedClaimedUser {
        id: user_id.to_string(),
        email: email.to_string(),
    })
}
