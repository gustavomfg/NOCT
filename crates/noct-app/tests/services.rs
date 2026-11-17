//! The work behind the commands, against a store in a scratch directory. The expected values are the
//! ones `registerSettingsIpc.ts`, `registerWorkspaceIpc.ts` and the conversation repository produce.

mod common;

use common::Scratch;
use noct_app::bootstrap;
use noct_app::error::CommandError;
use noct_app::services;
use noct_core::ai::bindings::WorkspaceModelBindings;
use noct_core::ai::model::ModelReference;
use noct_core::brain_memory::{
    BrainMemoryKind, BrainMemoryScope, BrainMemorySource, BrainMemoryStatus,
};
use noct_core::contracts::Role;
use noct_core::contracts::{Workspace, WorkspaceAvailability};
use noct_core::ipc::ReplyRequest;
use noct_core::ipc::ai::SaveAssistantRequest;
use noct_core::ipc::brain::{BrainMemoryCreateRequest, BrainMemoryUpdateRequest};
use noct_core::ipc::conversations::ConversationPageRequest;
use noct_core::ipc::settings::{AppLanguage, AppTheme, ApprovalPolicy, SettingsUpdate};
use noct_core::ipc::suggestions::{SuggestionCreation, SuggestionStatusRequest};
use noct_core::ipc::workspaces::{WorkspaceCursor, WorkspaceFavoriteRequest, WorkspacePageRequest};
use noct_core::suggestions::SuggestionStatus;
use serde_json::json;

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

#[test]
fn a_conversation_is_created_in_a_saved_authorized_folder() {
    let scratch = Scratch::new("conversation-create");
    let project = scratch.path().join("project");
    std::fs::create_dir(&project).expect("the folder is created");
    let persistence = bootstrap::open(&scratch.path().join("data")).expect("the shell opens");
    let store = persistence.store.lock().expect("not poisoned");
    let path = project.to_string_lossy().into_owned();
    store
        .workspaces()
        .touch(&path)
        .expect("the workspace is saved");

    let created =
        services::conversation_create(&store, &[], &path).expect("the conversation is created");

    assert_eq!(created.title, "Nova conversa");
    assert_eq!(created.workspace, path);
    assert_eq!(created.created_at, created.updated_at);
    assert_eq!(created.id.len(), 36, "the id is a hyphenated UUID");
    let stored: i64 = store
        .connection()
        .query_row(
            "SELECT COUNT(*) FROM conversations WHERE id=?1 AND workspace=?2",
            [created.id.as_str(), path.as_str()],
            |row| row.get(0),
        )
        .expect("the count is read");
    assert_eq!(stored, 1, "the conversation is stored under its project");
}

#[test]
fn a_conversation_is_refused_unless_its_folder_is_saved_authorized_available_and_not_blocked() {
    let scratch = Scratch::new("conversation-refused");
    let project = scratch.path().join("project");
    std::fs::create_dir(&project).expect("the folder is created");
    let path = project.to_string_lossy().into_owned();
    let persistence = bootstrap::open(&scratch.path().join("data")).expect("the shell opens");
    let store = persistence.store.lock().expect("not poisoned");
    let conversations = |store: &noct_db::store::Store| -> i64 {
        store
            .connection()
            .query_row("SELECT COUNT(*) FROM conversations", [], |row| row.get(0))
            .expect("the count is read")
    };

    assert!(
        matches!(
            services::conversation_create(&store, &[], &path),
            Err(CommandError::UnknownWorkspace)
        ),
        "a folder that is not saved"
    );
    assert_eq!(conversations(&store), 0);

    store
        .connection()
        .execute(
            "INSERT INTO workspaces(path,name,authorized,created_at,last_opened_at)
             VALUES(?1,'project',0,'2026-01-01T00:00:00.000Z','2026-01-01T00:00:00.000Z')",
            [path.as_str()],
        )
        .expect("the unauthorized row is saved");
    assert!(
        matches!(
            services::conversation_create(&store, &[], &path),
            Err(CommandError::UnknownWorkspace)
        ),
        "a saved row that is not authorized"
    );
    assert_eq!(conversations(&store), 0);

    store
        .workspaces()
        .touch(&path)
        .expect("the workspace is authorized");
    assert!(
        matches!(
            services::conversation_create(&store, std::slice::from_ref(&project), &path),
            Err(CommandError::UnknownWorkspace)
        ),
        "a folder on the blocked list"
    );
    assert_eq!(conversations(&store), 0);

    std::fs::remove_dir(&project).expect("the folder is removed");
    assert!(
        matches!(
            services::conversation_create(&store, &[], &path),
            Err(CommandError::UnknownWorkspace)
        ),
        "a saved folder that is no longer available"
    );
    assert_eq!(conversations(&store), 0);

    assert!(
        matches!(
            services::conversation_create(&store, &[], ""),
            Err(CommandError::InvalidInput)
        ),
        "an empty path is invalid input"
    );
}

