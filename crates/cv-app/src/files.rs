//! Backend-owned file dialogs and safe file writes (spec section 10.2).

use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExportKind {
    Pdf,
    Html,
    /// A copy of the whole database (spec section 13).
    Backup,
}

impl ExportKind {
    pub fn extension(self) -> &'static str {
        match self {
            Self::Pdf => "pdf",
            Self::Html => "html",
            Self::Backup => BACKUP_EXTENSION,
        }
    }
}

/// Extension of backup files: a SQLite database.
pub const BACKUP_EXTENSION: &str = "pinaxcv";

/// Implemented by the shell with the platform file dialogs.
pub trait FileDialog: Send + Sync {
    /// Asks the user where to save; `None` when they cancel.
    fn choose_save_path(&self, suggested_file_name: &str, kind: ExportKind) -> Option<PathBuf>;
    /// Asks the user for a file to import, offering files with these
    /// extensions; `None` when they cancel.
    fn choose_open_path(&self, extensions: &[&str]) -> Option<PathBuf>;
}

/// Writes `bytes` to a temporary file next to `path`, then renames it over
/// `path`, so a failed export never leaves a half-written file behind.
///
/// Each call gets its own temporary file, created exclusively, so two exports
/// to the same path cannot share or truncate each other's file. On Unix the
/// temporary file is private (`0600`) until it takes over the destination's
/// permissions when one exists, and the folder is synced after the rename so
/// the new entry survives a crash.
pub fn write_atomically(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let name = path
        .file_name()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "no file name"))?;
    let mut tmp_name = std::ffi::OsString::from(".");
    tmp_name.push(name);
    tmp_name.push(format!(".{}.pinaxcv-tmp", ulid::Ulid::generate()));
    let tmp = path.with_file_name(tmp_name);
    let mut file = create_private(&tmp)?;
    let result = (|| {
        file.write_all(bytes)?;
        if let Ok(existing) = fs::metadata(path) {
            file.set_permissions(existing.permissions())?;
        }
        file.sync_all()?;
        fs::rename(&tmp, path)
    })();
    drop(file);
    if result.is_err() {
        let _ = fs::remove_file(&tmp);
        return result;
    }
    sync_parent(path)
}

/// Lets `write` create a file at a fresh temporary path next to `path`,
/// makes it private (`0600` on Unix), then renames it over `path`, like
/// [`write_atomically`] for writers that must create their own file (such
/// as SQLite's `VACUUM INTO`). The temporary file is removed on failure.
pub fn create_atomically(
    path: &Path,
    write: impl FnOnce(&Path) -> io::Result<()>,
) -> io::Result<()> {
    let name = path
        .file_name()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "no file name"))?;
    let mut tmp_name = std::ffi::OsString::from(".");
    tmp_name.push(name);
    tmp_name.push(format!(".{}.pinaxcv-tmp", ulid::Ulid::generate()));
    let tmp = path.with_file_name(tmp_name);
    let result = (|| {
        write(&tmp)?;
        let file = fs::OpenOptions::new().write(true).open(&tmp)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            file.set_permissions(fs::Permissions::from_mode(0o600))?;
        }
        file.sync_all()?;
        drop(file);
        fs::rename(&tmp, path)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&tmp);
        return result;
    }
    sync_parent(path)
}

#[cfg(unix)]
fn create_private(path: &Path) -> io::Result<fs::File> {
    use std::os::unix::fs::OpenOptionsExt;
    fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)
}

#[cfg(not(unix))]
fn create_private(path: &Path) -> io::Result<fs::File> {
    fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
}

/// Makes the renamed entry durable. Filesystems that cannot sync a folder
/// report `InvalidInput` or `Unsupported`; the file itself is already synced.
#[cfg(unix)]
fn sync_parent(path: &Path) -> io::Result<()> {
    let parent = match path.parent() {
        Some(p) if !p.as_os_str().is_empty() => p,
        _ => Path::new("."),
    };
    match fs::File::open(parent).and_then(|dir| dir.sync_all()) {
        Err(e)
            if matches!(
                e.kind(),
                io::ErrorKind::InvalidInput | io::ErrorKind::Unsupported
            ) =>
        {
            Ok(())
        }
        other => other,
    }
}

/// Windows cannot open a folder as a file without platform flags; NTFS
/// journals the rename itself.
#[cfg(not(unix))]
fn sync_parent(_path: &Path) -> io::Result<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_and_replaces_without_leftovers() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("Jane-Doe-CV.pdf");
        write_atomically(&path, b"first").expect("write");
        write_atomically(&path, b"second").expect("replace");
        assert_eq!(fs::read(&path).expect("read"), b"second");
        let names: Vec<_> = fs::read_dir(dir.path())
            .expect("list")
            .map(|e| e.expect("entry").file_name())
            .collect();
        assert_eq!(names, ["Jane-Doe-CV.pdf"]);
    }

    #[test]
    fn created_files_replace_the_target_and_are_private() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("backup.pinaxcv");
        fs::write(&path, b"old").expect("old");
        create_atomically(&path, |tmp| fs::write(tmp, b"new")).expect("create");
        assert_eq!(fs::read(&path).expect("read"), b"new");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = fs::metadata(&path).expect("meta").permissions().mode();
            assert_eq!(mode & 0o777, 0o600);
        }
        let failed = create_atomically(&path, |tmp| {
            fs::write(tmp, b"partial")?;
            Err(io::Error::other("full disk"))
        });
        assert!(failed.is_err());
        assert_eq!(fs::read(&path).expect("read"), b"new");
        assert_eq!(fs::read_dir(dir.path()).expect("list").count(), 1);
    }

    #[test]
    fn failure_leaves_no_temporary_file() {
        let dir = tempfile::tempdir().expect("tempdir");
        let missing = dir.path().join("no-such-dir").join("cv.pdf");
        assert!(write_atomically(&missing, b"x").is_err());
        assert!(write_atomically(Path::new(""), b"x").is_err());
        assert_eq!(fs::read_dir(dir.path()).expect("list").count(), 0);
    }

    #[test]
    fn a_leftover_temporary_file_does_not_block_or_get_truncated() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("cv.pdf");
        let other = dir.path().join(".cv.pdf.pinaxcv-tmp");
        fs::write(&other, b"other export").expect("seed");
        write_atomically(&path, b"mine").expect("write");
        assert_eq!(fs::read(&path).expect("read"), b"mine");
        assert_eq!(fs::read(&other).expect("other"), b"other export");
    }

    #[cfg(unix)]
    #[test]
    fn new_files_are_private_and_replacements_keep_permissions() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("cv.pdf");
        write_atomically(&path, b"first").expect("write");
        let mode = |p: &Path| fs::metadata(p).expect("meta").permissions().mode() & 0o777;
        assert_eq!(mode(&path), 0o600);
        fs::set_permissions(&path, fs::Permissions::from_mode(0o640)).expect("chmod");
        write_atomically(&path, b"second").expect("replace");
        assert_eq!(mode(&path), 0o640);
        assert_eq!(fs::read(&path).expect("read"), b"second");
    }

    #[test]
    fn extensions() {
        assert_eq!(ExportKind::Pdf.extension(), "pdf");
        assert_eq!(ExportKind::Html.extension(), "html");
    }
}
