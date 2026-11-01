//! The platform file dialogs, opened from Rust so the UI never handles paths.

use std::path::PathBuf;

use cv_app::{ExportKind, FileDialog};
use tauri::{AppHandle, Wry};
use tauri_plugin_dialog::DialogExt;

pub struct TauriFileDialog {
    handle: AppHandle<Wry>,
}

impl TauriFileDialog {
    pub fn new(handle: AppHandle<Wry>) -> Self {
        Self { handle }
    }
}

/// Called from the blocking pool, never the main thread, so the blocking
/// dialog calls cannot freeze the window.
impl FileDialog for TauriFileDialog {
    fn choose_save_path(&self, suggested_file_name: &str, kind: ExportKind) -> Option<PathBuf> {
        let (label, ext) = match kind {
            ExportKind::Pdf => ("PDF document", kind.extension()),
            ExportKind::Html => ("Web page", kind.extension()),
            ExportKind::Backup => ("PinaxCV backup", kind.extension()),
            ExportKind::Style => ("PinaxCV style", kind.extension()),
        };
        self.handle
            .dialog()
            .file()
            .set_file_name(suggested_file_name)
            .add_filter(label, &[ext])
            .blocking_save_file()
            .and_then(|p| p.into_path().ok())
    }

    fn choose_open_path(&self, extensions: &[&str]) -> Option<PathBuf> {
        let label = if extensions == [cv_app::BACKUP_EXTENSION] {
            "PinaxCV backup"
        } else if extensions == cv_app::PHOTO_EXTENSIONS {
            "Photo"
        } else if extensions == cv_app::STYLE_EXTENSIONS {
            "PinaxCV style"
        } else {
            "CV file"
        };
        self.handle
            .dialog()
            .file()
            .add_filter(label, extensions)
            .blocking_pick_file()
            .and_then(|p| p.into_path().ok())
    }
}

/// End-to-end test builds only (`--features e2e`): WebDriver cannot drive a
/// native dialog, so exports go straight into the folder named by
/// `PINAXCV_E2E_EXPORT_DIR`, and imports read `PINAXCV_E2E_IMPORT_FILE`.
/// Release builds never enable the feature.
#[cfg(feature = "e2e")]
pub struct E2eFileDialog {
    dir: PathBuf,
    import: Option<PathBuf>,
}

#[cfg(feature = "e2e")]
impl E2eFileDialog {
    /// `None` when `PINAXCV_E2E_EXPORT_DIR` is not set.
    pub fn from_env() -> Option<Self> {
        std::env::var_os("PINAXCV_E2E_EXPORT_DIR").map(|dir| Self {
            dir: dir.into(),
            import: std::env::var_os("PINAXCV_E2E_IMPORT_FILE").map(PathBuf::from),
        })
    }
}

#[cfg(feature = "e2e")]
impl FileDialog for E2eFileDialog {
    fn choose_save_path(&self, suggested_file_name: &str, _: ExportKind) -> Option<PathBuf> {
        Some(self.dir.join(suggested_file_name))
    }

    fn choose_open_path(&self, _: &[&str]) -> Option<PathBuf> {
        self.import.clone()
    }
}
