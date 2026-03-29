use std::collections::BTreeSet;
use std::path::Path;

use komga_application::identity_access::AuthUser;
use sqlx::Row;

use super::auth_access::persisted_users;
use super::backend_contract::{CreateAuthUserInput, SharedLibrariesInput, UpdateAuthUserInput};
use crate::sqlite::connect_pool;

pub async fn create_auth_user(
    database_file: &Path,
    input: CreateAuthUserInput,
) -> Result<Option<AuthUser>, sqlx::Error> {
    let pool = connect_pool(database_file, 1).await?;
    let mut tx = pool.begin().await?;

    let email_exists = sqlx::query("SELECT 1 FROM USER WHERE LOWER(EMAIL) = LOWER(?) LIMIT 1")
        .bind(&input.email)
        .fetch_optional(&mut *tx)
        .await?
        .is_some();
    if email_exists {
        tx.rollback().await?;
        return Ok(None);
    }

    let shared_libraries =
        resolve_shared_libraries(&mut tx, input.shared_libraries.clone()).await?;
    let age = input.age_restriction.as_ref().map(|value| value.age);
    let allow_only = input.age_restriction.as_ref().map(|value| value.allow_only);

    sqlx::query(
        "INSERT INTO USER (ID, EMAIL, PASSWORD, SHARED_ALL_LIBRARIES, AGE_RESTRICTION, AGE_RESTRICTION_ALLOW_ONLY) \
         VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind(&input.user_id)
    .bind(&input.email)
    .bind(&input.password_hash)
    .bind(shared_libraries.all)
    .bind(age)
    .bind(allow_only)
    .execute(&mut *tx)
    .await?;

    for role in &input.roles {
        sqlx::query("INSERT OR IGNORE INTO USER_ROLE (USER_ID, ROLE) VALUES (?, ?)")
            .bind(&input.user_id)
            .bind(role)
            .execute(&mut *tx)
            .await?;
    }

    if !shared_libraries.all {
        for library_id in &shared_libraries.library_ids {
            sqlx::query(
                "INSERT OR IGNORE INTO USER_LIBRARY_SHARING (USER_ID, LIBRARY_ID) VALUES (?, ?)",
            )
            .bind(&input.user_id)
            .bind(library_id)
            .execute(&mut *tx)
            .await?;
        }
    }

    for label in &input.labels_allow {
        sqlx::query("INSERT OR IGNORE INTO USER_SHARING (LABEL, ALLOW, USER_ID) VALUES (?, ?, ?)")
            .bind(label)
            .bind(true)
            .bind(&input.user_id)
            .execute(&mut *tx)
            .await?;
    }
    for label in &input.labels_exclude {
        sqlx::query("INSERT OR IGNORE INTO USER_SHARING (LABEL, ALLOW, USER_ID) VALUES (?, ?, ?)")
            .bind(label)
            .bind(false)
            .bind(&input.user_id)
            .execute(&mut *tx)
            .await?;
    }

    tx.commit().await?;

    persisted_users(database_file)
        .await
        .and_then(|users| {
            users
                .into_iter()
                .find(|candidate| candidate.id == input.user_id)
        })
        .ok_or(sqlx::Error::RowNotFound)
        .map(Some)
}

pub async fn delete_auth_user(
    database_file: &Path,
    target_user_id: &str,
) -> Result<bool, sqlx::Error> {
    let pool = connect_pool(database_file, 1).await?;
    let mut tx = pool.begin().await?;

    let exists = sqlx::query("SELECT 1 FROM USER WHERE ID = ? LIMIT 1")
        .bind(target_user_id)
        .fetch_optional(&mut *tx)
        .await?
        .is_some();
    if !exists {
        tx.rollback().await?;
        return Ok(false);
    }

    let sync_point_ids = sqlx::query("SELECT ID FROM SYNC_POINT WHERE USER_ID = ?")
        .bind(target_user_id)
        .fetch_all(&mut *tx)
        .await?
        .into_iter()
        .map(|row| row.get::<String, _>("ID"))
        .collect::<Vec<_>>();

    for sync_point_id in &sync_point_ids {
        for sql in [
            "DELETE FROM SYNC_POINT_BOOK WHERE SYNC_POINT_ID = ?",
            "DELETE FROM SYNC_POINT_BOOK_REMOVED_SYNCED WHERE SYNC_POINT_ID = ?",
            "DELETE FROM SYNC_POINT_READLIST WHERE SYNC_POINT_ID = ?",
            "DELETE FROM SYNC_POINT_READLIST_BOOK WHERE SYNC_POINT_ID = ?",
            "DELETE FROM SYNC_POINT_READLIST_REMOVED_SYNCED WHERE SYNC_POINT_ID = ?",
        ] {
            sqlx::query(sql)
                .bind(sync_point_id)
                .execute(&mut *tx)
                .await?;
        }
    }

    for sql in [
        "DELETE FROM SYNC_POINT WHERE USER_ID = ?",
        "DELETE FROM AUTHENTICATION_ACTIVITY WHERE USER_ID = ?",
        "DELETE FROM USER_API_KEY WHERE USER_ID = ?",
        "DELETE FROM USER_ROLE WHERE USER_ID = ?",
        "DELETE FROM USER_LIBRARY_SHARING WHERE USER_ID = ?",
        "DELETE FROM USER_SHARING WHERE USER_ID = ?",
        "DELETE FROM CLIENT_SETTINGS_USER WHERE USER_ID = ?",
        "DELETE FROM READ_PROGRESS WHERE USER_ID = ?",
        "DELETE FROM READ_PROGRESS_SERIES WHERE USER_ID = ?",
        "DELETE FROM ANNOUNCEMENTS_READ WHERE USER_ID = ?",
    ] {
        sqlx::query(sql)
            .bind(target_user_id)
            .execute(&mut *tx)
            .await?;
    }

    sqlx::query("DELETE FROM USER WHERE ID = ?")
        .bind(target_user_id)
        .execute(&mut *tx)
        .await?;

    tx.commit().await?;
    Ok(true)
}

pub async fn update_auth_user(
    database_file: &Path,
    target_user_id: &str,
    patch: UpdateAuthUserInput,
) -> Result<bool, sqlx::Error> {
    let pool = connect_pool(database_file, 1).await?;
    let mut tx = pool.begin().await?;

    let Some(user_row) = sqlx::query(
        "SELECT SHARED_ALL_LIBRARIES, AGE_RESTRICTION, AGE_RESTRICTION_ALLOW_ONLY \
         FROM USER WHERE ID = ? LIMIT 1",
    )
    .bind(target_user_id)
    .fetch_optional(&mut *tx)
    .await?
    else {
        tx.rollback().await?;
        return Ok(false);
    };

    let shared_libraries_patch = if let Some(shared_libraries) = patch.shared_libraries.clone() {
        Some(resolve_shared_libraries(&mut tx, shared_libraries).await?)
    } else {
        None
    };

    let mut shared_all_libraries = user_row.get::<bool, _>("SHARED_ALL_LIBRARIES");
    if let Some(shared_libraries) = &shared_libraries_patch {
        shared_all_libraries = shared_libraries.all;
    }

    let mut age_restriction = user_row.get::<Option<i64>, _>("AGE_RESTRICTION");
    let mut age_restriction_allow_only =
        user_row.get::<Option<bool>, _>("AGE_RESTRICTION_ALLOW_ONLY");
    if let Some(age_patch) = &patch.age_restriction {
        age_restriction = age_patch.as_ref().map(|value| value.age);
        age_restriction_allow_only = age_patch.as_ref().map(|value| value.allow_only);
    }

    sqlx::query(
        "UPDATE USER \
         SET SHARED_ALL_LIBRARIES = ?, AGE_RESTRICTION = ?, AGE_RESTRICTION_ALLOW_ONLY = ?, \
             LAST_MODIFIED_DATE = CURRENT_TIMESTAMP \
         WHERE ID = ?",
    )
    .bind(shared_all_libraries)
    .bind(age_restriction)
    .bind(age_restriction_allow_only)
    .bind(target_user_id)
    .execute(&mut *tx)
    .await?;

    if let Some(roles) = &patch.roles {
        sqlx::query("DELETE FROM USER_ROLE WHERE USER_ID = ?")
            .bind(target_user_id)
            .execute(&mut *tx)
            .await?;
        for role in roles {
            sqlx::query("INSERT OR IGNORE INTO USER_ROLE (USER_ID, ROLE) VALUES (?, ?)")
                .bind(target_user_id)
                .bind(role)
                .execute(&mut *tx)
                .await?;
        }
    }

    if let Some(shared_libraries) = &shared_libraries_patch {
        sqlx::query("DELETE FROM USER_LIBRARY_SHARING WHERE USER_ID = ?")
            .bind(target_user_id)
            .execute(&mut *tx)
            .await?;
        for library_id in &shared_libraries.library_ids {
            sqlx::query(
                "INSERT OR IGNORE INTO USER_LIBRARY_SHARING (USER_ID, LIBRARY_ID) VALUES (?, ?)",
            )
            .bind(target_user_id)
            .bind(library_id)
            .execute(&mut *tx)
            .await?;
        }
    }

    if patch.labels_allow.is_some() || patch.labels_exclude.is_some() {
        let (existing_allow, existing_exclude) =
            load_user_sharing_labels(&mut tx, target_user_id).await?;
        let labels_allow = patch.labels_allow.unwrap_or(existing_allow);
        let labels_exclude = patch.labels_exclude.unwrap_or(existing_exclude);

        sqlx::query("DELETE FROM USER_SHARING WHERE USER_ID = ?")
            .bind(target_user_id)
            .execute(&mut *tx)
            .await?;

        for label in &labels_allow {
            sqlx::query(
                "INSERT OR IGNORE INTO USER_SHARING (LABEL, ALLOW, USER_ID) VALUES (?, ?, ?)",
            )
            .bind(label)
            .bind(true)
            .bind(target_user_id)
            .execute(&mut *tx)
            .await?;
        }
        for label in &labels_exclude {
            sqlx::query(
                "INSERT OR IGNORE INTO USER_SHARING (LABEL, ALLOW, USER_ID) VALUES (?, ?, ?)",
            )
            .bind(label)
            .bind(false)
            .bind(target_user_id)
            .execute(&mut *tx)
            .await?;
        }
    }

    tx.commit().await?;
    Ok(true)
}

async fn resolve_shared_libraries(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    shared_libraries: SharedLibrariesInput,
) -> Result<SharedLibrariesInput, sqlx::Error> {
    if shared_libraries.all {
        return Ok(SharedLibrariesInput {
            all: true,
            library_ids: Vec::new(),
        });
    }

    let existing = sqlx::query("SELECT ID FROM LIBRARY ORDER BY ID")
        .fetch_all(&mut **tx)
        .await?
        .into_iter()
        .map(|row| row.get::<String, _>("ID"))
        .collect::<BTreeSet<_>>();

    Ok(SharedLibrariesInput {
        all: false,
        library_ids: shared_libraries
            .library_ids
            .into_iter()
            .filter(|library_id| existing.contains(library_id))
            .collect(),
    })
}

async fn load_user_sharing_labels(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    user_id: &str,
) -> Result<(Vec<String>, Vec<String>), sqlx::Error> {
    let rows =
        sqlx::query("SELECT LABEL, ALLOW FROM USER_SHARING WHERE USER_ID = ? ORDER BY LABEL")
            .bind(user_id)
            .fetch_all(&mut **tx)
            .await?;

    let mut allow = Vec::new();
    let mut exclude = Vec::new();
    for row in rows {
        let label = row.get::<String, _>("LABEL");
        if row.get::<bool, _>("ALLOW") {
            allow.push(label);
        } else {
            exclude.push(label);
        }
    }

    Ok((allow, exclude))
}
