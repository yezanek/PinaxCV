//! The platform save dialog, opened from Rust so the UI never handles paths.

use std::path::PathBuf;

use cv_app::{ExportKind, SaveDialog};
use tauri::{AppHandle, Wry};
use tauri_plugin_dialog::DialogExt;

pub struct TauriSaveDialog {
    handle: AppHandle<Wry>,
}

impl TauriSaveDialog {
    pub fn new(handle: AppHandle<Wry>) -> Self {
        Self { handle }
    }
}

impl SaveDialog for TauriSaveDialog {
    /// Called from the blocking pool, never the main thread, so the blocking
    /// dialog call cannot freeze the window.
    fn choose_save_path(&self, suggested_file_name: &str, kind: ExportKind) -> Option<PathBuf> {
        let (label, ext) = match kind {
            ExportKind::Pdf => ("PDF document", kind.extension()),
            ExportKind::Html => ("Web page", kind.extension()),
        };
        self.handle
            .dialog()
            .file()
            .set_file_name(suggested_file_name)
            .add_filter(label, &[ext])
            .blocking_save_file()
            .and_then(|p| p.into_path().ok())
    }
}
