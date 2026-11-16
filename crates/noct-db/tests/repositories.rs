//! Behaviour of the repositories, checked against the rules of the TypeScript
//! repositories they port (`electron/database/*Repository.ts`).

use std::collections::BTreeMap;
use std::thread;
use std::time::Duration;

use noct_core::limits::persistence::METADATA_CHARACTERS;
use noct_db::repositories::conversations::{ConversationRow, Conversations, MessageRole};
use noct_db::repositories::settings::Settings;
use noct_db::repositories::workspaces::Workspaces;
use noct_db::{Database, Error, Result, transaction};
use serde_json::{Value, json};

mod common;
use common::scratch;

/// Timestamps have millisecond resolution; ordering assertions need distinct instants.
fn tick() {
    thread::sleep(Duration::from_millis(3));
}

fn open(name: &str) -> Database {
    Database::open_runtime(&scratch(name)).expect("open database")
}

fn title(conversations: &Conversations<'_>, id: &str) -> String {
    conversations
        .get(id)
        .expect("query")
        .expect("conversation exists")
        .title
}

fn ids(rows: &[ConversationRow]) -> Vec<String> {
    rows.iter().map(|row| row.id.clone()).collect()
}

#[test]
fn settings_apply_defaults_and_hide_runtime_keys() {
    let db = open("settings-defaults");
    let settings = Settings::new(db.connection()).get().unwrap();
    assert_eq!(settings["approvalPolicy"], "on-request");
    assert_eq!(settings["theme"], "dark");
    assert_eq!(settings["language"], "pt-BR");
    assert!(settings.keys().all(|key| !key.starts_with("maintenance.")));
}

#[test]
fn settings_round_trip_and_fall_back_on_unknown_values() {
    let db = open("settings-round-trip");
    let settings = Settings::new(db.connection());
    settings
        .set(&BTreeMap::from([
            ("theme".to_string(), "light".to_string()),
            ("approvalPolicy".to_string(), "untrusted".to_string()),
            ("language".to_string(), "en".to_string()),
            ("custom.flag".to_string(), "1".to_string()),
        ]))
        .unwrap();
    let stored = settings.get().unwrap();
    assert_eq!(stored["theme"], "light");
    assert_eq!(stored["approvalPolicy"], "untrusted");
    assert_eq!(stored["language"], "en");
    assert_eq!(stored["custom.flag"], "1");

    settings
        .set(&BTreeMap::from([("theme".to_string(), "neon".to_string())]))
        .unwrap();
    assert_eq!(settings.get().unwrap()["theme"], "dark");
}

#[test]
fn workspaces_list_favorites_first_then_most_recently_opened() {
    let db = open("workspaces-order");
    let workspaces = Workspaces::new(db.connection());
    workspaces.touch("/work/alpha").unwrap();
    tick();
    workspaces.touch("/work/beta").unwrap();
    tick();
    workspaces.touch("/work/gamma").unwrap();
    tick();
    workspaces.set_favorite("/work/alpha", true).unwrap();

    let order: Vec<String> = workspaces
        .list()
        .unwrap()
        .into_iter()
        .map(|row| row.path)
        .collect();
    assert_eq!(order, ["/work/alpha", "/work/gamma", "/work/beta"]);
}

#[test]
fn touch_names_the_workspace_after_its_folder_and_marks_it_authorized() {
    let db = open("workspaces-touch");
    let workspaces = Workspaces::new(db.connection());
    workspaces.touch("/home/dev/projects/app/").unwrap();

    let row = workspaces.list().unwrap().remove(0);
    assert_eq!(row.name, "app");
    assert!(row.authorized);
    assert!(!row.favorite);
}

#[test]
fn removing_a_workspace_forgets_it_and_favorites_toggle() {
    let db = open("workspaces-remove");
    let workspaces = Workspaces::new(db.connection());
    workspaces.touch("/work/alpha").unwrap();

    workspaces.set_favorite("/work/alpha", true).unwrap();
    assert!(workspaces.list().unwrap()[0].favorite);
    workspaces.set_favorite("/work/alpha", false).unwrap();
    assert!(!workspaces.list().unwrap()[0].favorite);

    workspaces.remove("/work/alpha").unwrap();
    assert!(workspaces.list().unwrap().is_empty());
}

#[test]
fn conversations_page_newest_first_and_report_more_pages() {
    let db = open("conversations-page");
    let conversations = Conversations::new(db.connection());
    let first = conversations.create("/work/alpha").unwrap();
    tick();
    let second = conversations.create("/work/alpha").unwrap();
    tick();
    let third = conversations.create("/work/alpha").unwrap();

    let page = conversations.page(0, 2).unwrap();
    assert_eq!(ids(&page.items), [third.id.clone(), second.id.clone()]);
    assert!(page.has_more);

    let rest = conversations.page(2, 2).unwrap();
    assert_eq!(rest.items.len(), 1);
    assert_eq!(rest.items[0].id, first.id);
    assert!(!rest.has_more);
}