#[test]
fn a_brain_memory_is_a_manual_candidate_in_the_project_of_its_conversation() {
    let scratch = Scratch::new("brain-create");
    let project = scratch.path().join("project");
    std::fs::create_dir(&project).expect("the folder is created");
    let path = project.to_string_lossy().into_owned();
    let persistence = bootstrap::open(&scratch.path().join("data")).expect("the shell opens");
    let store = persistence.store.lock().expect("not poisoned");
    store
        .workspaces()
        .touch(&path)
        .expect("the workspace is saved");
    let conversation = services::conversation_create(&store, &[], &path).expect("created");

    let request = BrainMemoryCreateRequest {
        conversation_id: conversation.id.clone(),
        kind: BrainMemoryKind::Decision,
        scope: BrainMemoryScope::Conversation,
        content: "Prefer small commits".to_string(),
    };
    let memory = services::brain_create(&store, &[], &request).expect("the memory is created");

    assert_eq!(memory.workspace_id, path);
    assert_eq!(
        memory.conversation_id.as_deref(),
        Some(conversation.id.as_str())
    );
    assert_eq!(memory.status, BrainMemoryStatus::Candidate);
    assert_eq!(memory.source_type, BrainMemorySource::Manual);
    assert_eq!(memory.content, "Prefer small commits");

    let workspace_wide = BrainMemoryCreateRequest {
        scope: BrainMemoryScope::Workspace,
        content: "The project uses Rust 1.97".to_string(),
        ..request
    };
    let shared = services::brain_create(&store, &[], &workspace_wide).expect("created");
    assert_eq!(
        shared.conversation_id, None,
        "a workspace memory keeps no conversation"
    );
}

#[test]
fn a_brain_memory_is_refused_without_a_conversation_or_an_authorized_project() {
    let scratch = Scratch::new("brain-refused");
    let project = scratch.path().join("project");
    std::fs::create_dir(&project).expect("the folder is created");
    let path = project.to_string_lossy().into_owned();
    let persistence = bootstrap::open(&scratch.path().join("data")).expect("the shell opens");
    let store = persistence.store.lock().expect("not poisoned");
    store
        .workspaces()
        .touch(&path)
        .expect("the workspace is saved");
    let conversation = services::conversation_create(&store, &[], &path).expect("created");
    let memories = || -> i64 {
        store
            .connection()
            .query_row("SELECT COUNT(*) FROM brain_memories", [], |row| row.get(0))
            .expect("the count is read")
    };
    let request = |conversation_id: &str| BrainMemoryCreateRequest {
        conversation_id: conversation_id.to_string(),
        kind: BrainMemoryKind::Fact,
        scope: BrainMemoryScope::Workspace,
        content: "A note".to_string(),
    };

    assert!(
        matches!(
            services::brain_create(
                &store,
                &[],
                &request("3f0c7a52-8d1e-4b6a-9c2f-5e7d8a9b0c1d")
            ),
            Err(CommandError::UnknownConversation)
        ),
        "a conversation that does not exist"
    );
    assert_eq!(memories(), 0);

    std::fs::remove_dir(&project).expect("the folder is removed");
    assert!(
        matches!(
            services::brain_create(&store, &[], &request(&conversation.id)),
            Err(CommandError::UnknownWorkspace)
        ),
        "a conversation whose project is no longer available"
    );
    assert_eq!(memories(), 0, "the refused request writes nothing");
}

#[test]
fn a_brain_memory_follows_its_lifecycle_and_a_refused_change_writes_nothing() {
    let scratch = Scratch::new("brain-update");
    let project = scratch.path().join("project");
    std::fs::create_dir(&project).expect("the folder is created");
    let path = project.to_string_lossy().into_owned();
    let persistence = bootstrap::open(&scratch.path().join("data")).expect("the shell opens");
    let store = persistence.store.lock().expect("not poisoned");
    store
        .workspaces()
        .touch(&path)
        .expect("the workspace is saved");
    let conversation = services::conversation_create(&store, &[], &path).expect("created");
    let memory = services::brain_create(
        &store,
        &[],
        &BrainMemoryCreateRequest {
            conversation_id: conversation.id.clone(),
            kind: BrainMemoryKind::Fact,
            scope: BrainMemoryScope::Workspace,
            content: "A note".to_string(),
        },
    )
    .expect("created");
    let blank = || BrainMemoryUpdateRequest {
        conversation_id: conversation.id.clone(),
        memory_id: memory.id.clone(),
        kind: None,
        scope: None,
        content: None,
        confidence: None,
        status: None,
    };
    let change = |edit: &dyn Fn(&mut BrainMemoryUpdateRequest)| {
        let mut request = blank();
        edit(&mut request);
        services::brain_update(&store, &[], &request)
    };

    let edited = change(&|r| {
        r.content = Some("Prefer large commits".to_string());
        r.confidence = Some(80);
    })
    .expect("the content and confidence change");
    assert_eq!(edited.content, "Prefer large commits");
    assert_eq!(edited.confidence, 80);
    assert_eq!(edited.status, BrainMemoryStatus::Candidate);

    assert_eq!(
        change(&|r| r.status = Some(BrainMemoryStatus::Active))
            .expect("candidate to active")
            .status,
        BrainMemoryStatus::Active
    );
    assert_eq!(
        change(&|r| r.status = Some(BrainMemoryStatus::Archived))
            .expect("active to archived")
            .status,
        BrainMemoryStatus::Archived
    );
    assert!(
        matches!(
            change(&|r| r.status = Some(BrainMemoryStatus::Candidate)),
            Err(CommandError::InvalidTransition)
        ),
        "an archived memory does not become a candidate"
    );
    assert!(
        change(&|r| r.status = Some(BrainMemoryStatus::Archived)).is_ok(),
        "a status that stays the same is not a change"
    );

    let stored = store
        .brain_memories()
        .get(&memory.id, &path)
        .expect("the memory is read")
        .expect("the memory is stored");
    assert_eq!(
        stored.status,
        BrainMemoryStatus::Archived,
        "the refused change wrote nothing"
    );
    assert_eq!(stored.content, "Prefer large commits");
}

