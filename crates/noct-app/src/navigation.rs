//! Which addresses the main window may load. The window hands every navigation to `is_app_url`, so
//! a link, a redirect or a script cannot move the window to a page outside the application. Only the
//! packaged frontend is allowed, plus the development server in debug builds.

use tauri::Url;

/// Origins of the packaged frontend. `tauri://localhost` is the custom protocol of macOS and Linux;
/// `http://tauri.localhost` is the rewrite Tauri uses on Windows. `useHttpsScheme` is not set in
/// `tauri.conf.json`, so the `https` form is not an application origin.
const PACKAGED_ORIGINS: &[&str] = &["tauri://localhost", "http://tauri.localhost"];

/// True when `url` is the packaged frontend or `dev_server`, compared by scheme, host and port.
/// `dev_server` is `Some` only in debug builds, where it is the `devUrl` of the configuration.
pub fn is_app_url(url: &Url, dev_server: Option<&Url>) -> bool {
    PACKAGED_ORIGINS
        .iter()
        .filter_map(|origin| Url::parse(origin).ok())
        .any(|origin| same_origin(url, &origin))
        || dev_server.is_some_and(|dev| same_origin(url, dev))
}

fn same_origin(url: &Url, origin: &Url) -> bool {
    url.scheme() == origin.scheme()
        && url.host_str() == origin.host_str()
        && url.port_or_known_default() == origin.port_or_known_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn url(text: &str) -> Url {
        Url::parse(text).expect("a valid test URL")
    }

    #[test]
    fn the_packaged_frontend_is_allowed_on_every_platform_form() {
        for text in [
            "tauri://localhost/",
            "tauri://localhost/index.html",
            "http://tauri.localhost/",
        ] {
            assert!(is_app_url(&url(text), None), "{text} is the application");
        }
    }

    #[test]
    fn external_pages_and_other_schemes_are_refused() {
        for text in [
            "https://example.com/",
            "http://example.com/",
            "https://tauri.localhost/",
            "http://tauri.localhost.example.com/",
            "tauri://example.com/",
            "tauri://localhost.example.com/",
            "file:///etc/passwd",
            "about:blank",
            "data:text/html,<script>alert(1)</script>",
            "javascript:alert(1)",
            "blob:tauri://localhost/00000000-0000-0000-0000-000000000000",
        ] {
            assert!(
                !is_app_url(&url(text), None),
                "{text} must stay outside the window"
            );
        }
    }

    #[test]
    fn the_development_server_is_allowed_only_where_it_is_configured() {
        let dev = url("http://localhost:8080");

        assert!(is_app_url(&url("http://localhost:8080/"), Some(&dev)));
        assert!(!is_app_url(&url("http://localhost:8080/"), None));
        assert!(!is_app_url(&url("http://localhost:9999/"), Some(&dev)));
        assert!(!is_app_url(&url("http://localhost/"), Some(&dev)));
        assert!(!is_app_url(&url("https://localhost:8080/"), Some(&dev)));
    }

    #[test]
    fn the_default_port_of_the_development_server_is_the_same_origin() {
        let dev = url("http://localhost");

        assert!(is_app_url(&url("http://localhost:80/"), Some(&dev)));
    }
}
