use std::fs::{self, DirEntry, Metadata, ReadDir};
use std::path::Path;

use anyhow::{Context, Result};

pub fn read_cmake_file(path: &Path) -> Result<String> {
    fs::read_to_string(path)
        .with_context(|| format!("failed to read CMake file: {}", path.display()))
}

pub fn write_fixed_file(path: &Path, fixed: &str) -> Result<()> {
    fs::write(path, fixed)
        .with_context(|| format!("failed to write fixed file: {}", path.display()))
}

pub fn write_formatted_file(path: &Path, output: String) -> Result<()> {
    fs::write(path, output)
        .with_context(|| format!("failed to write formatted file: {}", path.display()))
}

pub fn read_metadata(path: &Path) -> Result<Metadata> {
    fs::metadata(path).with_context(|| format!("failed to read file metadata: {}", path.display()))
}

pub fn read_directory(path: &Path) -> Result<ReadDir> {
    fs::read_dir(path).with_context(|| format!("failed to read directory: {}", path.display()))
}

pub fn read_directory_entry(entry: std::io::Result<DirEntry>, path: &Path) -> Result<DirEntry> {
    entry.with_context(|| format!("failed to read entry in {}", path.display()))
}