#[test]
fn a_brain_memory_scope_change_sets_or_clears_its_conversation() {
    let scratch = Scratch::new("brain-update-scope");
    let project = scratch.path().join("project");
    std::fs::create_dir(&project).expect("the folder is created");
    let path = project.to_string_lossy().into_owned();
    let persistence = bootstrap::open(&scratch.path().join("data")).expect("the shell opens");
    let store = persistence.store.lock().expect("not poisoned");
    store
        .workspaces()
        .touch(&path)
        .expect("the workspace is saved");
    let conversation = services::conversation_create(&store, &[], &path).expect("created");
    let memory = services::brain_create(
        &store,
        &[],
        &BrainMemoryCreateRequest {
            conversation_id: conversation.id.clone(),
            kind: BrainMemoryKind::Fact,
            scope: BrainMemoryScope::Workspace,
            content: "A note".to_string(),
        },
    )
    .expect("created");
    let update = |scope: BrainMemoryScope| BrainMemoryUpdateRequest {
        conversation_id: conversation.id.clone(),
        memory_id: memory.id.clone(),
        kind: None,
        scope: Some(scope),
        content: None,
        confidence: None,
        status: None,
    };

    let scoped = services::brain_update(&store, &[], &update(BrainMemoryScope::Conversation))
        .expect("the memory moves to the conversation");
    assert_eq!(
        scoped.conversation_id.as_deref(),
        Some(conversation.id.as_str())
    );

    let shared = services::brain_update(&store, &[], &update(BrainMemoryScope::Workspace))
        .expect("the memory moves to the workspace");
    assert_eq!(
        shared.conversation_id, None,
        "a workspace memory keeps no conversation"
    );
}

#[test]
fn a_brain_update_names_the_missing_conversation_or_memory() {
    let scratch = Scratch::new("brain-update-missing");
    let project = scratch.path().join("project");
    std::fs::create_dir(&project).expect("the folder is created");
    let path = project.to_string_lossy().into_owned();
    let persistence = bootstrap::open(&scratch.path().join("data")).expect("the shell opens");
    let store = persistence.store.lock().expect("not poisoned");
    store
        .workspaces()
        .touch(&path)
        .expect("the workspace is saved");
    let conversation = services::conversation_create(&store, &[], &path).expect("created");
    let request = |conversation_id: &str, memory_id: &str| BrainMemoryUpdateRequest {
        conversation_id: conversation_id.to_string(),
        memory_id: memory_id.to_string(),
        kind: None,
        scope: None,
        content: Some("A note".to_string()),
        confidence: None,
        status: None,
    };
    let absent = "7a1d2c3e-4f50-4a61-8b72-9c83d4e5f607";

    assert!(
        matches!(
            services::brain_update(
                &store,
                &[],
                &request("3f0c7a52-8d1e-4b6a-9c2f-5e7d8a9b0c1d", absent)
            ),
            Err(CommandError::UnknownConversation)
        ),
        "a conversation that does not exist"
    );
    assert!(
        matches!(
            services::brain_update(&store, &[], &request(&conversation.id, absent)),
            Err(CommandError::UnknownMemory)
        ),
        "a memory that does not exist in the project"
    );
}

