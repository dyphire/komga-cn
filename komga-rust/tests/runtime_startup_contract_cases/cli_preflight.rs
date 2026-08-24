use super::support::connect_test_pool;
use bcrypt::{DEFAULT_COST, hash as hash_bcrypt_password, verify as verify_bcrypt_password};
use sqlx::Row;
use std::fs;
use std::future::Future;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use komga_infrastructure::{
    identity::InitialBootstrapUserWriteModel, identity::persist_initial_bootstrap_users,
    persistence::bootstrap_pool, persistence::bootstrap_tasks_pool,
};

fn run_cli(args: &[&str]) -> std::process::Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_komga-rust"));
    command
        .args(args)
        .env("KOMGA_RUST_MODE", "definitely-invalid");
    command.output().expect("CLI command should run")
}

fn unique_cli_config_dir(prefix: &str) -> PathBuf {
    std::env::temp_dir().join(format!(
        "{prefix}-{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock should be after unix epoch")
            .as_nanos()
    ))
}

fn run_async<T>(future: impl Future<Output = T>) -> T {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("test runtime should build")
        .block_on(future)
}

fn prepare_action_fixture(config_dir: &Path, email: &str, password: &str) {
    prepare_action_fixture_with_users(config_dir, &[(email, password)]);
}

fn prepare_action_fixture_with_users(config_dir: &Path, users: &[(&str, &str)]) {
    fs::create_dir_all(config_dir).expect("CLI action config dir should be created");

    run_async(async {
        let database_file = config_dir.join("database.sqlite");
        let tasks_db_file = config_dir.join("tasks.sqlite");

        let main_pool = connect_test_pool(&database_file, 1)
            .await
            .expect("CLI action main pool should open");
        bootstrap_pool(&main_pool)
            .await
            .expect("CLI action main schema should bootstrap");

        let tasks_pool = connect_test_pool(&tasks_db_file, 1)
            .await
            .expect("CLI action tasks pool should open");
        bootstrap_tasks_pool(&tasks_pool)
            .await
            .expect("CLI action tasks schema should bootstrap");
        tasks_pool.close().await;

        let user_write_models = users
            .iter()
            .enumerate()
            .map(|(index, (email, password))| {
                let hashed_password = hash_bcrypt_password(password, DEFAULT_COST)
                    .expect("CLI action fixture password hash should be created");

                InitialBootstrapUserWriteModel {
                    id: format!("cli-user-{index}"),
                    email: (*email).to_string(),
                    hashed_password,
                    roles: vec!["ROLE_ADMIN".to_string()],
                }
            })
            .collect::<Vec<_>>();

        persist_initial_bootstrap_users(&main_pool, &user_write_models)
            .await
            .expect("CLI action fixture user should persist");
        main_pool.close().await;
    });
}

fn load_password_hash(config_dir: &Path, email: &str) -> String {
    run_async(async {
        let database_file = config_dir.join("database.sqlite");
        let pool = connect_test_pool(&database_file, 1)
            .await
            .expect("password verification pool should open");
        let password = sqlx::query("SELECT PASSWORD FROM USER WHERE EMAIL = ? LIMIT 1")
            .bind(email)
            .fetch_one(&pool)
            .await
            .expect("expected user row should exist")
            .get::<String, _>("PASSWORD");
        pool.close().await;
        password
    })
}

fn run_cli_action(args: &[&str], config_dir: &Path) -> std::process::Output {
    let tasks_blocker = config_dir.join("blocked-tasks-parent");
    fs::write(&tasks_blocker, "not a directory")
        .expect("CLI action tasks blocker file should be created");

    let mut command = Command::new(env!("CARGO_BIN_EXE_komga-rust"));
    command
        .args(args)
        .env("KOMGA_CONFIG_DIR", config_dir)
        .env("KOMGA_RUST_ADDR", "definitely-not-a-socket-address")
        .env("KOMGA_TASKS_DB_FILE", tasks_blocker.join("tasks.sqlite"));
    command.output().expect("CLI action command should run")
}

#[test]
fn help_flag_prints_usage_before_runtime_config_resolution() {
    let output = run_cli(&["--help"]);

    assert!(
        output.status.success(),
        "expected --help to exit successfully, stderr was: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Usage: komga-rust"), "stdout was: {stdout}");
    assert!(
        stdout.contains("Environment is configured mainly through env vars"),
        "stdout was: {stdout}"
    );
}

#[test]
fn invalid_arguments_fail_before_runtime_config_resolution() {
    let cases = [
        (
            vec!["--wat"],
            "Unknown argument: --wat",
            "unknown flags should fail before startup",
        ),
        (
            vec!["--reset", "alice@example.org"],
            "Password reset requires both '--reset=<email>'",
            "incomplete reset should fail before startup",
        ),
        (
            vec!["--reset", "--wat"],
            "Missing value for --reset. Use --reset=<email> or --reset <email>.",
            "dash-prefixed value should be treated as a missing reset value",
        ),
    ];

    for (args, expected_error, context) in cases {
        let output = run_cli(&args);

        assert!(
            !output.status.success(),
            "{context}: stdout was: {}",
            String::from_utf8_lossy(&output.stdout)
        );
        assert_eq!(output.status.code(), Some(2), "{context}");

        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            stderr.contains(expected_error),
            "{context}: stderr was: {stderr}"
        );
        assert!(
            stderr.contains("Usage: komga-rust"),
            "{context}: stderr was: {stderr}"
        );
        assert!(
            !stderr.contains("invalid runtime config"),
            "{context}: stderr was: {stderr}"
        );
    }
}

