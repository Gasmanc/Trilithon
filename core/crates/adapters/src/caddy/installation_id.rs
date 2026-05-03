//! Read-or-create the persistent daemon installation identifier.
//!
//! The installation id is a UUID v4 stored as a hyphenated string in
//! `<data_dir>/installation_id`.  If the file does not yet exist it is
//! generated and written atomically (write to `<file>.tmp` then rename).

use std::{
    fs,
    io::{self, Write as _},
    path::Path,
};

/// Read the installation id from `<data_dir>/installation_id`, or generate and
/// persist a fresh UUID v4 if the file does not exist.
///
/// The write is atomic: the UUID is written to a sibling `.tmp` file and then
/// renamed into place so a crash mid-write cannot corrupt the stored value.
///
/// # Errors
///
/// Returns `Err` if any filesystem operation fails.
pub fn read_or_create(data_dir: &Path) -> Result<String, io::Error> {
    let id_path = data_dir.join("installation_id");
    let tmp_path = data_dir.join("installation_id.tmp");

    // Attempt the read first; only generate a new id on NotFound.
    // This avoids a TOCTOU window that would exist if we checked existence
    // before reading.
    match fs::read_to_string(&id_path) {
        Ok(raw) => {
            let trimmed = raw.trim().to_owned();
            // Validate that the stored value is a plausible UUID v4 (hyphenated
            // 8-4-4-4-12 format).  An empty or corrupted file would produce an
            // invalid id that silently propagates into every downstream record.
            if uuid::Uuid::parse_str(&trimmed).is_ok() {
                return Ok(trimmed);
            }
            // Treat an invalid stored value as absent and regenerate.
        }
        Err(e) if e.kind() == io::ErrorKind::NotFound => {}
        Err(e) => return Err(e),
    }

    let id = uuid::Uuid::new_v4().hyphenated().to_string();

    // Ensure the data directory exists before creating the tmp file.
    fs::create_dir_all(data_dir)?;

    let mut file = fs::File::create(&tmp_path)?;
    file.write_all(id.as_bytes())?;
    file.flush()?;
    // Flush to the OS page cache then sync to hardware so a crash between
    // write and rename cannot leave a directory entry pointing to an empty
    // or partial file.
    file.sync_all()?;
    drop(file);

    if let Err(e) = fs::rename(&tmp_path, &id_path) {
        // Best-effort cleanup of the tmp file; ignore secondary errors.
        let _ = fs::remove_file(&tmp_path);
        return Err(e);
    }

    Ok(id)
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::disallowed_methods
)]
// reason: test-only code; panics are the correct failure mode in tests
mod tests {
    use super::read_or_create;

    #[test]
    fn creates_on_first_call_and_returns_same_on_second() {
        let dir = tempfile::tempdir().expect("tempdir");
        let id1 = read_or_create(dir.path()).expect("first call");
        let id2 = read_or_create(dir.path()).expect("second call");
        assert_eq!(id1, id2, "id must be stable across calls");
        // Validate UUID v4 hyphenated format (8-4-4-4-12).
        assert_eq!(id1.len(), 36);
        assert_eq!(id1.chars().filter(|&c| c == '-').count(), 4);
    }

    #[test]
    fn file_contains_uuid_without_newline() {
        let dir = tempfile::tempdir().expect("tempdir");
        let id = read_or_create(dir.path()).expect("call");
        let raw = std::fs::read_to_string(dir.path().join("installation_id")).expect("read");
        assert_eq!(raw.trim(), id.as_str());
    }
}
