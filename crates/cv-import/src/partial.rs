//! The importer output (spec section 9): a CV-shaped structure where every
//! field is optional and every item already has a fresh ID. It is shown in the
//! import preview and never written to storage by this crate.

use cv_core::{Link, PersonalDetails, Section};

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
    /// Optional personal details (ADR-088). An import never switches them on.
    pub personal: PersonalDetails,
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
}
