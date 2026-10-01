use collector_domain::ApiError;
use std::{
    fs::{self, File, OpenOptions, TryLockError},
    path::Path,
};

pub struct ServeLease {
    _file: File,
}

impl ServeLease {
    pub fn acquire(data_dir: &Path) -> Result<Self, ApiError> {
        fs::create_dir_all(data_dir).map_err(|_| data_directory_unavailable())?;

        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(data_dir.join("collector.serve.lock"))
            .map_err(|_| data_directory_lock_failed())?;

        file.try_lock().map_err(|error| match error {
            TryLockError::WouldBlock => data_directory_in_use(),
            TryLockError::Error(_) => data_directory_lock_failed(),
        })?;

        Ok(Self { _file: file })
    }
}

fn data_directory_unavailable() -> ApiError {
    ApiError::new(
        "data_directory_unavailable",
        "Cannot create or access the data directory.",
    )
}

fn data_directory_in_use() -> ApiError {
    ApiError::new(
        "core_data_directory_in_use",
        "The data directory is already in use by another Core host.",
    )
}

fn data_directory_lock_failed() -> ApiError {
    ApiError::new(
        "core_data_directory_lock_failed",
        "Cannot acquire the data directory lease.",
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn a_second_handle_cannot_acquire_the_same_directory() {
        let temp = tempdir().expect("create temporary directory");
        let _lease = ServeLease::acquire(temp.path()).expect("acquire first lease");

        let error = ServeLease::acquire(temp.path())
            .err()
            .expect("a distinct handle must not acquire the same lease");

        assert_eq!(error.code, "core_data_directory_in_use");
        assert_eq!(
            error.message,
            "The data directory is already in use by another Core host."
        );
    }

    #[test]
    fn dropping_the_handle_releases_the_lease() {
        let temp = tempdir().expect("create temporary directory");
        let lease = ServeLease::acquire(temp.path()).expect("acquire first lease");

        drop(lease);

        let _replacement =
            ServeLease::acquire(temp.path()).expect("reacquire lease after handle drop");
    }

    #[test]
    fn different_directories_can_be_leased_together() {
        let first = tempdir().expect("create first temporary directory");
        let second = tempdir().expect("create second temporary directory");

        let _first_lease = ServeLease::acquire(first.path()).expect("acquire first lease");
        let _second_lease = ServeLease::acquire(second.path()).expect("acquire second lease");
    }

    #[test]
    fn acquire_creates_nested_data_directories() {
        let temp = tempdir().expect("create temporary directory");
        let nested = temp.path().join("nested").join("data");

        let _lease = ServeLease::acquire(&nested).expect("acquire lease in nested directory");

        assert!(nested.is_dir());
        assert!(nested.join("collector.serve.lock").is_file());
    }

    #[test]
    fn a_stale_empty_lock_file_does_not_remain_busy() {
        let temp = tempdir().expect("create temporary directory");
        let lock_path = temp.path().join("collector.serve.lock");
        fs::write(&lock_path, []).expect("create stale empty lock file");

        let _lease = ServeLease::acquire(temp.path()).expect("acquire stale lock file");
    }

    #[test]
    fn acquiring_the_lease_does_not_truncate_existing_content() {
        let temp = tempdir().expect("create temporary directory");
        let lock_path = temp.path().join("collector.serve.lock");
        let original = b"existing diagnostic content";
        fs::write(&lock_path, original).expect("write existing lock file content");

        let lease = ServeLease::acquire(temp.path()).expect("acquire existing lock file");
        drop(lease);

        assert_eq!(
            fs::read(lock_path).expect("read released lock file"),
            original
        );
    }

    #[test]
    fn filesystem_errors_are_sanitized_before_any_database_mutation() {
        let temp = tempdir().expect("create temporary directory");
        let lock_path = temp.path().join("collector.serve.lock");
        fs::create_dir(&lock_path).expect("make lock path impossible to open as a file");

        let error = ServeLease::acquire(temp.path())
            .err()
            .expect("opening a directory as the lock file must fail");

        assert_eq!(error.code, "core_data_directory_lock_failed");
        assert_eq!(error.message, "Cannot acquire the data directory lease.");
        assert!(!error.message.contains(&temp.path().display().to_string()));
        assert!(!temp.path().join("collector.db").exists());
    }

    #[test]
    fn directory_creation_errors_are_sanitized_before_any_database_mutation() {
        let temp = tempdir().expect("create temporary directory");
        let data_dir = temp.path().join("not-a-directory");
        fs::write(&data_dir, b"occupied").expect("create file at data-directory path");

        let error = ServeLease::acquire(&data_dir)
            .err()
            .expect("creating a directory over a file must fail");

        assert_eq!(error.code, "data_directory_unavailable");
        assert_eq!(error.message, "Cannot create or access the data directory.");
        assert!(!error.message.contains(&data_dir.display().to_string()));
        assert!(!data_dir.join("collector.db").exists());
    }
}