#[test]
fn brain_extract_stores_the_candidates_of_a_reply_and_skips_the_known_ones() {
    let scratch = Scratch::new("brain-extract");
    let project = scratch.path().join("project");
    std::fs::create_dir(&project).expect("the folder is created");
    let path = project.to_string_lossy().into_owned();
    let persistence = bootstrap::open(&scratch.path().join("data")).expect("the shell opens");
    let store = persistence.store.lock().expect("not poisoned");
    store
        .workspaces()
        .touch(&path)
        .expect("the workspace is saved");
    let conversation = services::conversation_create(&store, &[], &path).expect("created");
    services::brain_create(
        &store,
        &[],
        &BrainMemoryCreateRequest {
            conversation_id: conversation.id.clone(),
            kind: BrainMemoryKind::Fact,
            scope: BrainMemoryScope::Workspace,
            content: "Use SQLite".to_string(),
        },
    )
    .expect("the known memory is stored");
    let reply = "Resposta final.\n```nocturne-memories\n[\
        {\"kind\":\"decision\",\"scope\":\"conversation\",\"content\":\"Keep the review read-only\"},\
        {\"kind\":\"fact\",\"scope\":\"workspace\",\"content\":\"use sqlite\"}]\n```";

    let extraction = services::brain_extract(
        &store,
        &[],
        &ReplyRequest {
            conversation_id: conversation.id.clone(),
            content: reply.to_string(),
        },
    )
    .expect("the reply is read");

    assert_eq!(extraction.content, "Resposta final.");
    assert_eq!(extraction.warning, None);
    assert_eq!(
        extraction.memories.len(),
        1,
        "the equivalent memory is skipped"
    );
    let memory = &extraction.memories[0];
    assert_eq!(memory.content, "Keep the review read-only");
    assert_eq!(memory.source_type, BrainMemorySource::Agent);
    assert_eq!(memory.status, BrainMemoryStatus::Candidate);
    assert_eq!(
        memory.conversation_id.as_deref(),
        Some(conversation.id.as_str())
    );
}

#[test]
fn a_reply_whose_memories_cannot_be_stored_is_kept_with_a_warning() {
    let scratch = Scratch::new("brain-extract-warning");
    let project = scratch.path().join("project");
    std::fs::create_dir(&project).expect("the folder is created");
    let path = project.to_string_lossy().into_owned();
    let persistence = bootstrap::open(&scratch.path().join("data")).expect("the shell opens");
    let store = persistence.store.lock().expect("not poisoned");
    store
        .workspaces()
        .touch(&path)
        .expect("the workspace is saved");
    let conversation = services::conversation_create(&store, &[], &path).expect("created");
    store
        .connection()
        .execute_batch(
            "CREATE TRIGGER refuse_candidates BEFORE INSERT ON brain_memories
             BEGIN SELECT RAISE(ABORT, 'refused by the test'); END;",
        )
        .expect("the trigger is installed");
    let reply = "Resposta.\n```nocturne-memories\n[{\"kind\":\"fact\",\"scope\":\"workspace\",\"content\":\"A note\"}]\n```";

    let extraction = services::brain_extract(
        &store,
        &[],
        &ReplyRequest {
            conversation_id: conversation.id.clone(),
            content: reply.to_string(),
        },
    )
    .expect("a failed store is not an error: the reply is kept");

    assert!(extraction.memories.is_empty());
    assert_eq!(extraction.content, "Resposta.");
    assert_eq!(
        extraction.warning.as_deref(),
        Some(
            "A resposta foi preservada, mas não foi possível salvar as candidatas do Segundo Cérebro."
        )
    );
    let stored: i64 = store
        .connection()
        .query_row("SELECT COUNT(*) FROM brain_memories", [], |row| row.get(0))
        .expect("the count is read");
    assert_eq!(stored, 0);
}

#[test]
fn brain_extract_names_a_conversation_that_does_not_exist() {
    let scratch = Scratch::new("brain-extract-missing");
    let persistence = bootstrap::open(&scratch.path().join("data")).expect("the shell opens");
    let store = persistence.store.lock().expect("not poisoned");
    let refused = services::brain_extract(
        &store,
        &[],
        &ReplyRequest {
            conversation_id: "3f0c7a52-8d1e-4b6a-9c2f-5e7d8a9b0c1d".to_string(),
            content: "Resposta.".to_string(),
        },
    );
    assert!(matches!(refused, Err(CommandError::UnknownConversation)));
}

#[test]
fn suggestions_create_reconciles_a_review_with_the_previous_one() {
    let scratch = Scratch::new("suggestions-create");
    let project = scratch.path().join("project");
    std::fs::create_dir(&project).expect("the folder is created");
    let path = project.to_string_lossy().into_owned();
    let persistence = bootstrap::open(&scratch.path().join("data")).expect("the shell opens");
    let store = persistence.store.lock().expect("not poisoned");
    store
        .workspaces()
        .touch(&path)
        .expect("the workspace is saved");
    let conversation = services::conversation_create(&store, &[], &path).expect("created");
    let review = |body: &str| ReplyRequest {
        conversation_id: conversation.id.clone(),
        content: format!(
            "Revisão.\n```nocturne-suggestions\n[{{\"title\":\"Missing tests\",\"description\":\"d\",\"reasoning\":\"r\",\"category\":\"testing\",\"severity\":\"medium\"{body}}}]\n```"
        ),
    };

    let first: SuggestionCreation =
        services::suggestions_create(&store, &[], &review("")).expect("the review is read");
    assert_eq!(first.suggestions.len(), 1);
    assert_eq!(first.warning, None);
    let comparison = first
        .comparison
        .as_ref()
        .expect("a review carries its comparison");
    assert_eq!(comparison.new_suggestions.len(), 1);
    assert!(
        first
            .content
            .starts_with("Revisão.\n\n### Comparação com a revisão anterior\n\n- Novas: 1"),
        "{}",
        first.content
    );

    let second =
        services::suggestions_create(&store, &[], &review("")).expect("the review is read");
    let comparison = second.comparison.as_ref().expect("comparison");
    assert_eq!(
        comparison.persistent_suggestions.len(),
        1,
        "the same finding persists"
    );
    assert_eq!(comparison.new_suggestions.len(), 0);
}

