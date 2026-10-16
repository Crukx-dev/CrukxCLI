use serde::{de::DeserializeOwned, Serialize};
use std::fmt;
use std::fs;
use std::path::Path;

#[derive(Debug)]
pub enum StorageError {
    Io(std::io::Error),
    Json(serde_json::Error),
    Yaml(serde_yaml::Error),
}

impl fmt::Display for StorageError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            StorageError::Io(err) => write!(f, "io error: {err}"),
            StorageError::Json(err) => write!(f, "json error: {err}"),
            StorageError::Yaml(err) => write!(f, "yaml error: {err}"),
        }
    }
}

impl std::error::Error for StorageError {}

impl From<std::io::Error> for StorageError {
    fn from(err: std::io::Error) -> Self {
        StorageError::Io(err)
    }
}

impl From<serde_json::Error> for StorageError {
    fn from(err: serde_json::Error) -> Self {
        StorageError::Json(err)
    }
}

impl From<serde_yaml::Error> for StorageError {
    fn from(err: serde_yaml::Error) -> Self {
        StorageError::Yaml(err)
    }
}

pub fn read_json<T: DeserializeOwned>(path: &Path) -> Result<T, StorageError> {
    let raw = fs::read_to_string(path)?;
    Ok(serde_json::from_str(&raw)?)
}

/// `None` when the file doesn't exist — distinguishes "not tracked for
/// this run" from a real I/O failure, so callers can treat missing
/// optional state (a judge record, a security scan) as "not recorded"
/// rather than an error.
pub fn read_json_if_exists<T: DeserializeOwned>(path: &Path) -> Result<Option<T>, StorageError> {
    match fs::read_to_string(path) {
        Ok(raw) => Ok(Some(serde_json::from_str(&raw)?)),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(err) => Err(err.into()),
    }
}

pub fn write_json<T: Serialize>(path: &Path, value: &T) -> Result<(), StorageError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let raw = serde_json::to_string_pretty(value)?;
    fs::write(path, raw)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;

    #[derive(Debug, Serialize, Deserialize, PartialEq)]
    struct Sample {
        name: String,
        count: u32,
    }

    #[test]
    fn write_json_then_read_json_round_trips() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nested").join("sample.json");
        let value = Sample {
            name: "x".to_string(),
            count: 3,
        };

        write_json(&path, &value).unwrap();
        let read_back: Sample = read_json(&path).unwrap();
        assert_eq!(read_back, value);
    }

    #[test]
    fn write_json_creates_missing_parent_directories() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a").join("b").join("c.json");
        write_json(
            &path,
            &Sample {
                name: "x".to_string(),
                count: 1,
            },
        )
        .unwrap();
        assert!(path.exists());
    }

    #[test]
    fn read_json_if_exists_returns_none_for_a_missing_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("missing.json");
        let result: Option<Sample> = read_json_if_exists(&path).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn read_json_if_exists_returns_some_for_a_present_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("present.json");
        write_json(
            &path,
            &Sample {
                name: "y".to_string(),
                count: 2,
            },
        )
        .unwrap();
        let result: Option<Sample> = read_json_if_exists(&path).unwrap();
        assert_eq!(
            result,
            Some(Sample {
                name: "y".to_string(),
                count: 2
            })
        );
    }

    #[test]
    fn read_json_surfaces_a_parse_error_for_malformed_content() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("bad.json");
        fs::write(&path, "{not json").unwrap();
        let result: Result<Sample, StorageError> = read_json(&path);
        assert!(matches!(result, Err(StorageError::Json(_))));
    }
}
