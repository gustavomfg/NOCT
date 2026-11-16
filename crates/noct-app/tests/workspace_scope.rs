//! The workspace inspection, against the availabilities and messages of `WorkspaceTrust.ts`. Every folder
//! made here is inside a scratch directory.

mod common;

use std::path::{Path, PathBuf};

use common::Scratch;
use noct_app::workspace_scope::{default_blocked_roots, inspect};
use noct_core::contracts::WorkspaceAvailability;

const BLOCKED: &str = "Selecione uma pasta de projeto específica; raízes amplas e diretórios do sistema não são permitidos.";

fn availability(value: &str, blocked: &[PathBuf]) -> WorkspaceAvailability {
    inspect(value, blocked).availability
}

#[test]
fn an_existing_folder_is_available_without_a_message() {
    let scratch = Scratch::new("available");
    let project = scratch.path().join("project");
    std::fs::create_dir(&project).expect("the folder is created");

    let inspection = inspect(&project.to_string_lossy(), &[]);

    assert_eq!(inspection.availability, WorkspaceAvailability::Available);
    assert_eq!(inspection.message, None);
}

#[test]
fn a_folder_that_does_not_exist_is_missing() {
    let scratch = Scratch::new("missing");

    let inspection = inspect(&scratch.path().join("gone").to_string_lossy(), &[]);

    assert_eq!(inspection.availability, WorkspaceAvailability::Missing);
    assert_eq!(inspection.message, Some("Pasta do projeto não encontrada."));
}

#[test]
fn a_file_is_not_a_workspace() {
    let scratch = Scratch::new("file");
    let file = scratch.path().join("notes.txt");
    std::fs::write(&file, b"x").expect("the file is written");

    let inspection = inspect(&file.to_string_lossy(), &[]);

    assert_eq!(inspection.availability, WorkspaceAvailability::Invalid);
    assert_eq!(
        inspection.message,
        Some("O caminho do workspace não é uma pasta.")
    );
}

#[cfg(unix)]
#[test]
fn the_file_system_root_is_blocked_whatever_the_blocked_list_says() {
    let inspection = inspect("/", &[]);

    assert_eq!(inspection.availability, WorkspaceAvailability::Invalid);
    assert_eq!(inspection.message, Some(BLOCKED));
}

#[test]
fn a_folder_on_the_blocked_list_is_refused_and_so_is_its_canonical_path() {
    let scratch = Scratch::new("blocked");
    let project = scratch.path().join("project");
    std::fs::create_dir(&project).expect("the folder is created");

    assert_eq!(
        availability(&project.to_string_lossy(), std::slice::from_ref(&project)),
        WorkspaceAvailability::Invalid
    );
    assert_eq!(
        inspect(&project.to_string_lossy(), std::slice::from_ref(&project)).message,
        Some(BLOCKED)
    );
}

/// `..` is resolved before the checks, as `path.resolve` does, so a detour through a blocked folder does
/// not hide it.
#[test]
fn dot_dot_is_resolved_before_the_checks() {
    let scratch = Scratch::new("dot-dot");
    let project = scratch.path().join("project");
    std::fs::create_dir(&project).expect("the folder is created");
    let detour = format!("{}/project/../project", scratch.path().display());

    assert_eq!(availability(&detour, &[]), WorkspaceAvailability::Available);
    assert_eq!(
        availability(&detour, std::slice::from_ref(&project)),
        WorkspaceAvailability::Invalid
    );
}

#[test]
fn device_paths_and_nul_bytes_are_invalid() {
    for value in [
        r"\\?\C:\projects",
        r"\\.\pipe\x",
        "//server/share",
        "/tmp/a\0b",
    ] {
        let inspection = inspect(value, &[]);

        assert_eq!(
            inspection.availability,
            WorkspaceAvailability::Invalid,
            "{value:?}"
        );
        assert_eq!(
            inspection.message,
            Some("Caminho de workspace inválido."),
            "{value:?}"
        );
    }
}

#[test]
fn the_default_blocked_list_holds_the_system_folders() {
    let roots = default_blocked_roots();

    if cfg!(unix) {
        assert!(roots.contains(&PathBuf::from("/usr")));
        assert!(roots.contains(&PathBuf::from("/etc")));
    }
}

#[cfg(unix)]
mod unix {
    use std::os::unix::fs::PermissionsExt;

    use super::*;

    /// Sets the mode of `folder` and restores it on drop, so that the scratch directory can be removed.
    struct Mode<'a>(&'a Path);

    impl Drop for Mode<'_> {
        fn drop(&mut self) {
            let _ = std::fs::set_permissions(self.0, PermissionsExt::from_mode(0o700));
        }
    }

    fn with_mode(folder: &Path, mode: u32) -> Mode<'_> {
        std::fs::set_permissions(folder, PermissionsExt::from_mode(mode)).expect("the mode is set");
        Mode(folder)
    }

    #[test]
    fn a_folder_that_cannot_be_listed_is_permission_denied() {
        let scratch = Scratch::new("unlistable");
        let locked = scratch.path().join("locked");
        std::fs::create_dir(&locked).expect("the folder is created");
        let _restore = with_mode(&locked, 0o000);

        let inspection = inspect(&locked.to_string_lossy(), &[]);

        assert_eq!(
            inspection.availability,
            WorkspaceAvailability::PermissionDenied
        );
        assert_eq!(
            inspection.message,
            Some("Sem permissão para acessar a pasta do projeto.")
        );
    }

    /// A folder that can be listed but not searched is not usable as a workspace: its entries cannot be
    /// reached, which is what `access(X_OK)` refuses.
    #[test]
    fn a_folder_that_cannot_be_searched_is_permission_denied() {
        let scratch = Scratch::new("unsearchable");
        let folder = scratch.path().join("listed-only");
        std::fs::create_dir(&folder).expect("the folder is created");
        let _restore = with_mode(&folder, 0o400);

        assert_eq!(
            availability(&folder.to_string_lossy(), &[]),
            WorkspaceAvailability::PermissionDenied
        );
    }

    /// A link into a system folder resolves to the system folder, so it is refused as that folder is.
    #[test]
    fn a_symbolic_link_into_a_system_folder_is_blocked() {
        let scratch = Scratch::new("link");
        let link = scratch.path().join("link");
        std::os::unix::fs::symlink("/usr", &link).expect("the link is created");

        let inspection = inspect(&link.to_string_lossy(), &default_blocked_roots());

        assert_eq!(inspection.availability, WorkspaceAvailability::Invalid);
        assert_eq!(inspection.message, Some(BLOCKED));
    }
}