#[test]
fn a_reply_without_a_snapshot_keeps_the_earlier_suggestions_and_says_so() {
    let scratch = Scratch::new("suggestions-absent");
    let project = scratch.path().join("project");
    std::fs::create_dir(&project).expect("the folder is created");
    let path = project.to_string_lossy().into_owned();
    let persistence = bootstrap::open(&scratch.path().join("data")).expect("the shell opens");
    let store = persistence.store.lock().expect("not poisoned");
    store
        .workspaces()
        .touch(&path)
        .expect("the workspace is saved");
    let conversation = services::conversation_create(&store, &[], &path).expect("created");

    let reply = ReplyRequest {
        conversation_id: conversation.id.clone(),
        content: "  Só texto.  ".to_string(),
    };
    let created = services::suggestions_create(&store, &[], &reply).expect("the reply is read");
    assert!(created.suggestions.is_empty());
    assert_eq!(created.comparison, None);
    assert_eq!(created.content, "Só texto.");
    assert_eq!(
        created.warning.as_deref(),
        Some(
            "A resposta não trouxe um snapshot estruturado; sugestões anteriores foram preservadas."
        )
    );
    let stored: i64 = store
        .connection()
        .query_row("SELECT COUNT(*) FROM suggestions", [], |row| row.get(0))
        .expect("the count is read");
    assert_eq!(
        stored, 0,
        "nothing is stored for a reply without a snapshot"
    );
}

#[test]
fn suggestions_create_names_a_conversation_that_does_not_exist() {
    let scratch = Scratch::new("suggestions-missing");
    let persistence = bootstrap::open(&scratch.path().join("data")).expect("the shell opens");
    let store = persistence.store.lock().expect("not poisoned");
    let refused = services::suggestions_create(
        &store,
        &[],
        &ReplyRequest {
            conversation_id: "3f0c7a52-8d1e-4b6a-9c2f-5e7d8a9b0c1d".to_string(),
            content: "x".to_string(),
        },
    );
    assert!(matches!(refused, Err(CommandError::UnknownConversation)));
}

fn review_of_one(conversation: &str) -> ReplyRequest {
    ReplyRequest {
        conversation_id: conversation.to_string(),
        content: "Revisão.\n```nocturne-suggestions\n[{\"title\":\"Pin the toolchain\",\"description\":\"d\",\"reasoning\":\"r\",\"category\":\"dependency\",\"severity\":\"low\"}]\n```".to_string(),
    }
}

#[test]
fn a_suggestion_moves_through_its_lifecycle_and_each_move_is_one_decision() {
    let scratch = Scratch::new("suggestion-status");
    let project = scratch.path().join("project");
    std::fs::create_dir(&project).expect("the folder is created");
    let path = project.to_string_lossy().into_owned();
    let persistence = bootstrap::open(&scratch.path().join("data")).expect("the shell opens");
    let store = persistence.store.lock().expect("not poisoned");
    store
        .workspaces()
        .touch(&path)
        .expect("the workspace is saved");
    let conversation = services::conversation_create(&store, &[], &path).expect("created");
    let created = services::suggestions_create(&store, &[], &review_of_one(&conversation.id))
        .expect("the review is read");
    let suggestion = created.suggestions[0].id.clone();
    let decisions = || -> i64 {
        store
            .connection()
            .query_row(
                "SELECT COUNT(*) FROM suggestion_decisions WHERE suggestion_id=?1",
                [&suggestion],
                |row| row.get(0),
            )
            .expect("the count is read")
    };
    let before = decisions();
    let move_to = |to: SuggestionStatus, result: Option<&str>| SuggestionStatusRequest {
        conversation_id: conversation.id.clone(),
        suggestion_id: suggestion.clone(),
        status: to,
        result: result.map(str::to_string),
    };

    let analysing =
        services::suggestions_status(&store, &[], &move_to(SuggestionStatus::InAnalysis, None))
            .expect("moves to analysis");
    assert_eq!(analysing.status, SuggestionStatus::InAnalysis);
    let accepted = services::suggestions_status(
        &store,
        &[],
        &move_to(SuggestionStatus::Accepted, Some("Entra na próxima versão")),
    )
    .expect("accepted");
    assert!(
        accepted
            .history
            .iter()
            .any(|entry| entry.status == SuggestionStatus::Accepted
                && entry.result.as_deref() == Some("Entra na próxima versão")),
        "the decision keeps its result in the history"
    );

    services::suggestions_status(&store, &[], &move_to(SuggestionStatus::Accepted, None))
        .expect("the same status is accepted and changes nothing");
    assert_eq!(
        decisions(),
        before + 2,
        "the same status writes no decision"
    );

    let refused = services::suggestions_status(&store, &[], &move_to(SuggestionStatus::New, None));
    assert!(matches!(
        refused,
        Err(CommandError::InvalidSuggestionTransition)
    ));
    services::suggestions_status(&store, &[], &move_to(SuggestionStatus::Resolved, None))
        .expect("resolved");
    let closed =
        services::suggestions_status(&store, &[], &move_to(SuggestionStatus::Accepted, None));
    assert!(
        matches!(closed, Err(CommandError::InvalidSuggestionTransition)),
        "a resolved suggestion is final"
    );
    assert_eq!(decisions(), before + 3, "the refused moves wrote nothing");
}

