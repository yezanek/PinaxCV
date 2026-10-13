//! The page-counting seam used by fit-to-length (spec section 4.6).

use crate::ResolvedCv;

/// Counts the pages a resolved CV renders to. Implemented by `cv-render`;
/// tests use a fake.
pub trait PageCounter {
    fn pages(&self, cv: &ResolvedCv) -> Result<u32, PageCountError>;
}

/// Rendering failed while counting pages.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[error("could not count pages: {0}")]
pub struct PageCountError(pub String);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn page_count_error_message_includes_cause() {
        let err = PageCountError("template failed to compile".into());
        assert_eq!(
            err.to_string(),
            "could not count pages: template failed to compile"
        );
    }
}
