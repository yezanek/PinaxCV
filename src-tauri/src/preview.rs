//! The `preview://` scheme: serves raw PNG page bytes from the newest preview
//! (spec section 10.3, ADR-025). Paths are `/{revision}/{page}.png`.

use cv_app::App;
use tauri::http::{Response, StatusCode};

/// What the webview uses to reach the scheme on each platform.
#[cfg(windows)]
pub const BASE_URL: &str = "http://preview.localhost";
#[cfg(not(windows))]
pub const BASE_URL: &str = "preview://localhost";

/// Parses `/{revision}/{page}.png`.
pub fn parse_path(path: &str) -> Option<(u64, u32)> {
    let rest = path.strip_prefix('/')?;
    let (revision, page) = rest.split_once('/')?;
    let page = page.strip_suffix(".png")?;
    Some((revision.parse().ok()?, page.parse().ok()?))
}

/// 200 with the PNG, 404 for an unknown or replaced page, 503 before the
/// app has finished starting.
pub fn respond(app: Option<&App>, path: &str) -> Response<Vec<u8>> {
    let (status, body) = match app {
        None => (StatusCode::SERVICE_UNAVAILABLE, Vec::new()),
        Some(app) => match parse_path(path).and_then(|(r, p)| app.preview_page(r, p)) {
            Some(png) => (StatusCode::OK, png),
            None => (StatusCode::NOT_FOUND, Vec::new()),
        },
    };
    let mut response = Response::new(body);
    *response.status_mut() = status;
    let headers = response.headers_mut();
    headers.insert(
        "Content-Type",
        tauri::http::HeaderValue::from_static("image/png"),
    );
    headers.insert(
        "Cache-Control",
        tauri::http::HeaderValue::from_static("no-store"),
    );
    response
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_revision_and_page() {
        assert_eq!(parse_path("/12/3.png"), Some((12, 3)));
        for bad in [
            "",
            "/12",
            "/12/3",
            "/x/3.png",
            "/1/2/3.png",
            "12/3.png",
            "/-1/1.png",
        ] {
            assert_eq!(parse_path(bad), None, "{bad}");
        }
    }

    #[test]
    fn missing_app_or_page_is_not_ok() {
        assert_eq!(
            respond(None, "/1/1.png").status(),
            StatusCode::SERVICE_UNAVAILABLE
        );
        let r = respond(None, "/1/1.png");
        assert_eq!(r.headers()["Cache-Control"], "no-store");
    }
}
