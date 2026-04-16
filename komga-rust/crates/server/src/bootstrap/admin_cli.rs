use bcrypt::{DEFAULT_COST, hash as hash_bcrypt_password};
use std::fmt;
use std::path::Path;

#[derive(Debug, Default, Eq, PartialEq)]
pub struct AdminCliCommands {
    list_users: bool,
    reset_emails: Vec<String>,
    new_password: Option<String>,
}

#[derive(Debug, Eq, PartialEq)]
pub enum StartupCliPreflight {
    Help,
    Admin(AdminCliCommands),
    Server,
}

#[derive(Debug, Eq, PartialEq)]
pub struct CliUsageError {
    message: String,
}

impl CliUsageError {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl fmt::Display for CliUsageError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for CliUsageError {}

#[derive(Debug, Eq, PartialEq)]
pub struct AdminCliActionError {
    message: String,
}

impl AdminCliActionError {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl fmt::Display for AdminCliActionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for AdminCliActionError {}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PendingValueFlag {
    Reset,
    NewPassword,
}

impl PendingValueFlag {
    fn error_message(self) -> &'static str {
        match self {
            Self::Reset => "Missing value for --reset. Use --reset=<email> or --reset <email>.",
            Self::NewPassword => {
                "Missing value for --newpassword. Use --newpassword=<password> or --newpassword <password>."
            }
        }
    }
}

const PASSWORD_RESET_PAIRING_ERROR: &str = "Password reset requires both '--reset=<email>' (or '--reset <email>') and '--newpassword=<password>' (or '--newpassword <password>').";

pub fn render_usage() -> String {
    [
        "Usage: komga-rust [OPTIONS]",
        "",
        "By default komga-rust starts the HTTP server and background runtime.",
        "",
        "Admin options:",
        "  --list-users                 Print persisted user emails and exit.",
        "  --reset <email>              Reset one or more user passwords and exit.",
        "  --newpassword <password>     Provide the password used with --reset.",
        "  -h, --help                   Print this help and exit.",
        "",
        "Environment is configured mainly through env vars:",
        "  KOMGA_CONFIG_DIR             Base config/data directory.",
        "  KOMGA_RUST_ADDR              HTTP bind address (default: 127.0.0.1:25600).",
        "  KOMGA_RUST_MODE              Runtime mode: snapshot, localdb, isolated, canary.",
        "  KOMGA_DATABASE_FILE          Main SQLite database path.",
        "  KOMGA_TASKS_DB_FILE          Tasks SQLite database path.",
        "  SERVER_SERVLET_CONTEXT_PATH  Optional HTTP base path.",
        "",
        "Examples:",
        "  komga-rust",
        "  komga-rust --list-users",
        "  komga-rust --reset alice@example.org --newpassword new-secret",
        "",
        "Notes:",
        "  Use the = form when a value starts with '-'.",
    ]
    .join("\n")
}

pub fn parse_startup_cli<I>(args: I) -> Result<StartupCliPreflight, CliUsageError>
where
    I: IntoIterator,
    I::Item: Into<String>,
{
    let mut commands = AdminCliCommands::default();
    let mut pending_value: Option<PendingValueFlag> = None;

    for raw in args.into_iter().map(Into::into) {
        if let Some(flag) = pending_value.take() {
            if is_help_flag(raw.as_str()) {
                return Ok(StartupCliPreflight::Help);
            }

            if raw.trim().is_empty() || raw.starts_with('-') {
                return Err(CliUsageError::new(flag.error_message()));
            }
            apply_pending_value(&mut commands, flag, raw.as_str());
            continue;
        }

        match raw.as_str() {
            "-h" | "--help" => return Ok(StartupCliPreflight::Help),
            "--list-users" => commands.list_users = true,
            "--reset" => pending_value = Some(PendingValueFlag::Reset),
            "--newpassword" => pending_value = Some(PendingValueFlag::NewPassword),
            _ => {
                if let Some(value) = raw.strip_prefix("--reset=") {
                    if value.trim().is_empty() {
                        return Err(CliUsageError::new(PendingValueFlag::Reset.error_message()));
                    }
                    commands.reset_emails.push(value.trim().to_string());
                    continue;
                }

                if let Some(value) = raw.strip_prefix("--newpassword=") {
                    if value.trim().is_empty() {
                        return Err(CliUsageError::new(
                            PendingValueFlag::NewPassword.error_message(),
                        ));
                    }
                    commands.new_password = Some(value.to_string());
                    continue;
                }

                return Err(CliUsageError::new(format!("Unknown argument: {raw}")));
            }
        }
    }

    if let Some(flag) = pending_value {
        return Err(CliUsageError::new(flag.error_message()));
    }

    if commands.reset_emails.is_empty() != commands.new_password.is_none() {
        return Err(CliUsageError::new(PASSWORD_RESET_PAIRING_ERROR));
    }

    if commands.list_users || !commands.reset_emails.is_empty() {
        Ok(StartupCliPreflight::Admin(commands))
    } else {
        Ok(StartupCliPreflight::Server)
    }
}

fn is_help_flag(argument: &str) -> bool {
    matches!(argument, "-h" | "--help")
}

fn apply_pending_value(commands: &mut AdminCliCommands, flag: PendingValueFlag, value: &str) {
    match flag {
        PendingValueFlag::Reset => commands.reset_emails.push(value.trim().to_string()),
        PendingValueFlag::NewPassword => commands.new_password = Some(value.to_string()),
    }
}

pub async fn run_admin_cli_commands(
    database_file: &Path,
    commands: &AdminCliCommands,
) -> Result<(), AdminCliActionError> {
    if commands.list_users {
        print_user_list(database_file).await?;
    }

    if commands.reset_emails.is_empty() {
        return Ok(());
    }

    let new_password = commands
        .new_password
        .as_deref()
        .ok_or_else(|| AdminCliActionError::new(PASSWORD_RESET_PAIRING_ERROR))?;

    let mut users = Vec::with_capacity(commands.reset_emails.len());
    let mut failures = Vec::new();

    for email in &commands.reset_emails {
        let user = komga_infrastructure::sqlite::write_models::bootstrap_users::load_persisted_user_by_email(
            database_file,
            email,
        )
        .await;

        let Some(user) = (match user {
            Ok(row) => row,
            Err(error) => {
                failures.push(format!(
                    "failed to query user for password reset ({email}): {error}"
                ));
                continue;
            }
        }) else {
            failures.push(format!("User does not exist: {email}"));
            continue;
        };
        users.push(user);
    }

    if !failures.is_empty() {
        return Err(AdminCliActionError::new(failures.join("\n")));
    }

    let password_updates = users
        .iter()
        .map(|user| {
            hash_bcrypt_password(new_password, DEFAULT_COST)
                .map(|hashed_password| (user.id.clone(), hashed_password))
                .map_err(|error| {
                    AdminCliActionError::new(format!("failed to hash reset password: {error}"))
                })
        })
        .collect::<Result<Vec<_>, _>>()?;

    komga_infrastructure::sqlite::write_models::bootstrap_users::update_persisted_user_passwords(
        database_file,
        &password_updates,
    )
    .await
    .map_err(|error| {
        AdminCliActionError::new(format!("failed to reset password batch: {error}"))
    })?;

    for user in users {
        komga_infrastructure::runtime_identity_access::invalidate_user_sessions(user.id.as_str());
        println!("Reset password for user: {}", user.email);
    }

    Ok(())
}

async fn print_user_list(database_file: &Path) -> Result<(), AdminCliActionError> {
    let rows = komga_infrastructure::sqlite::write_models::bootstrap_users::list_persisted_user_emails(
        database_file,
    )
    .await;

    match rows {
        Ok(rows) if rows.is_empty() => {
            println!("No users exist yet");
            Ok(())
        }
        Ok(rows) => {
            println!("Here is a list of all users: {:?}", rows);
            Ok(())
        }
        Err(error) => Err(AdminCliActionError::new(format!(
            "failed to list users: {error}"
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::{AdminCliCommands, StartupCliPreflight, parse_startup_cli};

    #[test]
    fn parse_startup_cli_returns_server_when_no_action_is_requested() {
        assert_eq!(
            parse_startup_cli([] as [&str; 0]).expect("empty CLI should parse"),
            StartupCliPreflight::Server,
        );
    }

    #[test]
    fn parse_startup_cli_supports_equals_and_split_action_forms() {
        let parsed = parse_startup_cli([
            "--list-users",
            "--reset=alice@example.org",
            "--reset",
            "bob@example.org",
            "--newpassword",
            "secret-1",
        ])
        .expect("supported admin flags should parse");

        assert_eq!(
            parsed,
            StartupCliPreflight::Admin(AdminCliCommands {
                list_users: true,
                reset_emails: vec![
                    "alice@example.org".to_string(),
                    "bob@example.org".to_string(),
                ],
                new_password: Some("secret-1".to_string()),
            }),
        );
    }
}
