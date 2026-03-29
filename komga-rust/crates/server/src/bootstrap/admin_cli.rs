use bcrypt::{DEFAULT_COST, hash as hash_bcrypt_password};
use std::path::Path;

use crate::config::RuntimeConfig;

#[derive(Debug, Default, Eq, PartialEq)]
pub struct AdminCliCommands {
    list_users: bool,
    reset_emails: Vec<String>,
    new_password: Option<String>,
}

pub fn parse_admin_cli_commands<I>(args: I) -> AdminCliCommands
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

pub async fn run_admin_cli_commands(config: &RuntimeConfig, commands: &AdminCliCommands) {
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

    let remember_me_store_root = config
        .config_dir
        .as_deref()
        .or_else(|| config.database_file.parent())
        .unwrap_or_else(|| Path::new("."));
    let _ = komga_interfaces::http::identity_access::auth::configure_remember_me_store(
        remember_me_store_root,
    );

    for email in &commands.reset_emails {
        let user = komga_infrastructure::sqlite::write_models::load_persisted_user_by_email(
            config.database_file.as_path(),
            email,
        )
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

        let update_result =
            komga_infrastructure::sqlite::write_models::update_persisted_user_password(
                config.database_file.as_path(),
                &user.id,
                &hashed_password,
            )
            .await;

        match update_result {
            Ok(true) => {
                komga_interfaces::http::identity_access::auth::invalidate_user_sessions(
                    user.id.as_str(),
                );
                println!("Reset password for user: {}", user.email)
            }
            Ok(false) => eprintln!("User does not exist: {email}"),
            Err(error) => eprintln!("failed to reset password for user {email}: {error}"),
        }
    }
}

async fn print_user_list(database_file: &Path) {
    let rows =
        komga_infrastructure::sqlite::write_models::list_persisted_user_emails(database_file).await;

    match rows {
        Ok(rows) if rows.is_empty() => println!("No users exist yet"),
        Ok(rows) => println!("Here is a list of all users: {:?}", rows),
        Err(error) => eprintln!("failed to list users: {error}"),
    }
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
