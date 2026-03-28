use bcrypt::{DEFAULT_COST, hash as hash_bcrypt_password};
use sqlx::Row;
use std::io::Read;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    let admin_commands = parse_admin_cli_commands(std::env::args().skip(1));

    let config = komga_rust::config::RuntimeConfig::from_env().expect("invalid runtime config");
    validate_startup_schema_gate(&config).await;
    ensure_noclaim_initial_users(&config).await;
    run_admin_cli_commands(&config, &admin_commands).await;

    let listener = tokio::net::TcpListener::bind(config.bind_address)
        .await
        .expect("failed to bind address");

    komga_rust::app::serve_with_config(listener, config)
        .await
        .expect("server error");
}

#[derive(Debug, Default, Eq, PartialEq)]
struct AdminCliCommands {
    list_users: bool,
    reset_emails: Vec<String>,
    new_password: Option<String>,
}

fn parse_admin_cli_commands<I>(args: I) -> AdminCliCommands
where
    I: IntoIterator,
    I::Item: Into<String>,
{
    let mut commands = AdminCliCommands::default();
    let mut pending_reset = false;
    let mut pending_new_password = false;

    for raw in args.into_iter().map(Into::into) {
        if pending_reset {
            if !raw.trim().is_empty() {
                commands.reset_emails.push(raw);
            }
            pending_reset = false;
            continue;
        }

        if pending_new_password {
            if !raw.trim().is_empty() {
                commands.new_password = Some(raw);
            }
            pending_new_password = false;
            continue;
        }

        if raw == "--list-users" {
            commands.list_users = true;
            continue;
        }

        if let Some(value) = raw.strip_prefix("--reset=") {
            if !value.trim().is_empty() {
                commands.reset_emails.push(value.to_string());
            }
            continue;
        }

        if raw == "--reset" {
            pending_reset = true;
            continue;
        }

        if let Some(value) = raw.strip_prefix("--newpassword=") {
            if !value.trim().is_empty() {
                commands.new_password = Some(value.to_string());
            }
            continue;
        }

        if raw == "--newpassword" {
            pending_new_password = true;
        }
    }

    commands
}

