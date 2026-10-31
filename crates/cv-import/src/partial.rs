//! The importer output (spec section 9): a CV-shaped structure where every
//! field is optional and every item already has a fresh ID. It is shown in the
//! import preview and never written to storage by this crate.

use cv_core::{Link, Section};

#[derive(Debug, Clone, Default, PartialEq)]
pub struct CvPartial {
    pub full_name: Option<String>,
    pub headline: Option<String>,
    pub email: Option<String>,
    pub phone: Option<String>,
    pub location: Option<String>,
    pub links: Vec<Link>,
    pub summary: Option<String>,
    pub sections: Vec<Section>,
    /// A profile photo as the file carried it: JPEG or PNG bytes, at most
    /// [`MAX_PHOTO`]. Not yet decoded or cleaned of metadata; the app does
    /// that before showing or storing it.
    pub photo: Option<Vec<u8>>,
}

/// Largest imported photo, decoded (the photo dialog's limit, ADR-083).
pub const MAX_PHOTO: usize = 5 * 1024 * 1024;

/// Whether `bytes` start like a JPEG or PNG file.
pub fn is_jpeg_or_png(bytes: &[u8]) -> bool {
    bytes.starts_with(&[0xFF, 0xD8, 0xFF]) || bytes.starts_with(b"\x89PNG\r\n\x1a\n")
}

/// Something the importer skipped or had to guess, in words for the user.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportWarning {
    pub message: String,
}

impl ImportWarning {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct ImportResult {
    pub cv_partial: CvPartial,
    pub warnings: Vec<ImportWarning>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn warning_keeps_its_message() {
        assert_eq!(ImportWarning::new("skipped").message, "skipped");
        assert!(ImportResult::default().warnings.is_empty());
    }

    #[test]
    fn photos_are_recognised_by_their_signature() {
        assert!(is_jpeg_or_png(&[0xFF, 0xD8, 0xFF, 0xE0]));
        assert!(is_jpeg_or_png(b"\x89PNG\r\n\x1a\n...."));
        assert!(!is_jpeg_or_png(b"GIF89a"));
        assert!(!is_jpeg_or_png(b""));
    }
}
