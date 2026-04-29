use std::fs;
use std::path::Path;

use super::ConfigError;

pub fn read_file(path: &Path) -> Result<String, ConfigError> {
    fs::read_to_string(path).map_err(|source| ConfigError::ReadFile {
        path: path.to_path_buf(),
        source,
    })
}
