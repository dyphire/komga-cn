use bcrypt::{DEFAULT_COST, hash};
use std::time::{SystemTime, UNIX_EPOCH};

use komga_application::identity_access::AuthUserRole;
use komga_config::env_config::RuntimeConfig;
use komga_infrastructure::identity::{
    InitialBootstrapUserWriteModel, load_persisted_user_count, persist_initial_bootstrap_users,
};

pub(super) async fn ensure_noclaim_initial_users(config: &RuntimeConfig) {
    if !spring_profile_enabled("noclaim") || spring_profile_enabled("test") {
        return;
    }

    let pool =
        match komga_infrastructure::persistence::connect_read_pool(config.database_file.as_path())
            .await
        {
            Ok(pool) => pool,
            Err(error) => {
                eprintln!("failed to open database for noclaim bootstrap: {error}");
                return;
            }
        };

    let existing_users = load_persisted_user_count(&pool).await;

    let existing_users = match existing_users {
        Ok(count) => count,
        Err(error) => {
            eprintln!("failed to read existing users for noclaim bootstrap: {error}");
            return;
        }
    };

    if existing_users > 0 {
        return;
    }

    let initial_users = if spring_profile_enabled("dev") {
        vec![
            InitialUserBootstrapSpec {
                email: "admin@example.org",
                password: "admin".to_string(),
                roles: AuthUserRole::claim_roles().collect(),
            },
            InitialUserBootstrapSpec {
                email: "user@example.org",
                password: "user".to_string(),
                roles: vec![AuthUserRole::FileDownload, AuthUserRole::PageStreaming],
            },
        ]
    } else {
        vec![InitialUserBootstrapSpec {
            email: "admin@example.org",
            password: generate_alphanumeric_secret(12),
            roles: AuthUserRole::claim_roles().collect(),
        }]
    };

    let mut users_to_persist = Vec::with_capacity(initial_users.len());

    for user in &initial_users {
        let hashed_password = match hash(user.password.as_str(), DEFAULT_COST) {
            Ok(hash) => hash,
            Err(error) => {
                eprintln!(
                    "failed to hash noclaim startup password for {}: {error}",
                    user.email
                );
                return;
            }
        };

        users_to_persist.push(InitialBootstrapUserWriteModel {
            id: generate_startup_user_id(user.email),
            email: user.email.to_string(),
            hashed_password,
            roles: user
                .roles
                .iter()
                .copied()
                .map(AuthUserRole::persisted_name)
                .map(str::to_string)
                .collect(),
        });
    }

    let write_pool =
        match komga_infrastructure::persistence::connect_write_pool(config.database_file.as_path())
            .await
        {
            Ok(pool) => pool,
            Err(error) => {
                eprintln!("failed to open write database for noclaim bootstrap: {error}");
                return;
            }
        };

    if let Err(error) = persist_initial_bootstrap_users(&write_pool, &users_to_persist).await {
        eprintln!("failed to persist noclaim bootstrap users: {error}");
        return;
    }

    for user in initial_users {
        println!(
            "Initial user created. Login: {}, Password: {}",
            user.email, user.password,
        );
    }
}

struct InitialUserBootstrapSpec {
    email: &'static str,
    password: String,
    roles: Vec<AuthUserRole>,
}

fn spring_profile_enabled(profile: &str) -> bool {
    std::env::var("SPRING_PROFILES_ACTIVE")
        .ok()
        .map(|profiles| {
            profiles
                .split(',')
                .map(str::trim)
                .any(|candidate| candidate.eq_ignore_ascii_case(profile))
        })
        .unwrap_or(false)
}

fn generate_startup_user_id(seed: &str) -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|value| value.as_nanos())
        .unwrap_or_default();
    let normalized_seed = seed.replace(['@', '.'], "-");
    format!("startup-{normalized_seed}-{nanos}")
}

fn generate_alphanumeric_secret(length: usize) -> String {
    const ALPHANUM: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789";
    let mut bytes = vec![0u8; length.max(1)];
    getrandom::fill(&mut bytes).expect("system random source should be available");
    bytes
        .into_iter()
        .map(|value| ALPHANUM[(value as usize) % ALPHANUM.len()] as char)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generated_alphanumeric_secret_uses_requested_length() {
        assert_eq!(generate_alphanumeric_secret(12).len(), 12);
    }
}