#[test]
fn list_users_action_exits_without_starting_http_server() {
    let config_dir = unique_cli_config_dir("komga-cli-list-users");
    prepare_action_fixture(&config_dir, "alice@example.org", "old-secret");

    let output = run_cli_action(&["--list-users"], &config_dir);

    assert!(
        output.status.success(),
        "expected list-users action to exit successfully, stderr was: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("Here is a list of all users: [\"alice@example.org\"]"),
        "stdout was: {stdout}"
    );
    assert!(
        !stdout.contains("Komga Rust startup"),
        "stdout should not include startup banner/logs: {stdout}"
    );
    assert!(
        !stdout.contains("Version: "),
        "stdout should not include runtime startup banner details: {stdout}"
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.is_empty(), "stderr was: {stderr}");
}

#[test]
fn reset_action_updates_password_and_exits_without_starting_http_server() {
    let config_dir = unique_cli_config_dir("komga-cli-reset-password");
    prepare_action_fixture(&config_dir, "alice@example.org", "old-secret");

    let output = run_cli_action(
        &[
            "--reset",
            "alice@example.org",
            "--newpassword",
            "new-secret",
        ],
        &config_dir,
    );

    assert!(
        output.status.success(),
        "expected reset action to exit successfully, stderr was: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("Reset password for user: alice@example.org"),
        "stdout was: {stdout}"
    );
    assert!(
        !stdout.contains("Komga Rust startup"),
        "stdout should not include startup banner/logs: {stdout}"
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.is_empty(), "stderr was: {stderr}");

    let updated_password_hash = load_password_hash(&config_dir, "alice@example.org");

    assert!(
        verify_bcrypt_password("new-secret", &updated_password_hash)
            .expect("updated CLI action password should verify"),
        "expected reset action to persist the new password hash"
    );
}

#[test]
fn reset_action_failures_return_non_zero_exit_code() {
    let config_dir = unique_cli_config_dir("komga-cli-reset-failure");
    prepare_action_fixture(&config_dir, "alice@example.org", "old-secret");

    let output = run_cli_action(
        &[
            "--reset",
            "missing@example.org",
            "--newpassword",
            "new-secret",
        ],
        &config_dir,
    );

    assert!(
        !output.status.success(),
        "expected reset failure to exit non-zero, stdout was: {}",
        String::from_utf8_lossy(&output.stdout)
    );
    assert_eq!(output.status.code(), Some(1));

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("User does not exist: missing@example.org"),
        "stderr was: {stderr}"
    );
    assert!(
        !stderr.contains("invalid KOMGA_RUST_ADDR"),
        "stderr should not mention unrelated HTTP config: {stderr}"
    );
}

#[test]
fn admin_actions_fail_fast_when_main_database_is_missing() {
    let config_dir = unique_cli_config_dir("komga-cli-missing-main-db");
    fs::create_dir_all(&config_dir).expect("missing-db config dir should exist for test");

    let database_file = config_dir.join("database.sqlite");
    assert!(
        !database_file.exists(),
        "test fixture should begin without a main database file"
    );

    let output = run_cli_action(&["--list-users"], &config_dir);

    assert!(
        !output.status.success(),
        "expected missing main db to fail, stdout was: {}",
        String::from_utf8_lossy(&output.stdout)
    );
    assert_eq!(output.status.code(), Some(1));

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("admin action requires an existing main database"),
        "stderr was: {stderr}"
    );
    assert!(
        !database_file.exists(),
        "missing-db admin action should not bootstrap a brand-new main database"
    );
}

#[test]
fn mixed_batch_reset_does_not_partially_update_existing_users() {
    let config_dir = unique_cli_config_dir("komga-cli-reset-mixed-batch");
    prepare_action_fixture_with_users(
        &config_dir,
        &[
            ("alice@example.org", "old-secret"),
            ("bob@example.org", "older-secret"),
        ],
    );

    let alice_password_before = load_password_hash(&config_dir, "alice@example.org");

    let output = run_cli_action(
        &[
            "--reset",
            "alice@example.org",
            "--reset",
            "missing@example.org",
            "--newpassword",
            "new-secret",
        ],
        &config_dir,
    );

    assert!(
        !output.status.success(),
        "expected mixed batch reset to fail, stdout was: {}",
        String::from_utf8_lossy(&output.stdout)
    );
    assert_eq!(output.status.code(), Some(1));

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("User does not exist: missing@example.org"),
        "stderr was: {stderr}"
    );

    let alice_password_after = load_password_hash(&config_dir, "alice@example.org");
    assert_eq!(alice_password_after, alice_password_before);
    assert!(
        verify_bcrypt_password("old-secret", &alice_password_after)
            .expect("original alice password hash should still verify"),
        "mixed batch reset failure should leave existing user passwords unchanged"
    );
}
