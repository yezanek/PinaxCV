//! Backend-owned save dialogs and safe file writes (spec section 10.2).

use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExportKind {
    Pdf,
    Html,
}

impl ExportKind {
    pub fn extension(self) -> &'static str {
        match self {
            Self::Pdf => "pdf",
            Self::Html => "html",
        }
    }
}

/// Implemented by the shell with the platform save dialog.
pub trait SaveDialog: Send + Sync {
    /// Asks the user where to save; `None` when they cancel.
    fn choose_save_path(&self, suggested_file_name: &str, kind: ExportKind) -> Option<PathBuf>;
}

/// Writes `bytes` to a temporary file next to `path`, then renames it over
/// `path`, so a failed export never leaves a half-written file behind.
pub fn write_atomically(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let name = path
        .file_name()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "no file name"))?;
    let mut tmp_name = std::ffi::OsString::from(".");
    tmp_name.push(name);
    tmp_name.push(".pinaxcv-tmp");
    let tmp = path.with_file_name(tmp_name);
    let result = (|| {
        let mut file = fs::File::create(&tmp)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        drop(file);
        fs::rename(&tmp, path)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    result
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
    fn failure_leaves_no_temporary_file() {
        let dir = tempfile::tempdir().expect("tempdir");
        let missing = dir.path().join("no-such-dir").join("cv.pdf");
        assert!(write_atomically(&missing, b"x").is_err());
        assert!(write_atomically(Path::new(""), b"x").is_err());
        assert_eq!(fs::read_dir(dir.path()).expect("list").count(), 0);
    }

    #[test]
    fn extensions() {
        assert_eq!(ExportKind::Pdf.extension(), "pdf");
        assert_eq!(ExportKind::Html.extension(), "html");
    }
}