#[test]
fn titles_follow_the_first_prompt_with_collapsed_whitespace() {
    let db = open("conversation-title");
    let conversations = Conversations::new(db.connection());
    let conversation = conversations.create("/work/alpha").unwrap();

    conversations
        .rename_from_prompt(&conversation.id, "  Olá   mundo\n\tcom  espaços  ")
        .unwrap();
    assert_eq!(
        title(&conversations, &conversation.id),
        "Olá mundo com espaços"
    );
}

#[test]
fn titles_keep_fifty_two_utf16_units_and_fall_back_when_empty() {
    let db = open("conversation-title-limits");
    let conversations = Conversations::new(db.connection());
    let conversation = conversations.create("/work/alpha").unwrap();

    conversations
        .rename_from_prompt(&conversation.id, &"a".repeat(60))
        .unwrap();
    assert_eq!(title(&conversations, &conversation.id), "a".repeat(52));

    // Each emoji is two UTF-16 code units, so 52 units hold 26 of them.
    conversations
        .rename_from_prompt(&conversation.id, &"😀".repeat(30))
        .unwrap();
    assert_eq!(title(&conversations, &conversation.id), "😀".repeat(26));

    // A byte-order mark is JavaScript whitespace, so it is trimmed.
    conversations
        .rename_from_prompt(&conversation.id, "\u{feff}Título")
        .unwrap();
    assert_eq!(title(&conversations, &conversation.id), "Título");

    conversations
        .rename_from_prompt(&conversation.id, "\u{feff} \t\n ")
        .unwrap();
    assert_eq!(title(&conversations, &conversation.id), "Nova conversa");
}

#[test]
fn messages_keep_insertion_order_and_page_from_the_newest() {
    let db = open("messages-order");
    let conversations = Conversations::new(db.connection());
    let conversation = conversations.create("/work/alpha").unwrap();
    // Inserts that share a millisecond are ordered by rowid, so insertion order holds.
    for text in ["one", "two", "three"] {
        conversations
            .add_message(&conversation.id, MessageRole::User, text, None)
            .unwrap();
    }

    let texts = |rows: &[noct_db::repositories::conversations::MessageRow]| -> Vec<String> {
        rows.iter().map(|row| row.content.clone()).collect()
    };
    let all = conversations.list_messages(&conversation.id).unwrap();
    assert_eq!(texts(&all), ["one", "two", "three"]);

    let newest = conversations.page_messages(&conversation.id, 0, 2).unwrap();
    assert_eq!(texts(&newest.items), ["two", "three"]);
    assert!(newest.has_more);

    let oldest = conversations.page_messages(&conversation.id, 2, 2).unwrap();
    assert_eq!(texts(&oldest.items), ["one"]);
    assert!(!oldest.has_more);
}

#[test]
fn recent_messages_skip_system_entries_and_bound_the_window() {
    let db = open("messages-recent");
    let conversations = Conversations::new(db.connection());
    let conversation = conversations.create("/work/alpha").unwrap();
    conversations
        .add_message(&conversation.id, MessageRole::System, "setup", None)
        .unwrap();
    conversations
        .add_message(&conversation.id, MessageRole::User, "question", None)
        .unwrap();
    conversations
        .add_message(&conversation.id, MessageRole::Assistant, "answer", None)
        .unwrap();

    let texts = |rows: Vec<noct_db::repositories::conversations::MessageRow>| -> Vec<String> {
        rows.into_iter().map(|row| row.content).collect()
    };
    assert_eq!(
        texts(
            conversations
                .list_recent_messages(&conversation.id, 10)
                .unwrap()
        ),
        ["question", "answer"]
    );
    // The window is clamped to at least one message.
    assert_eq!(
        texts(
            conversations
                .list_recent_messages(&conversation.id, 0)
                .unwrap()
        ),
        ["answer"]
    );
}

