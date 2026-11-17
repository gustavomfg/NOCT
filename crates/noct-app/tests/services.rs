//! The work behind the commands, against a store in a scratch directory. The expected values are the
//! ones `registerSettingsIpc.ts`, `registerWorkspaceIpc.ts` and the conversation repository produce.

mod common;

use common::Scratch;
use noct_app::bootstrap;
use noct_app::error::CommandError;
use noct_app::services;
use noct_core::contracts::{Workspace, WorkspaceAvailability};
use noct_core::ipc::conversations::ConversationPageRequest;
use noct_core::ipc::settings::{AppLanguage, AppTheme, ApprovalPolicy, SettingsUpdate};
use noct_core::ipc::workspaces::{WorkspaceCursor, WorkspaceFavoriteRequest, WorkspacePageRequest};

const NOT_FOUND: &str = "Pasta do projeto não encontrada.";
const NOT_A_FOLDER: &str = "O caminho do workspace não é uma pasta.";

#[test]
fn settings_read_the_defaults_of_a_new_store() {
    let scratch = Scratch::new("defaults");
    let persistence = bootstrap::open(scratch.path()).expect("the shell opens");
    let store = persistence.store.lock().expect("not poisoned");

    let settings = services::settings_get(&store).expect("the settings are read");

    assert_eq!(settings.approval_policy, ApprovalPolicy::OnRequest);
    assert_eq!(settings.theme, Some(AppTheme::Dark));
    assert_eq!(settings.language, Some(AppLanguage::PtBr));
    assert_eq!(settings.diagnostic_mode, Some(false));
}

#[test]
fn an_update_is_stored_and_read_back() {
    let scratch = Scratch::new("roundtrip");
    let persistence = bootstrap::open(scratch.path()).expect("the shell opens");
    let store = persistence.store.lock().expect("not poisoned");
    let update = SettingsUpdate {
        approval_policy: Some(ApprovalPolicy::Untrusted),
        theme: Some(AppTheme::Light),
        language: Some(AppLanguage::En),
        ..SettingsUpdate::default()
    };

    let written =
        services::settings_set(&store, &persistence.diagnostics, &update).expect("written");
    let read = services::settings_get(&store).expect("read");

    assert_eq!(written, read);
    assert_eq!(read.approval_policy, ApprovalPolicy::Untrusted);
    assert_eq!(read.theme, Some(AppTheme::Light));
    assert_eq!(read.language, Some(AppLanguage::En));
}

#[test]
fn the_diagnostic_gate_follows_each_update_that_carries_the_mode() {
    let scratch = Scratch::new("gate-follows");
    let persistence = bootstrap::open(scratch.path()).expect("the shell opens");
    let store = persistence.store.lock().expect("not poisoned");
    let on = SettingsUpdate {
        diagnostic_mode: Some(true),
        ..SettingsUpdate::default()
    };
    let off = SettingsUpdate {
        diagnostic_mode: Some(false),
        ..SettingsUpdate::default()
    };

    services::settings_set(&store, &persistence.diagnostics, &on).expect("written");
    assert!(persistence.diagnostics.is_enabled());
    services::settings_set(&store, &persistence.diagnostics, &off).expect("written");
    assert!(!persistence.diagnostics.is_enabled());
}

#[test]
fn an_update_without_the_mode_leaves_the_gate_alone() {
    let scratch = Scratch::new("gate-untouched");
    let persistence = bootstrap::open(scratch.path()).expect("the shell opens");
    let store = persistence.store.lock().expect("not poisoned");
    persistence.diagnostics.set_enabled(true);

    services::settings_set(&store, &persistence.diagnostics, &SettingsUpdate::default())
        .expect("written");

    assert!(persistence.diagnostics.is_enabled());
}

