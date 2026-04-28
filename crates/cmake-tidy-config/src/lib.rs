use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};
use std::str::FromStr;

use serde::Deserialize;
use thiserror::Error;

const CONFIG_FILENAMES: [&str; 3] = ["cmake-tidy.toml", ".cmake-tidy.toml", "pyproject.toml"];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Configuration {
    pub source: Option<PathBuf>,
    pub main: MainConfiguration,
    pub lint: LintConfiguration,
    pub format: FormatConfiguration,
}

impl Default for Configuration {
    fn default() -> Self {
        Self {
            source: None,
            main: MainConfiguration::default(),
            lint: LintConfiguration::default(),
            format: FormatConfiguration::default(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct MainConfiguration {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LintConfiguration {
    pub select: Vec<RuleSelector>,
    pub ignore: Vec<RuleSelector>,
}

impl Default for LintConfiguration {
    fn default() -> Self {
        Self {
            select: vec![RuleSelector::prefix("E"), RuleSelector::prefix("W")],
            ignore: Vec::new(),
        }
    }
}

impl LintConfiguration {
    #[must_use]
    pub fn is_rule_enabled(&self, code: &str) -> bool {
        let selected = self
            .select
            .iter()
            .any(|selector| selector.matches(code));
        let ignored = self
            .ignore
            .iter()
            .any(|selector| selector.matches(code));
        selected && !ignored
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct FormatConfiguration {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RuleSelector {
    All,
    Prefix(String),
}

impl RuleSelector {
    #[must_use]
    pub fn prefix(prefix: impl Into<String>) -> Self {
        Self::Prefix(prefix.into())
    }

    #[must_use]
    pub fn matches(&self, code: &str) -> bool {
        match self {
            Self::All => true,
            Self::Prefix(prefix) => code.starts_with(prefix),
        }
    }
}

impl fmt::Display for RuleSelector {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::All => formatter.write_str("ALL"),
            Self::Prefix(prefix) => formatter.write_str(prefix),
        }
    }
}

impl FromStr for RuleSelector {
    type Err = ConfigError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        if value == "ALL" {
            return Ok(Self::All);
        }

        if value.is_empty() || !value.chars().all(|character| character.is_ascii_uppercase() || character.is_ascii_digit()) {
            return Err(ConfigError::InvalidRuleSelector(value.to_owned()));
        }

        Ok(Self::Prefix(value.to_owned()))
    }
}

impl<'de> Deserialize<'de> for RuleSelector {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        RuleSelector::from_str(&value).map_err(serde::de::Error::custom)
    }
}

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("failed to read configuration file `{path}`")]
    ReadFile {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("failed to parse configuration file `{path}`")]
    ParseToml {
        path: PathBuf,
        #[source]
        source: toml::de::Error,
    },
    #[error("`pyproject.toml` does not contain a `[tool.cmake-tidy]` section")]
    MissingPyprojectSection,
    #[error("invalid rule selector `{0}`")]
    InvalidRuleSelector(String),
}

#[must_use]
pub fn find_configuration(directory: &Path) -> Option<PathBuf> {
    for filename in CONFIG_FILENAMES {
        let path = directory.join(filename);
        if !path.is_file() {
            continue;
        }

        if filename != "pyproject.toml" || pyproject_has_section(&path).ok().flatten().is_some() {
            return Some(path);
        }
    }

    None
}

pub fn load_configuration(directory: &Path) -> Result<Configuration, ConfigError> {
    for filename in CONFIG_FILENAMES {
        let path = directory.join(filename);
        if !path.is_file() {
            continue;
        }

        if filename == "pyproject.toml" {
            if pyproject_has_section(&path)?.is_none() {
                continue;
            }
            let content = read_file(&path)?;
            let raw = parse_pyproject(&content, &path)?;
            return Ok(normalize_configuration(raw, Some(path)));
        }

        let content = read_file(&path)?;
        let raw = parse_standard_file(&content, &path)?;
        return Ok(normalize_configuration(raw, Some(path)));
    }

    Ok(Configuration::default())
}

pub fn load_configuration_from_file(path: &Path) -> Result<Configuration, ConfigError> {
    let content = read_file(path)?;
    let raw = if path.file_name().is_some_and(|filename| filename == "pyproject.toml") {
        parse_pyproject(&content, path)?
    } else {
        parse_standard_file(&content, path)?
    };

    Ok(normalize_configuration(raw, Some(path.to_path_buf())))
}

fn read_file(path: &Path) -> Result<String, ConfigError> {
    fs::read_to_string(path).map_err(|source| ConfigError::ReadFile {
        path: path.to_path_buf(),
        source,
    })
}

fn parse_standard_file(content: &str, path: &Path) -> Result<RawConfiguration, ConfigError> {
    toml::from_str(content).map_err(|source| ConfigError::ParseToml {
        path: path.to_path_buf(),
        source,
    })
}

fn parse_pyproject(content: &str, path: &Path) -> Result<RawConfiguration, ConfigError> {
    let pyproject = toml::from_str::<Pyproject>(content).map_err(|source| ConfigError::ParseToml {
        path: path.to_path_buf(),
        source,
    })?;

    pyproject
        .tool
        .and_then(|tool| tool.cmake_tidy)
        .ok_or(ConfigError::MissingPyprojectSection)
}

fn pyproject_has_section(path: &Path) -> Result<Option<RawConfiguration>, ConfigError> {
    let content = read_file(path)?;
    match parse_pyproject(&content, path) {
        Ok(configuration) => Ok(Some(configuration)),
        Err(ConfigError::MissingPyprojectSection) => Ok(None),
        Err(error) => Err(error),
    }
}

fn normalize_configuration(raw: RawConfiguration, source: Option<PathBuf>) -> Configuration {
    Configuration {
        source,
        main: MainConfiguration::default(),
        lint: LintConfiguration {
            select: raw
                .lint
                .select
                .unwrap_or_else(|| LintConfiguration::default().select),
            ignore: raw.lint.ignore.unwrap_or_default(),
        },
        format: raw.format.into(),
    }
}

#[derive(Debug, Deserialize, Default)]
struct RawConfiguration {
    #[serde(default)]
    lint: RawLintConfiguration,
    #[serde(default)]
    format: RawFormatConfiguration,
}

#[derive(Debug, Deserialize, Default)]
struct RawLintConfiguration {
    select: Option<Vec<RuleSelector>>,
    ignore: Option<Vec<RuleSelector>>,
}

#[derive(Debug, Deserialize, Default)]
struct RawFormatConfiguration {}

impl From<RawFormatConfiguration> for FormatConfiguration {
    fn from(_value: RawFormatConfiguration) -> Self {
        Self::default()
    }
}

#[derive(Debug, Deserialize)]
struct Pyproject {
    tool: Option<PyprojectTool>,
}

#[derive(Debug, Deserialize)]
struct PyprojectTool {
    #[serde(rename = "cmake-tidy")]
    cmake_tidy: Option<RawConfiguration>,
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::{
        ConfigError, LintConfiguration, RuleSelector, find_configuration, load_configuration,
        load_configuration_from_file,
    };

    static NEXT_TEMP_DIR: AtomicU64 = AtomicU64::new(0);

    #[test]
    fn defaults_select_error_and_warning_rules() {
        let config = load_configuration(&unique_temp_dir()).expect("default config should load");
        assert!(config.lint.is_rule_enabled("E001"));
        assert!(config.lint.is_rule_enabled("W302"));
        assert!(!config.lint.is_rule_enabled("B900"));
    }

    #[test]
    fn parses_standard_toml_configuration() {
        let directory = create_temp_dir();
        write_file(
            &directory.join("cmake-tidy.toml"),
            "[lint]\nselect = [\"ALL\"]\nignore = [\"W2\"]\n",
        );

        let config = load_configuration(&directory).expect("config should parse");
        assert_eq!(config.source, Some(directory.join("cmake-tidy.toml")));
        assert!(config.lint.is_rule_enabled("E001"));
        assert!(!config.lint.is_rule_enabled("W201"));
        assert!(config.lint.is_rule_enabled("W301"));
    }

    #[test]
    fn parses_hidden_toml_configuration() {
        let directory = create_temp_dir();
        write_file(
            &directory.join(".cmake-tidy.toml"),
            "[lint]\nselect = [\"W301\"]\n",
        );

        let config = load_configuration(&directory).expect("hidden config should parse");
        assert_eq!(config.source, Some(directory.join(".cmake-tidy.toml")));
        assert!(config.lint.is_rule_enabled("W301"));
        assert!(!config.lint.is_rule_enabled("W302"));
    }

    #[test]
    fn parses_pyproject_configuration() {
        let directory = create_temp_dir();
        write_file(
            &directory.join("pyproject.toml"),
            "[tool.cmake-tidy.lint]\nselect = [\"W3\"]\nignore = [\"W302\"]\n",
        );

        let config = load_configuration(&directory).expect("pyproject config should parse");
        assert_eq!(config.source, Some(directory.join("pyproject.toml")));
        assert!(config.lint.is_rule_enabled("W301"));
        assert!(!config.lint.is_rule_enabled("W302"));
        assert!(!config.lint.is_rule_enabled("E001"));
    }

    #[test]
    fn discovers_configuration_in_precedence_order() {
        let directory = create_temp_dir();
        write_file(&directory.join("pyproject.toml"), "[tool.cmake-tidy.lint]\nselect = [\"ALL\"]\n");
        write_file(&directory.join(".cmake-tidy.toml"), "[lint]\nselect = [\"W\"]\n");
        write_file(&directory.join("cmake-tidy.toml"), "[lint]\nselect = [\"E\"]\n");

        assert_eq!(
            find_configuration(&directory),
            Some(directory.join("cmake-tidy.toml"))
        );

        let config = load_configuration(&directory).expect("preferred config should parse");
        assert!(config.lint.is_rule_enabled("E001"));
        assert!(!config.lint.is_rule_enabled("W301"));
    }

    #[test]
    fn explicit_pyproject_without_section_errors() {
        let directory = create_temp_dir();
        let path = directory.join("pyproject.toml");
        write_file(&path, "[tool.other]\nvalue = true\n");

        let error = load_configuration_from_file(&path).expect_err("pyproject should require tool.cmake-tidy");
        assert!(matches!(error, ConfigError::MissingPyprojectSection));
    }

    #[test]
    fn pyproject_without_tool_section_is_ignored_during_discovery() {
        let directory = create_temp_dir();
        write_file(&directory.join("pyproject.toml"), "[tool.other]\nvalue = true\n");

        let config = load_configuration(&directory).expect("missing tool section should be ignored");
        assert_eq!(config.source, None);
        assert!(config.lint.is_rule_enabled("E001"));
    }

    #[test]
    fn invalid_selector_is_rejected() {
        let directory = create_temp_dir();
        write_file(&directory.join("cmake-tidy.toml"), "[lint]\nselect = [\"e\"]\n");

        let error = load_configuration(&directory).expect_err("invalid selector should fail");
        assert!(matches!(error, ConfigError::ParseToml { .. }));
    }

    #[test]
    fn ignore_overrides_selected_rules() {
        let lint = LintConfiguration {
            select: vec![RuleSelector::All],
            ignore: vec![RuleSelector::prefix("W2")],
        };

        assert!(lint.is_rule_enabled("E001"));
        assert!(!lint.is_rule_enabled("W201"));
    }

    fn create_temp_dir() -> PathBuf {
        let directory = unique_temp_dir();
        if directory.exists() {
            fs::remove_dir_all(&directory).expect("stale temporary directory should be removable");
        }
        fs::create_dir_all(&directory).expect("temporary directory should be created");
        directory
    }

    fn write_file(path: &Path, content: &str) {
        fs::write(path, content).expect("temporary file should be written");
    }

    fn unique_temp_dir() -> PathBuf {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock should be after UNIX_EPOCH")
            .as_nanos();
        let sequence = NEXT_TEMP_DIR.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir().join(format!(
            "cmake-tidy-config-{}-{timestamp}-{sequence}",
            std::process::id(),
        ))
    }
}