async fn ensure_noclaim_initial_users(config: &komga_rust::config::RuntimeConfig) {
    if !spring_profile_enabled("noclaim") || spring_profile_enabled("test") {
        return;
    }

    let pool = match komga_rust::persistence::sqlite::connect_pool(&config.database_file, 1).await {
        Ok(pool) => pool,
        Err(error) => {
            eprintln!("failed to open sqlite database for noclaim bootstrap: {error}");
            return;
        }
    };

    let existing_users = sqlx::query(
        "SELECT COUNT(*) AS COUNT \
         FROM USER",
    )
    .fetch_one(&pool)
    .await
    .map(|row| row.get::<i64, _>("COUNT"));

    let existing_users = match existing_users {
        Ok(count) => count,
        Err(error) => {
            pool.close().await;
            eprintln!("failed to read existing users for noclaim bootstrap: {error}");
            return;
        }
    };

    if existing_users > 0 {
        pool.close().await;
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

    let mut tx = match pool.begin().await {
        Ok(tx) => tx,
        Err(error) => {
            pool.close().await;
            eprintln!("failed to open noclaim bootstrap transaction: {error}");
            return;
        }
    };

    for user in &initial_users {
        let hashed_password = match hash_bcrypt_password(user.password.as_str(), DEFAULT_COST) {
            Ok(hash) => hash,
            Err(error) => {
                let _ = tx.rollback().await;
                pool.close().await;
                eprintln!(
                    "failed to hash noclaim startup password for {}: {error}",
                    user.email
                );
                return;
            }
        };

        let user_id = generate_startup_user_id(user.email);
        let inserted_user = sqlx::query(
            "INSERT INTO USER (ID, EMAIL, PASSWORD, SHARED_ALL_LIBRARIES, AGE_RESTRICTION, \
               AGE_RESTRICTION_ALLOW_ONLY) \
             VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(&user_id)
        .bind(user.email)
        .bind(&hashed_password)
        .bind(true)
        .bind(None::<i64>)
        .bind(None::<bool>)
        .execute(&mut *tx)
        .await;
        if let Err(error) = inserted_user {
            let _ = tx.rollback().await;
            pool.close().await;
            eprintln!(
                "failed to insert noclaim startup user {}: {error}",
                user.email
            );
            return;
        }

        for role in &user.roles {
            let inserted_role = sqlx::query(
                "INSERT INTO USER_ROLE (USER_ID, ROLE) \
                 VALUES (?, ?)",
            )
            .bind(&user_id)
            .bind(role)
            .execute(&mut *tx)
            .await;
            if let Err(error) = inserted_role {
                let _ = tx.rollback().await;
                pool.close().await;
                eprintln!(
                    "failed to insert noclaim startup role '{role}' for {}: {error}",
                    user.email,
                );
                return;
            }
        }
    }

    if let Err(error) = tx.commit().await {
        pool.close().await;
        eprintln!("failed to commit noclaim bootstrap transaction: {error}");
        return;
    }

    pool.close().await;
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

async fn run_admin_cli_commands(
    config: &komga_rust::config::RuntimeConfig,
    commands: &AdminCliCommands,
) {
    if commands.list_users {
        print_user_list(config.database_file.as_path()).await;
    }

    if commands.reset_emails.is_empty() && commands.new_password.is_none() {
        return;
    }

    if commands.reset_emails.is_empty() || commands.new_password.is_none() {
        eprintln!(
            "You need to specify both '--reset=user@domain.com' and '--newpassword=YourNewPassword'"
        );
        return;
    }

    let new_password = commands
        .new_password
        .as_deref()
        .map(str::trim)
        .unwrap_or_default();
    if new_password.is_empty() {
        eprintln!("The new password must not be blank");
        return;
    }

    let hashed_password = match hash_bcrypt_password(new_password, DEFAULT_COST) {
        Ok(hash) => hash,
        Err(error) => {
            eprintln!("failed to hash reset password: {error}");
            return;
        }
    };

    let pool = match komga_rust::persistence::sqlite::connect_pool(&config.database_file, 1).await {
        Ok(pool) => pool,
        Err(error) => {
            eprintln!("failed to open sqlite database for password reset: {error}");
            return;
        }
    };

    let remember_me_store_root = config
        .config_dir
        .as_deref()
        .or_else(|| config.database_file.parent())
        .unwrap_or_else(|| Path::new("."));
    let _ = komga_server::app::configure_remember_me_store_root(remember_me_store_root);

    for email in &commands.reset_emails {
        let user = sqlx::query(
            "SELECT ID, EMAIL \
             FROM USER \
             WHERE LOWER(EMAIL) = LOWER(?) \
             LIMIT 1",
        )
        .bind(email)
        .fetch_optional(&pool)
        .await;

        let Some(user) = (match user {
            Ok(row) => row,
            Err(error) => {
                eprintln!("failed to query user for password reset ({email}): {error}");
                continue;
            }
        }) else {
            eprintln!("User does not exist: {email}");
            continue;
        };

        let user_id = user.get::<String, _>("ID");
        let user_email = user.get::<String, _>("EMAIL");

        let update_result = sqlx::query(
            "UPDATE USER \
             SET PASSWORD = ? \
             WHERE ID = ?",
        )
        .bind(&hashed_password)
        .bind(&user_id)
        .execute(&pool)
        .await;

        match update_result {
            Ok(result) if result.rows_affected() > 0 => {
                komga_server::app::invalidate_sessions_for_user(user_id.as_str());
                println!("Reset password for user: {user_email}")
            }
            Ok(_) => eprintln!("User does not exist: {email}"),
            Err(error) => eprintln!("failed to reset password for user {email}: {error}"),
        }
    }

    pool.close().await;
}

async fn print_user_list(database_file: &Path) {
    let pool = match komga_rust::persistence::sqlite::connect_pool(database_file, 1).await {
        Ok(pool) => pool,
        Err(error) => {
            eprintln!("failed to open sqlite database for list-users: {error}");
            return;
        }
    };

    let rows = sqlx::query(
        "SELECT EMAIL \
         FROM USER \
         ORDER BY EMAIL",
    )
    .fetch_all(&pool)
    .await;

    match rows {
        Ok(rows) if rows.is_empty() => println!("No users exist yet"),
        Ok(rows) => {
            let emails = rows
                .into_iter()
                .map(|row| row.get::<String, _>("EMAIL"))
                .collect::<Vec<_>>();
            println!("Here is a list of all users: {:?}", emails);
        }
        Err(error) => eprintln!("failed to list users: {error}"),
    }

    pool.close().await;
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

async fn validate_startup_schema_gate(config: &komga_rust::config::RuntimeConfig) {
    let main_pool = komga_rust::persistence::sqlite::connect_pool(&config.database_file, 1)
        .await
        .expect("failed to open main sqlite database");
    let main_schema_result =
        komga_rust::persistence::sqlite::setup::bootstrap_pool(&main_pool).await;
    main_pool.close().await;
    main_schema_result.expect("main sqlite schema gate failed");

    let tasks_pool = komga_rust::persistence::sqlite::connect_pool(&config.tasks_db_file, 1)
        .await
        .expect("failed to open tasks sqlite database");
    let tasks_schema_result =
        komga_rust::persistence::sqlite::setup::bootstrap_tasks_pool(&tasks_pool).await;
    tasks_pool.close().await;
    tasks_schema_result.expect("tasks sqlite schema gate failed");
}

#[cfg(test)]
mod tests {
    use super::{AdminCliCommands, parse_admin_cli_commands};

    #[test]
    fn parse_admin_cli_commands_supports_equals_and_split_forms() {
        let parsed = parse_admin_cli_commands([
            "--list-users",
            "--reset=alice@example.org",
            "--reset",
            "bob@example.org",
            "--newpassword",
            "secret-1",
        ]);

        assert_eq!(
            parsed,
            AdminCliCommands {
                list_users: true,
                reset_emails: vec![
                    "alice@example.org".to_string(),
                    "bob@example.org".to_string(),
                ],
                new_password: Some("secret-1".to_string()),
            },
        );
    }

    #[test]
    fn parse_admin_cli_commands_ignores_blank_values() {
        let parsed = parse_admin_cli_commands([
            "--reset=",
            "--newpassword=",
            "--reset",
            "   ",
            "--newpassword",
            "   ",
        ]);

        assert_eq!(
            parsed,
            AdminCliCommands {
                list_users: false,
                reset_emails: vec![],
                new_password: None,
            },
        );
    }
}