#[test]
fn workspaces_report_an_existing_folder_as_available_and_authorized() {
    let scratch = Scratch::new("available");
    let project = scratch.path().join("project");
    std::fs::create_dir(&project).expect("the folder is created");
    let persistence = bootstrap::open(&scratch.path().join("data")).expect("the shell opens");
    let store = persistence.store.lock().expect("not poisoned");
    let path = project.to_string_lossy().into_owned();
    store
        .workspaces()
        .touch(&path)
        .expect("the workspace is saved");

    let rows = services::workspace_rows(&store).expect("rows are read");
    let workspaces = services::workspaces_from_rows(rows, &[]);

    assert_eq!(workspaces.len(), 1);
    assert_eq!(workspaces[0].path, path);
    assert_eq!(workspaces[0].availability, WorkspaceAvailability::Available);
    assert!(workspaces[0].authorized);
    assert_eq!(workspaces[0].availability_message, None);
}

#[test]
fn a_missing_folder_is_not_authorized_and_says_why() {
    let scratch = Scratch::new("missing");
    let persistence = bootstrap::open(&scratch.path().join("data")).expect("the shell opens");
    let store = persistence.store.lock().expect("not poisoned");
    let path = scratch.path().join("gone").to_string_lossy().into_owned();
    store
        .workspaces()
        .touch(&path)
        .expect("the workspace is saved");

    let workspaces = services::workspaces_from_rows(
        services::workspace_rows(&store).expect("rows are read"),
        &[],
    );

    assert_eq!(workspaces[0].availability, WorkspaceAvailability::Missing);
    assert!(
        !workspaces[0].authorized,
        "a missing folder is not authorized"
    );
    assert_eq!(
        workspaces[0].availability_message.as_deref(),
        Some(NOT_FOUND)
    );
}

#[test]
fn a_file_saved_as_a_workspace_is_invalid() {
    let scratch = Scratch::new("file");
    let file = scratch.path().join("notes.txt");
    std::fs::write(&file, b"x").expect("the file is written");
    let persistence = bootstrap::open(&scratch.path().join("data")).expect("the shell opens");
    let store = persistence.store.lock().expect("not poisoned");
    store
        .workspaces()
        .touch(&file.to_string_lossy())
        .expect("the workspace is saved");

    let workspaces = services::workspaces_from_rows(
        services::workspace_rows(&store).expect("rows are read"),
        &[],
    );

    assert_eq!(workspaces[0].availability, WorkspaceAvailability::Invalid);
    assert_eq!(
        workspaces[0].availability_message.as_deref(),
        Some(NOT_A_FOLDER)
    );
    assert!(!workspaces[0].authorized);
}

/// A saved row that was never authorized stays unauthorized even when its folder is available, as the
/// TypeScript mapping keeps `authorized && availability === "available"`.
#[test]
fn a_saved_unauthorized_row_stays_unauthorized_while_its_folder_is_available() {
    let scratch = Scratch::new("unauthorized");
    let project = scratch.path().join("project");
    std::fs::create_dir(&project).expect("the folder is created");
    let persistence = bootstrap::open(&scratch.path().join("data")).expect("the shell opens");
    let store = persistence.store.lock().expect("not poisoned");
    store
        .connection()
        .execute(
            "INSERT INTO workspaces(path,name,authorized,created_at,last_opened_at)
             VALUES(?1,'project',0,'2026-01-01T00:00:00.000Z','2026-01-01T00:00:00.000Z')",
            [project.to_string_lossy().as_ref()],
        )
        .expect("the unauthorized row is saved");

    let workspaces = services::workspaces_from_rows(
        services::workspace_rows(&store).expect("rows are read"),
        &[],
    );

    assert_eq!(workspaces[0].availability, WorkspaceAvailability::Available);
    assert!(!workspaces[0].authorized);
}

