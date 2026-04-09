use super::support::*;
use std::io::Write;

use time::OffsetDateTime;

#[test]
fn runtime_logging_initialization_installs_exactly_once_per_process() {
    let config = runtime_config_for_logging_contract("komga-runtime-logging-init-once");

    let first_install =
        komga_server::logging::init_global(&config).expect("first logging install should succeed");
    let second_install = komga_server::logging::init_global(&config)
        .expect("second logging install should not fail");

    println!(
        "logging_install_contract first_install={first_install} second_install={second_install}"
    );

    assert!(
        first_install,
        "first logging install should report a real installation"
    );
    assert!(
        !second_install,
        "second logging install should report the existing runtime instead of panicking",
    );
}

#[test]
fn runtime_logging_rotation_keeps_active_logfile_path_stable_and_archives_as_siblings() {
    let config = runtime_config_for_logging_contract("komga-runtime-logging-rotation-contract");
    let initial_period = OffsetDateTime::parse(
        "2026-04-08T10:15:00Z",
        &time::format_description::well_known::Rfc3339,
    )
    .expect("initial test timestamp should parse");
    let rotated_period = OffsetDateTime::parse(
        "2026-04-08T10:16:00Z",
        &time::format_description::well_known::Rfc3339,
    )
    .expect("rotated test timestamp should parse");
    let clock = fixed_test_clock(vec![
        initial_period,
        initial_period,
        rotated_period,
        rotated_period,
    ]);

    let mut writer = komga_server::logging::StableFileAppender::new_with_clock(
        config.log_file.clone(),
        komga_server::logging::FileRotation::Minutely,
        clock,
    )
    .expect("stable rotating file appender should be created");

    writer
        .write_all(b"first period line\n")
        .expect("first period write should succeed");
    writer.flush().expect("first period flush should succeed");
    writer
        .write_all(b"second period line\n")
        .expect("second period write should succeed");
    writer.flush().expect("second period flush should succeed");

    let archived_files = sibling_archives_for(&config.log_file);
    println!(
        "stable_logfile_rotation_contract active={} archives={archived_files:?}",
        config.log_file.display(),
    );

    assert_eq!(writer.active_path(), config.log_file.as_path());
    assert_eq!(
        std::fs::read_to_string(&config.log_file)
            .expect("active logfile should remain readable at the configured path"),
        "second period line\n",
    );
    assert_eq!(
        archived_files.len(),
        1,
        "rotation should create one sibling archive"
    );
    assert_eq!(
        std::fs::read_to_string(&archived_files[0])
            .expect("rotated archive should remain readable beside the active file"),
        "first period line\n",
    );
    assert_eq!(
        archived_files[0].parent(),
        config.log_file.parent(),
        "archive should live beside the active logfile rather than replacing it",
    );
    assert_ne!(
        archived_files[0], config.log_file,
        "rotation must archive to a sibling path, not replace RuntimeConfig.log_file",
    );
}