#[test]
fn a_status_move_checks_the_conversation_then_the_suggestion_then_the_transition() {
    let scratch = Scratch::new("suggestion-status-checks");
    let project = scratch.path().join("project");
    std::fs::create_dir(&project).expect("the folder is created");
    let path = project.to_string_lossy().into_owned();
    let persistence = bootstrap::open(&scratch.path().join("data")).expect("the shell opens");
    let store = persistence.store.lock().expect("not poisoned");
    store
        .workspaces()
        .touch(&path)
        .expect("the workspace is saved");
    let first = services::conversation_create(&store, &[], &path).expect("created");
    let second = services::conversation_create(&store, &[], &path).expect("created");
    let created = services::suggestions_create(&store, &[], &review_of_one(&first.id))
        .expect("the review is read");
    let suggestion = created.suggestions[0].id.clone();
    let move_in = |conversation: &str, to: SuggestionStatus| SuggestionStatusRequest {
        conversation_id: conversation.to_string(),
        suggestion_id: suggestion.clone(),
        status: to,
        result: None,
    };

    let unknown_conversation = services::suggestions_status(
        &store,
        &[],
        &move_in(
            "3f0c7a52-8d1e-4b6a-9c2f-5e7d8a9b0c1d",
            SuggestionStatus::Accepted,
        ),
    );
    assert!(matches!(
        unknown_conversation,
        Err(CommandError::UnknownConversation)
    ));

    let elsewhere = services::suggestions_status(
        &store,
        &[],
        &move_in(&second.id, SuggestionStatus::Accepted),
    );
    assert!(
        matches!(elsewhere, Err(CommandError::UnknownSuggestion)),
        "a suggestion of another conversation is not found there"
    );

    let blocked = vec![project.clone()];
    let refused_project = services::suggestions_status(
        &store,
        &blocked,
        &move_in(&first.id, SuggestionStatus::Accepted),
    );
    assert!(matches!(
        refused_project,
        Err(CommandError::UnknownWorkspace)
    ));

    let status: String = store
        .connection()
        .query_row(
            "SELECT status FROM suggestions WHERE id=?1",
            [&suggestion],
            |row| row.get(0),
        )
        .expect("the status is read");
    assert_eq!(status, "new", "no refused move changed the suggestion");
}

fn model(provider: &str, model: &str) -> ModelReference {
    ModelReference {
        provider_id: provider.to_string(),
        model_id: model.to_string(),
    }
}

#[test]
fn models_set_bindings_stores_the_bindings_of_a_saved_project_and_a_second_write_replaces_them() {
    let scratch = Scratch::new("models-set-bindings");
    let project = scratch.path().join("project");
    std::fs::create_dir(&project).expect("the folder is created");
    let path = project.to_string_lossy().into_owned();
    let persistence = bootstrap::open(&scratch.path().join("data")).expect("the shell opens");
    let store = persistence.store.lock().expect("not poisoned");
    store
        .workspaces()
        .touch(&path)
        .expect("the workspace is saved");

    let first = WorkspaceModelBindings {
        workspace_id: format!("  {path}  "),
        default_binding: Some(model("openai", "gpt-model")),
        embedding_binding: None,
        remote_embedding_allowed: Some(false),
    };
    let stored =
        services::models_set_bindings(&store, &[], first).expect("the bindings are stored");
    assert_eq!(
        stored.workspace_id, path,
        "the identifier is trimmed before it is stored"
    );
    assert_eq!(
        store
            .workspace_model_bindings()
            .get(&path)
            .expect("the bindings are read"),
        Some(stored.clone())
    );

    let replacement = WorkspaceModelBindings {
        workspace_id: path.clone(),
        default_binding: None,
        embedding_binding: Some(model("local", "embed")),
        remote_embedding_allowed: Some(true),
    };
    services::models_set_bindings(&store, &[], replacement.clone())
        .expect("the bindings are replaced");
    assert_eq!(
        store
            .workspace_model_bindings()
            .get(&path)
            .expect("the bindings are read"),
        Some(replacement)
    );
    let rows: i64 = store
        .connection()
        .query_row("SELECT COUNT(*) FROM workspace_model_bindings", [], |row| {
            row.get(0)
        })
        .expect("the count is read");
    assert_eq!(rows, 1, "one record per project");
}