#[test]
fn a_folder_on_the_blocked_list_is_invalid_and_unauthorized() {
    let scratch = Scratch::new("blocked");
    let project = scratch.path().join("project");
    std::fs::create_dir(&project).expect("the folder is created");
    let persistence = bootstrap::open(&scratch.path().join("data")).expect("the shell opens");
    let store = persistence.store.lock().expect("not poisoned");
    store
        .workspaces()
        .touch(&project.to_string_lossy())
        .expect("the workspace is saved");

    let workspaces = services::workspaces_from_rows(
        services::workspace_rows(&store).expect("rows are read"),
        std::slice::from_ref(&project),
    );

    assert_eq!(workspaces[0].availability, WorkspaceAvailability::Invalid);
    assert!(!workspaces[0].authorized);
}

#[test]
fn favorite_workspaces_are_listed_first() {
    let scratch = Scratch::new("favorites");
    let first = scratch.path().join("first");
    let second = scratch.path().join("second");
    std::fs::create_dir(&first).expect("the folder is created");
    std::fs::create_dir(&second).expect("the folder is created");
    let persistence = bootstrap::open(&scratch.path().join("data")).expect("the shell opens");
    let store = persistence.store.lock().expect("not poisoned");
    let first = first.to_string_lossy().into_owned();
    let second = second.to_string_lossy().into_owned();
    store.workspaces().touch(&first).expect("saved");
    store.workspaces().touch(&second).expect("saved");
    store
        .workspaces()
        .set_favorite(&first, true)
        .expect("favorited");

    let listed = services::workspaces_from_rows(
        services::workspace_rows(&store).expect("rows are read"),
        &[],
    );

    assert_eq!(listed[0].path, first);
    assert!(listed[0].favorite);
    assert_eq!(listed[1].path, second);
}

#[test]
fn conversations_are_listed_with_their_workspace() {
    let scratch = Scratch::new("conversations");
    let persistence = bootstrap::open(&scratch.path().join("data")).expect("the shell opens");
    let store = persistence.store.lock().expect("not poisoned");
    let created = store
        .conversations()
        .create("/work/project")
        .expect("the conversation is created");

    let request = ConversationPageRequest {
        limit: 10,
        cursor: None,
    };
    let page = services::conversations_page(&store, &request).expect("conversations are read");

    assert!(!page.has_more);
    assert_eq!(page.next_cursor, None);
    assert_eq!(page.items.len(), 1);
    assert_eq!(page.items[0].id, created.id);
    assert_eq!(page.items[0].workspace, "/work/project");
    assert_eq!(page.items[0].created_at, created.created_at);
    assert_eq!(page.items[0].updated_at, created.updated_at);
}

/// Pages of one conversation each: the first page reports more and carries the key of its conversation,
/// the page after it takes that key, and the last page reports no more and carries no cursor.
#[test]
fn conversation_pages_follow_the_cursor_and_report_more() {
    let scratch = Scratch::new("conversation-pages");
    let persistence = bootstrap::open(&scratch.path().join("data")).expect("the shell opens");
    let store = persistence.store.lock().expect("not poisoned");
    let created: Vec<_> = (0..3)
        .map(|_| {
            store
                .conversations()
                .create("/work/project")
                .expect("the conversation is created")
        })
        .collect();

    let mut seen = Vec::new();
    let mut cursor = None;
    let mut pages = 0;
    loop {
        let page =
            services::conversations_page(&store, &ConversationPageRequest { limit: 1, cursor })
                .expect("a page is read");
        pages += 1;
        assert_eq!(page.items.len(), 1, "one conversation per page");
        assert_eq!(
            page.next_cursor.is_some(),
            page.has_more,
            "the cursor is present exactly when more pages follow"
        );
        seen.push(page.items[0].id.clone());
        if !page.has_more {
            assert_eq!(page.next_cursor, None);
            break;
        }
        let next = page.next_cursor.expect("a page with more has a cursor");
        assert_eq!(
            next.id, page.items[0].id,
            "the cursor is the last conversation of its page"
        );
        assert_eq!(next.updated_at, page.items[0].updated_at);
        cursor = Some(next);
        assert!(pages <= created.len(), "the walk must end");
    }

    assert_eq!(pages, created.len(), "one page per conversation");
    let mut expected: Vec<String> = created.iter().map(|row| row.id.clone()).collect();
    seen.sort();
    expected.sort();
    assert_eq!(
        seen, expected,
        "every conversation is returned exactly once"
    );
}

