//! Maps storage and rendering errors to the `ApiError` the UI shows.

use cv_api::{ApiError, ErrorCode};
use cv_render::RenderError;
use cv_store::StoreError;

/// Never includes CV content or driver errors in the message.
pub fn store_error(e: StoreError) -> ApiError {
    match e {
        StoreError::NotFound(_) => ApiError::new(
            ErrorCode::NotFound,
            "That CV or variant no longer exists. It may have been deleted.",
        ),
        StoreError::Conflict { current_revision } => ApiError::conflict(current_revision),
        e @ (StoreError::NewerSchema { .. } | StoreError::Invalid(_) | StoreError::Backend(_)) => {
            ApiError::new(ErrorCode::Db, e.to_string())
        }
    }
}

pub fn render_error(e: RenderError) -> ApiError {
    match e {
        RenderError::Validation(issues) => ApiError::new(
            ErrorCode::Validation,
            "The CV is not ready to export. Fix the listed fields first.",
        )
        .with_details(serde_json::to_value(issues).unwrap_or_default()),
        RenderError::Compile(diagnostics) => ApiError::new(
            ErrorCode::Render,
            "The template could not be rendered. Please report this with the CV attached.",
        )
        .with_details(serde_json::to_value(diagnostics).unwrap_or_default()),
        e @ (RenderError::Export(_) | RenderError::Extract(_)) => {
            ApiError::new(ErrorCode::Render, e.to_string())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cv_core::Id;

    #[test]
    fn store_errors_map_to_codes() {
        let id = Id::parse("01HZX3M8Q4V7K2N9P5R6T0W1YB").expect("id");
        assert_eq!(
            store_error(StoreError::NotFound(id)).code,
            ErrorCode::NotFound
        );
        let conflict = store_error(StoreError::Conflict {
            current_revision: 4,
        });
        assert_eq!(conflict, ApiError::conflict(4));
        let backend = store_error(StoreError::Backend("disk I/O error near 'Jane'".into()));
        assert_eq!(backend.code, ErrorCode::Db);
        assert!(!backend.message.contains("Jane"), "{}", backend.message);
        assert_eq!(
            store_error(StoreError::NewerSchema {
                found: 2,
                supported: 1
            })
            .code,
            ErrorCode::Db
        );
    }

    #[test]
    fn render_errors_map_to_codes_with_details() {
        let e = render_error(RenderError::Compile(vec![]));
        assert_eq!(
            (e.code, e.details),
            (ErrorCode::Render, Some(serde_json::json!([])))
        );
        assert_eq!(
            render_error(RenderError::Validation(vec![])).code,
            ErrorCode::Validation
        );
        assert_eq!(
            render_error(RenderError::Export("x".into())).code,
            ErrorCode::Render
        );
    }
}
