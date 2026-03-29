use bcrypt::{DEFAULT_COST, hash as hash_bcrypt_password};
use std::io::Read;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::config::RuntimeConfig;

pub async fn ensure_noclaim_initial_users(config: &RuntimeConfig) {
    if !spring_profile_enabled("noclaim") || spring_profile_enabled("test") {
        return;
    }

    let existing_users = komga_infrastructure::sqlite::write_models::load_persisted_user_count(
        config.database_file.as_path(),
    )
    .await;

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
                roles: vec![
                    "ADMIN",
                    "FILE_DOWNLOAD",
                    "PAGE_STREAMING",
                    "KOBO_SYNC",
                    "KOREADER_SYNC",
                ],
            },
            InitialUserBootstrapSpec {
                email: "user@example.org",
                password: "user".to_string(),
                roles: vec!["FILE_DOWNLOAD", "PAGE_STREAMING"],
            },
        ]
    } else {
        vec![InitialUserBootstrapSpec {
            email: "admin@example.org",
            password: generate_alphanumeric_secret(12),
            roles: vec![
                "ADMIN",
                "FILE_DOWNLOAD",
                "PAGE_STREAMING",
                "KOBO_SYNC",
                "KOREADER_SYNC",
            ],
        }]
    };

    let mut users_to_persist = Vec::with_capacity(initial_users.len());

    for user in &initial_users {
        let hashed_password = match hash_bcrypt_password(user.password.as_str(), DEFAULT_COST) {
            Ok(hash) => hash,
            Err(error) => {
                eprintln!(
                    "failed to hash noclaim startup password for {}: {error}",
                    user.email
                );
                return;
            }
        };

        users_to_persist.push(
            komga_infrastructure::sqlite::write_models::InitialBootstrapUserWriteModel {
                id: generate_startup_user_id(user.email),
                email: user.email.to_string(),
                hashed_password,
                roles: user.roles.iter().map(|role| (*role).to_string()).collect(),
            },
        );
    }

    if let Err(error) = komga_infrastructure::sqlite::write_models::persist_initial_bootstrap_users(
        config.database_file.as_path(),
        &users_to_persist,
    )
    .await
    {
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
    roles: Vec<&'static str>,
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
    let normalized_seed = seed.replace('@', "-").replace('.', "-");
    format!("startup-{normalized_seed}-{nanos}")
}

fn generate_alphanumeric_secret(length: usize) -> String {
    const ALPHANUM: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789";
    let mut bytes = vec![0u8; length.max(1)];
    if let Ok(mut file) = std::fs::File::open("/dev/urandom")
        && file.read_exact(&mut bytes).is_ok()
    {
        return bytes
            .into_iter()
            .map(|value| ALPHANUM[(value as usize) % ALPHANUM.len()] as char)
            .collect();
    }

    let seed = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|value| value.as_nanos())
        .unwrap_or_default();
    (0..length.max(1))
        .map(|index| {
            let mixed = seed.wrapping_add(index as u128 * 7919);
            ALPHANUM[(mixed as usize) % ALPHANUM.len()] as char
        })
        .collect()
}