/// A project page holds the rows after the cursor of the page before it, in the store's order, and reports more
/// exactly when a row follows it. Its `nextCursor` is the key of its last row. Pages of any size join back into the
/// whole list, and a favorite comes first.
#[test]
fn workspace_pages_follow_the_cursor_and_report_more() {
    let scratch = Scratch::new("workspace-pages");
    let folders: Vec<String> = (0..5)
        .map(|index| {
            let folder = scratch.path().join(format!("project-{index}"));
            std::fs::create_dir(&folder).expect("the folder is created");
            folder.to_string_lossy().into_owned()
        })
        .collect();
    let persistence = bootstrap::open(&scratch.path().join("data")).expect("the shell opens");
    let store = persistence.store.lock().expect("not poisoned");
    for folder in &folders {
        store.workspaces().touch(folder).expect("saved");
    }
    store
        .workspaces()
        .set_favorite(&folders[3], true)
        .expect("the favorite is stored");
    let rows = services::workspace_rows(&store).expect("rows are read");
    let whole = services::workspaces_from_rows(rows.clone(), &[]);
    assert_eq!(whole[0].path, folders[3], "the favorite comes first");
    let page = |limit: usize, cursor: Option<WorkspaceCursor>| {
        services::workspace_page(&store, &[], &WorkspacePageRequest { limit, cursor })
            .expect("a page is read")
    };
    let paths = |items: &[Workspace]| -> Vec<String> {
        items.iter().map(|item| item.path.clone()).collect()
    };

    let first = page(2, None);
    assert_eq!(paths(&first.items), paths(&whole[0..2]));
    assert!(first.has_more, "rows follow the first page");
    let key = first
        .next_cursor
        .clone()
        .expect("a page with more rows has a cursor");
    assert_eq!(
        key.path, rows[1].path,
        "the cursor is the key of the last row"
    );
    assert_eq!(key.favorite, rows[1].favorite);
    assert_eq!(key.last_opened_at, rows[1].last_opened_at);

    let exact = page(5, None);
    assert_eq!(paths(&exact.items), paths(&whole));
    assert!(
        !exact.has_more && exact.next_cursor.is_none(),
        "a page that reaches the end reports no more and no cursor"
    );

    for limit in [1, 2, 5] {
        let mut joined = Vec::new();
        let mut cursor = None;
        loop {
            let next = page(limit, cursor.clone());
            assert!(next.items.len() <= limit, "a page holds at most its limit");
            assert_eq!(
                next.next_cursor.is_some(),
                next.has_more,
                "nextCursor is present exactly when hasMore is"
            );
            joined.extend(paths(&next.items));
            if !next.has_more {
                break;
            }
            cursor = next.next_cursor;
        }
        assert_eq!(
            joined,
            paths(&whole),
            "pages of {limit} join back into the whole list"
        );
    }

    let after_the_last = page(
        2,
        Some(WorkspaceCursor {
            favorite: false,
            last_opened_at: String::new(),
            path: "\u{10ffff}".to_string(),
        }),
    );
    assert!(
        after_the_last.items.is_empty(),
        "a cursor after the last project is an empty page"
    );
    assert!(!after_the_last.has_more);
}