#[test]
fn models_set_bindings_refuses_a_project_that_is_not_saved_authorized_available_or_allowed() {
    let scratch = Scratch::new("models-set-bindings-refused");
    let project = scratch.path().join("project");
    std::fs::create_dir(&project).expect("the folder is created");
    let path = project.to_string_lossy().into_owned();
    let persistence = bootstrap::open(&scratch.path().join("data")).expect("the shell opens");
    let store = persistence.store.lock().expect("not poisoned");
    let bindings = |workspace: &str| WorkspaceModelBindings {
        workspace_id: workspace.to_string(),
        default_binding: Some(model("openai", "gpt-model")),
        embedding_binding: None,
        remote_embedding_allowed: None,
    };

    let unsaved = services::models_set_bindings(&store, &[], bindings(&path));
    assert!(
        matches!(unsaved, Err(CommandError::UnknownWorkspace)),
        "an unsaved project is refused"
    );

    store
        .workspaces()
        .touch(&path)
        .expect("the workspace is saved");
    let blocked = vec![project.clone()];
    let refused = services::models_set_bindings(&store, &blocked, bindings(&path));
    assert!(
        matches!(refused, Err(CommandError::UnknownWorkspace)),
        "a blocked project is refused"
    );

    let empty = services::models_set_bindings(&store, &[], bindings("   "));
    assert!(matches!(empty, Err(CommandError::InvalidInput)));

    assert_eq!(
        store
            .workspace_model_bindings()
            .get(&path)
            .expect("the bindings are read"),
        None,
        "no refused write stored anything"
    );
}

#[test]
fn ai_save_assistant_stores_the_reply_with_its_metadata_and_the_artifacts_it_names() {
    let scratch = Scratch::new("ai-save-assistant");
    let project = scratch.path().join("project");
    std::fs::create_dir(&project).expect("the folder is created");
    let path = project.to_string_lossy().into_owned();
    let persistence = bootstrap::open(&scratch.path().join("data")).expect("the shell opens");
    let store = persistence.store.lock().expect("not poisoned");
    store
        .workspaces()
        .touch(&path)
        .expect("the workspace is saved");
    let conversation = services::conversation_create(&store, &[], &path).expect("created");
    let metadata = json!({
        "files": [{ "path": "src/app.ts" }, { "path": "docs/a.md" }, "skipped"],
        "diff": "--- a\n+++ b",
    });
    let request = SaveAssistantRequest {
        conversation_id: conversation.id.clone(),
        content: "Pronto.".to_string(),
        metadata: Some(metadata.clone()),
    };

    let message = services::ai_save_assistant(&store, &[], &request).expect("the reply is saved");
    assert_eq!(message.role, Role::Assistant);
    assert_eq!(message.content, "Pronto.");
    let stored_metadata: serde_json::Value =
        serde_json::from_str(message.metadata.as_deref().expect("the metadata is stored"))
            .expect("the metadata is JSON");
    assert_eq!(stored_metadata, metadata);

    let artifacts = store
        .artifacts()
        .list(&conversation.id)
        .expect("the artifacts are listed");
    let find = |kind: &str, title: &str| {
        artifacts
            .iter()
            .find(|artifact| artifact.kind == kind && artifact.title == title)
            .unwrap_or_else(|| panic!("an artifact {kind} {title} exists: {artifacts:?}"))
            .clone()
    };
    let reply = artifacts
        .iter()
        .find(|artifact| artifact.kind == "markdown" && artifact.title.starts_with("Resposta · "))
        .expect("the reply has its Markdown artifact");
    assert!(reply.title.ends_with(" UTC"), "{}", reply.title);
    assert_eq!(reply.content.as_deref(), Some("Pronto."));
    let app = find("code", "app.ts");
    assert_eq!(app.file_path.as_deref(), Some("src/app.ts"));
    let note = find("markdown", "a.md");
    assert_eq!(note.file_path.as_deref(), Some("docs/a.md"));
    let report = find("report", "Alterações do turno");
    assert_eq!(report.content.as_deref(), Some("--- a\n+++ b"));
    assert_eq!(artifacts.len(), 4, "one reply, two files and one report");
}

