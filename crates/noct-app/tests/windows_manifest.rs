//! The Windows manifest of the integration tests. The tests that build the application reach Common Controls v6
//! (see `build.rs`), so the manifest they link must request it. These checks fail when the requirement is
//! dropped or the build script stops linking the file; the Windows run of CI proves that the test executables
//! load.

use std::fs;
use std::path::Path;

fn crate_file(relative: &str) -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(relative);
    fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("{} cannot be read: {error}", path.display()))
}

#[test]
fn the_test_manifest_requests_common_controls_v6() {
    let manifest = crate_file("windows/common-controls.manifest");
    assert!(manifest.contains(r#"name="Microsoft.Windows.Common-Controls""#));
    assert!(manifest.contains(r#"version="6.0.0.0""#));
}

#[test]
fn the_build_script_links_the_test_manifest_on_windows() {
    let build = crate_file("build.rs");
    assert!(build.contains("cargo:rustc-link-arg-tests=/MANIFESTINPUT:"));
    assert!(build.contains(r#".join("common-controls.manifest")"#));
}