#[test]
fn metadata_is_stored_as_json_and_bounded_by_size_and_depth() {
    let db = open("message-metadata");
    let conversations = Conversations::new(db.connection());
    let conversation = conversations.create("/work/alpha").unwrap();

    let stored = conversations
        .add_message(
            &conversation.id,
            MessageRole::Assistant,
            "ok",
            Some(&json!({ "paths": [1, "b"] })),
        )
        .unwrap();
    assert_eq!(stored.metadata.as_deref(), Some(r#"{"paths":[1,"b"]}"#));

    // Null is stored as NULL, like an absent value.
    let without = conversations
        .add_message(
            &conversation.id,
            MessageRole::User,
            "no metadata",
            Some(&Value::Null),
        )
        .unwrap();
    assert_eq!(without.metadata, None);

    let oversized = json!({ "text": "a".repeat(METADATA_CHARACTERS as usize) });
    match conversations.add_message(&conversation.id, MessageRole::User, "big", Some(&oversized)) {
        Err(Error::Message(message)) => assert_eq!(
            message,
            "O metadata precisa conter apenas valores JSON válidos dentro do limite permitido."
        ),
        other => panic!("oversized metadata must be rejected, got {other:?}"),
    }

    let mut deep = json!(null);
    for _ in 0..33 {
        deep = json!([deep]);
    }
    assert!(
        conversations
            .add_message(&conversation.id, MessageRole::User, "deep", Some(&deep))
            .is_err()
    );

    // Rejected writes leave nothing behind.
    assert_eq!(
        conversations.list_messages(&conversation.id).unwrap().len(),
        2
    );
}

#[test]
fn adding_a_message_moves_its_conversation_to_the_top() {
    let db = open("conversation-activity");
    let conversations = Conversations::new(db.connection());
    let older = conversations.create("/work/alpha").unwrap();
    tick();
    let newer = conversations.create("/work/alpha").unwrap();
    tick();
    assert_eq!(
        ids(&conversations.list().unwrap()),
        [newer.id.clone(), older.id.clone()]
    );

    conversations
        .add_message(&older.id, MessageRole::User, "again", None)
        .unwrap();
    assert_eq!(ids(&conversations.list().unwrap()), [older.id, newer.id]);
}

/// Walks the keyset pages of `size` rows from the first page to the end.
fn walk(conversations: &Conversations<'_>, size: usize) -> Vec<String> {
    let mut seen = Vec::new();
    let mut after: Option<(String, String)> = None;
    loop {
        let rows = conversations
            .page_after(
                after.as_ref().map(|(at, id)| (at.as_str(), id.as_str())),
                size,
            )
            .unwrap();
        let Some(last) = rows.last() else {
            return seen;
        };
        after = Some((last.updated_at.clone(), last.id.clone()));
        seen.extend(rows.iter().map(|row| row.id.clone()));
    }
}

#[test]
fn keyset_pages_visit_every_conversation_once_in_the_list_order() {
    let db = open("conversation-keyset");
    let conversations = Conversations::new(db.connection());
    for _ in 0..5 {
        conversations.create("/work/alpha").unwrap();
        tick();
    }
    let all = ids(&conversations.list().unwrap());

    for size in [1, 2, 3, 5, 10] {
        assert_eq!(walk(&conversations, size), all, "pages of {size}");
    }
}

/// Equal timestamps are broken by identifier, in descending order, so the walk is still a total order.
#[test]
fn equal_timestamps_are_ordered_by_identifier_for_the_pages() {
    let db = open("conversation-keyset-ties");
    let conversations = Conversations::new(db.connection());
    for _ in 0..4 {
        conversations.create("/work/alpha").unwrap();
    }
    db.connection()
        .execute(
            "UPDATE conversations SET updated_at = '2026-01-01T00:00:00.000Z'",
            [],
        )
        .unwrap();
    let mut expected = ids(&conversations.list().unwrap());
    expected.sort();
    expected.reverse();

    for size in [1, 3, 10] {
        assert_eq!(walk(&conversations, size), expected, "pages of {size}");
    }
}

#[test]
fn nested_units_roll_back_only_their_own_writes() {
    let db = open("transactions-nested");
    let conn = db.connection();
    transaction::run(conn, |conn| {
        Settings::new(conn).set(&BTreeMap::from([("outer".to_string(), "1".to_string())]))?;
        let inner: Result<()> = transaction::run(conn, |conn| {
            Settings::new(conn).set(&BTreeMap::from([("inner".to_string(), "1".to_string())]))?;
            Err(Error::Message("inner failed".to_string()))
        });
        assert!(inner.is_err());
        Ok(())
    })
    .unwrap();

    let stored = Settings::new(conn).get().unwrap();
    assert_eq!(stored["outer"], "1");
    assert!(!stored.contains_key("inner"));
    assert!(conn.is_autocommit());
}

#[test]
fn a_failed_top_level_unit_leaves_no_partial_writes() {
    let db = open("transactions-failed");
    let conn = db.connection();
    let result: Result<()> = transaction::run(conn, |conn| {
        Settings::new(conn).set(&BTreeMap::from([("partial".to_string(), "1".to_string())]))?;
        Err(Error::Message("boom".to_string()))
    });
    assert!(result.is_err());
    assert!(!Settings::new(conn).get().unwrap().contains_key("partial"));
    assert!(
        conn.is_autocommit(),
        "the failed unit must not leave a transaction open"
    );
}