/// Projects with the same favorite flag and the same opening time are ordered by path, so every project is read once
/// through the pages, whatever their size. The TypeScript list leaves such ties to SQLite; the Rust order does not.
#[test]
fn workspace_pages_break_ties_by_path_so_every_project_is_read_once() {
    let scratch = Scratch::new("workspace-ties");
    let folders: Vec<String> = (0..4)
        .map(|index| {
            let folder = scratch.path().join(format!("tie-{index}"));
            std::fs::create_dir(&folder).expect("the folder is created");
            folder.to_string_lossy().into_owned()
        })
        .collect();
    let persistence = bootstrap::open(&scratch.path().join("data")).expect("the shell opens");
    let store = persistence.store.lock().expect("not poisoned");
    for folder in &folders {
        store.workspaces().touch(folder).expect("saved");
    }
    store
        .connection()
        .execute_batch("UPDATE workspaces SET last_opened_at = '2026-01-02T00:00:00.000Z'")
        .expect("every project gets the same opening time");
    store
        .workspaces()
        .set_favorite(&folders[2], true)
        .expect("the favorite is stored");

    let mut expected = vec![folders[2].clone()];
    let mut others: Vec<String> = folders
        .iter()
        .filter(|folder| **folder != folders[2])
        .cloned()
        .collect();
    others.sort();
    expected.extend(others);

    let whole: Vec<String> = services::workspace_rows(&store)
        .expect("rows are read")
        .into_iter()
        .map(|row| row.path)
        .collect();
    assert_eq!(whole, expected, "the favorite, then the paths in order");

    for limit in [1, 3] {
        let mut joined = Vec::new();
        let mut cursor = None;
        loop {
            let next = services::workspace_page(
                &store,
                &[],
                &WorkspacePageRequest {
                    limit,
                    cursor: cursor.clone(),
                },
            )
            .expect("a page is read");
            joined.extend(next.items.into_iter().map(|item| item.path));
            if !next.has_more {
                break;
            }
            cursor = next.next_cursor;
        }
        assert_eq!(joined, expected, "pages of {limit} read every project once");
    }
}

/// A favorite is stored for a saved project, and the list shows it first. A project that is not saved is refused,
/// and nothing is written for it: no row appears and the saved favorites stay as they were.
#[test]
fn a_favorite_is_stored_for_a_saved_project_and_refused_for_one_that_is_not() {
    let scratch = Scratch::new("favorite");
    let saved = scratch.path().join("saved");
    let other = scratch.path().join("other");
    std::fs::create_dir(&saved).expect("the folder is created");
    std::fs::create_dir(&other).expect("the folder is created");
    let persistence = bootstrap::open(&scratch.path().join("data")).expect("the shell opens");
    let store = persistence.store.lock().expect("not poisoned");
    let saved = saved.to_string_lossy().into_owned();
    let other = other.to_string_lossy().into_owned();
    store.workspaces().touch(&saved).expect("saved");
    store.workspaces().touch(&other).expect("saved");
    let favorite = |workspace: &str, favorite: bool| WorkspaceFavoriteRequest {
        workspace: workspace.to_string(),
        favorite,
    };

    services::workspace_set_favorite(&store, &favorite(&other, true))
        .expect("a saved project is marked");
    let listed = services::workspaces_from_rows(
        services::workspace_rows(&store).expect("rows are read"),
        &[],
    );
    assert_eq!(listed[0].path, other, "the favorite is listed first");
    assert!(listed[0].favorite);

    let missing = scratch
        .path()
        .join("never-saved")
        .to_string_lossy()
        .into_owned();
    let refused = services::workspace_set_favorite(&store, &favorite(&missing, true));
    assert!(
        matches!(refused, Err(CommandError::UnknownWorkspace)),
        "a project that is not saved is refused"
    );
    let rows = services::workspace_rows(&store).expect("rows are read");
    assert_eq!(rows.len(), 2, "the refused request saves no project");
    assert!(
        rows.iter().any(|row| row.path == other && row.favorite),
        "the favorite that was set is unchanged"
    );
    assert!(
        rows.iter().all(|row| row.path != missing),
        "no row is written for the refused path"
    );
}
