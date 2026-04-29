use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use cmake_tidy_config::{
    ConfigError, MainConfiguration, find_configuration, load_configuration_from_file,
};

static NEXT_TEMP_DIR: AtomicU64 = AtomicU64::new(0);

#[test]
fn explicit_standard_file_loads_configuration() {
    let directory = create_temp_dir();
    let path = directory.join("custom.toml");
    fs::write(
        &path,
        "fix = true\nexclude = [\"build\"]\n[format]\nfinal-newline = false\nspace-before-paren = true\n",
    )
    .expect("config should be written");

    let config = load_configuration_from_file(&path).expect("config should load");
    assert_eq!(config.source, Some(path));
    assert!(config.main.fix);
    assert_eq!(config.main.exclude, vec![PathBuf::from("build")]);
    assert!(!config.format.final_newline);
    assert!(config.format.space_before_paren);

    fs::remove_dir_all(&directory).expect("temporary directory should be removed");
}

#[test]
fn explicit_missing_file_reports_read_error() {
    let path = unique_temp_dir().join("missing.toml");
    let error =
        load_configuration_from_file(&path).expect_err("missing config should return read error");
    assert!(matches!(error, ConfigError::ReadFile { .. }));
}

#[test]
fn find_configuration_ignores_invalid_pyproject() {
    let directory = create_temp_dir();
    fs::write(directory.join("pyproject.toml"), "[tool.cmake-tidy\n")
        .expect("pyproject should be written");

    assert_eq!(find_configuration(&directory), None);

    fs::remove_dir_all(&directory).expect("temporary directory should be removed");
}

#[test]
fn absolute_excludes_match_absolute_paths() {
    let excluded = std::env::temp_dir().join("cmake-tidy-absolute-exclude");
    let configuration = MainConfiguration {
        exclude: vec![excluded.clone()],
        ..MainConfiguration::default()
    };

    assert!(configuration.is_path_excluded(&excluded.join("CMakeLists.txt")));
    assert!(!configuration.is_path_excluded(&std::env::temp_dir().join("elsewhere.cmake")));
}

fn create_temp_dir() -> PathBuf {
    let directory = unique_temp_dir();
    if directory.exists() {
        fs::remove_dir_all(&directory).expect("stale temporary directory should be removable");
    }
    fs::create_dir_all(&directory).expect("temporary directory should be created");
    directory
}

fn unique_temp_dir() -> PathBuf {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock should be after UNIX_EPOCH")
        .as_nanos();
    let sequence = NEXT_TEMP_DIR.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!(
        "cmake-tidy-config-test-{}-{timestamp}-{sequence}",
        std::process::id(),
    ))
}