#[test]
fn ai_save_assistant_refuses_an_unknown_conversation_or_a_blocked_project_and_stores_nothing() {
    let scratch = Scratch::new("ai-save-assistant-refused");
    let project = scratch.path().join("project");
    std::fs::create_dir(&project).expect("the folder is created");
    let path = project.to_string_lossy().into_owned();
    let persistence = bootstrap::open(&scratch.path().join("data")).expect("the shell opens");
    let store = persistence.store.lock().expect("not poisoned");
    store
        .workspaces()
        .touch(&path)
        .expect("the workspace is saved");
    let conversation = services::conversation_create(&store, &[], &path).expect("created");
    let request = |conversation_id: &str| SaveAssistantRequest {
        conversation_id: conversation_id.to_string(),
        content: "Pronto.".to_string(),
        metadata: Some(json!({ "files": [{ "path": "a.md" }] })),
    };

    let unknown = services::ai_save_assistant(
        &store,
        &[],
        &request("3f0c7a52-8d1e-4b6a-9c2f-5e7d8a9b0c1d"),
    );
    assert!(matches!(unknown, Err(CommandError::UnknownConversation)));

    let blocked = vec![project.clone()];
    let refused = services::ai_save_assistant(&store, &blocked, &request(&conversation.id));
    assert!(matches!(refused, Err(CommandError::UnknownWorkspace)));

    let messages: i64 = store
        .connection()
        .query_row("SELECT COUNT(*) FROM messages", [], |row| row.get(0))
        .expect("the count is read");
    let artifacts: i64 = store
        .connection()
        .query_row("SELECT COUNT(*) FROM artifacts", [], |row| row.get(0))
        .expect("the count is read");
    assert_eq!(
        (messages, artifacts),
        (0, 0),
        "a refused reply stores nothing"
    );
}

#[test]
fn conversation_delete_removes_its_messages_and_artifacts_and_keeps_the_other_conversations() {
    let scratch = Scratch::new("conversation-delete");
    let project = scratch.path().join("project");
    std::fs::create_dir(&project).expect("the folder is created");
    let path = project.to_string_lossy().into_owned();
    let persistence = bootstrap::open(&scratch.path().join("data")).expect("the shell opens");
    let store = persistence.store.lock().expect("not poisoned");
    store
        .workspaces()
        .touch(&path)
        .expect("the workspace is saved");
    let doomed = services::conversation_create(&store, &[], &path).expect("created");
    let kept = services::conversation_create(&store, &[], &path).expect("created");
    for conversation in [&doomed, &kept] {
        services::ai_save_assistant(
            &store,
            &[],
            &SaveAssistantRequest {
                conversation_id: conversation.id.clone(),
                content: "Pronto.".to_string(),
                metadata: Some(json!({ "files": [{ "path": "a.md" }] })),
            },
        )
        .expect("the reply is saved");
    }
    let count = |table: &str, conversation: &str| -> i64 {
        store
            .connection()
            .query_row(
                &format!("SELECT COUNT(*) FROM {table} WHERE conversation_id=?1"),
                [conversation],
                |row| row.get(0),
            )
            .expect("the count is read")
    };
    assert_eq!(
        (
            count("messages", &doomed.id),
            count("artifacts", &doomed.id)
        ),
        (1, 2)
    );

    services::conversation_delete(&store, &[], &doomed.id).expect("the conversation is deleted");

    assert_eq!(
        (
            count("messages", &doomed.id),
            count("artifacts", &doomed.id)
        ),
        (0, 0)
    );
    assert_eq!(
        (count("messages", &kept.id), count("artifacts", &kept.id)),
        (1, 2),
        "the other conversation is intact"
    );
    assert!(
        store
            .conversations()
            .get(&doomed.id)
            .expect("read")
            .is_none()
    );
    assert!(store.conversations().get(&kept.id).expect("read").is_some());
}

#[test]
fn conversation_delete_refuses_a_malformed_id_a_missing_conversation_or_a_blocked_project() {
    let scratch = Scratch::new("conversation-delete-refused");
    let project = scratch.path().join("project");
    std::fs::create_dir(&project).expect("the folder is created");
    let path = project.to_string_lossy().into_owned();
    let persistence = bootstrap::open(&scratch.path().join("data")).expect("the shell opens");
    let store = persistence.store.lock().expect("not poisoned");
    store
        .workspaces()
        .touch(&path)
        .expect("the workspace is saved");
    let conversation = services::conversation_create(&store, &[], &path).expect("created");

    assert!(matches!(
        services::conversation_delete(&store, &[], "not-a-uuid"),
        Err(CommandError::InvalidInput)
    ));
    assert!(matches!(
        services::conversation_delete(&store, &[], "3f0c7a52-8d1e-4b6a-9c2f-5e7d8a9b0c1d"),
        Err(CommandError::UnknownConversation)
    ));
    let blocked = vec![project.clone()];
    assert!(matches!(
        services::conversation_delete(&store, &blocked, &conversation.id),
        Err(CommandError::UnknownWorkspace)
    ));
    assert!(
        store
            .conversations()
            .get(&conversation.id)
            .expect("read")
            .is_some(),
        "a refused delete removes nothing"
    );
}
