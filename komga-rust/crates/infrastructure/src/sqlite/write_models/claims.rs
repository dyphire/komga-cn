use sqlx::{Row, SqlitePool};

#[derive(Clone, Debug)]
pub struct CreatedClaimedUser {
    pub id: String,
    pub email: String,
}

pub async fn load_persisted_user_count(pool: &SqlitePool) -> Result<i64, sqlx::Error> {
    let count = sqlx::query(r#"SELECT COUNT(*) AS COUNT FROM USER"#)
        .fetch_one(pool)
        .await?
        .get::<i64, _>("COUNT");
    Ok(count)
}

pub async fn persist_initial_admin_user(
    pool: &SqlitePool,
    user_id: &str,
    email: &str,
    hashed_password: &str,
) -> Result<CreatedClaimedUser, sqlx::Error> {
    let mut tx = pool.begin().await?;

    sqlx::query(
        r#"INSERT INTO USER (
            ID,
            EMAIL,
            PASSWORD,
            SHARED_ALL_LIBRARIES,
            AGE_RESTRICTION,
            AGE_RESTRICTION_ALLOW_ONLY
        )
        VALUES (?, ?, ?, ?, ?, ?)"#,
    )
    .bind(user_id)
    .bind(email)
    .bind(hashed_password)
    .bind(true)
    .bind(None::<i64>)
    .bind(None::<bool>)
    .execute(&mut *tx)
    .await?;

    for role in claim_user_roles() {
        sqlx::query(r#"INSERT INTO USER_ROLE (USER_ID, ROLE) VALUES (?, ?)"#)
            .bind(user_id)
            .bind(role)
            .execute(&mut *tx)
            .await?;
    }

    tx.commit().await?;

    Ok(CreatedClaimedUser {
        id: user_id.to_string(),
        email: email.to_string(),
    })
}

fn claim_user_roles() -> &'static [&'static str] {
    &[
        "ADMIN",
        "FILE_DOWNLOAD",
        "PAGE_STREAMING",
        "KOBO_SYNC",
        "KOREADER_SYNC",
    ]
}
