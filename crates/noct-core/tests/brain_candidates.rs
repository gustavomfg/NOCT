//! The reading of the memory blocks of a reply, checked against `extractBrainMemoryCandidates` in `shared/suggestions.ts`
//! and the request of `brain_extract` against `brainMemoryExtractSchema`.

use noct_core::brain_memory::{
    BrainMemoryKind, BrainMemoryScope, EXTRACTED_CANDIDATES_MAX, extract_candidates,
    normalize_brain_content,
};
use noct_core::ipc::brain::BrainMemoryExtractRequest;
use serde_json::json;

const CONVERSATION: &str = "3f0c7a52-8d1e-4b6a-9c2f-5e7d8a9b0c1d";

fn block(body: &str) -> String {
    format!("```nocturne-memories\n{body}\n```")
}

#[test]
fn a_block_yields_its_candidates_and_is_removed_from_the_reply() {
    let reply = format!(
        "Decidi usar SQLite.\n\n{}",
        block(
            r#"[{"kind":"decision","scope":"workspace","content":"  Use SQLite for the store  ","confidence":85}]"#
        )
    );
    let extracted = extract_candidates(&reply);

    assert_eq!(extracted.content, "Decidi usar SQLite.");
    assert_eq!(extracted.candidates.len(), 1);
    let candidate = &extracted.candidates[0];
    assert_eq!(candidate.kind, BrainMemoryKind::Decision);
    assert_eq!(candidate.scope, BrainMemoryScope::Workspace);
    assert_eq!(
        candidate.content, "Use SQLite for the store",
        "the content is trimmed"
    );
    assert_eq!(candidate.confidence, 85);
}

#[test]
fn an_item_that_does_not_match_the_schema_is_skipped_and_the_others_are_kept() {
    let reply = block(
        r#"[
            {"kind":"fact","scope":"workspace","content":"The app is local"},
            {"kind":"opinion","scope":"workspace","content":"Unknown kind"},
            {"kind":"fact","scope":"workspace","content":"Extra key","theme":"dark"},
            {"kind":"fact","scope":"workspace","content":"token=abcdefghijklmnop"},
            {"kind":"fact","scope":"workspace","content":"   "},
            {"kind":"fact","scope":"workspace","content":"Too confident","confidence":101},
            {"kind":"fact","scope":"workspace","content":"Whole decimal","confidence":70.0},
            {"kind":"fact","scope":"workspace","content":"Half decimal","confidence":70.5},
            {"kind":"learning","scope":"conversation","content":"Last one"}
        ]"#,
    );
    let contents: Vec<String> = extract_candidates(&reply)
        .candidates
        .into_iter()
        .map(|candidate| candidate.content)
        .collect();
    assert_eq!(
        contents,
        vec!["The app is local", "Whole decimal", "Last one"]
    );
}

#[test]
fn confidence_defaults_to_60_and_a_single_object_counts_as_one_item() {
    let reply =
        block(r#"{"kind":"preference","scope":"workspace","content":"Prefer small commits"}"#);
    let extracted = extract_candidates(&reply);
    assert_eq!(extracted.candidates.len(), 1);
    assert_eq!(extracted.candidates[0].confidence, 60);
}

#[test]
fn a_block_that_is_not_json_is_skipped_and_still_removed() {
    let reply = format!(
        "Resposta.\n{}\n{}",
        block("[{\"kind\":\"fact\",\"scope\":\"workspace\",\"content\":"),
        block(r#"[{"kind":"fact","scope":"workspace","content":"A later block"}]"#)
    );
    let extracted = extract_candidates(&reply);
    assert_eq!(extracted.content, "Resposta.");
    assert_eq!(extracted.candidates.len(), 1);
    assert_eq!(extracted.candidates[0].content, "A later block");
}

#[test]
fn at_most_five_candidates_are_kept_and_every_block_is_removed() {
    let blocks: Vec<String> = (0..7)
        .map(|i| {
            block(&format!(
                r#"[{{"kind":"fact","scope":"workspace","content":"Fact number {i}"}}]"#
            ))
        })
        .collect();
    let reply = format!("Texto {}", blocks.join("\n"));
    let extracted = extract_candidates(&reply);

    assert_eq!(extracted.candidates.len(), EXTRACTED_CANDIDATES_MAX);
    assert_eq!(extracted.candidates[4].content, "Fact number 4");
    assert_eq!(
        extracted.content, "Texto",
        "the two blocks beyond the cap are removed too"
    );
}

#[test]
fn the_marker_folds_ascii_letters_only() {
    let upper = "```NOCTURNE-MEMORIES\n[{\"kind\":\"fact\",\"scope\":\"workspace\",\"content\":\"Upper case marker\"}]\n```";
    assert_eq!(
        extract_candidates(upper).candidates.len(),
        1,
        "ASCII letters fold"
    );

    // The long s folds to S in Unicode, but JavaScript's /i without u does not match it to the ASCII s.
    let long_s = "```nocturne-memorieſ\n[{\"kind\":\"fact\",\"scope\":\"workspace\",\"content\":\"No match\"}]\n```";
    let extracted = extract_candidates(long_s);
    assert!(
        extracted.candidates.is_empty(),
        "a non-ASCII look-alike is not a marker"
    );
    assert_eq!(extracted.content, long_s.trim());
}

#[test]
fn a_reply_without_blocks_is_only_trimmed() {
    let extracted = extract_candidates("  Só texto.  \n");
    assert!(extracted.candidates.is_empty());
    assert_eq!(extracted.content, "Só texto.");
}

#[test]
fn the_content_rule_trims_then_checks_length_and_credentials() {
    assert_eq!(normalize_brain_content("  note  ").as_deref(), Some("note"));
    assert_eq!(normalize_brain_content(" \t "), None);
    assert_eq!(normalize_brain_content(&"a".repeat(8_001)), None);
    assert_eq!(
        normalize_brain_content(&"a".repeat(8_000)).map(|text| text.len()),
        Some(8_000)
    );
    assert_eq!(normalize_brain_content("api_key: 0123456789abcdef"), None);
}

#[test]
fn the_extract_request_keeps_the_reply_as_it_is() {
    let request: BrainMemoryExtractRequest = serde_json::from_value(json!({
        "conversationId": CONVERSATION,
        "content": "  reply with spaces  \n",
    }))
    .expect("valid");
    assert_eq!(
        request.content, "  reply with spaces  \n",
        "the reply is not trimmed by the request"
    );

    let too_long = json!({ "conversationId": CONVERSATION, "content": "a".repeat(2_000_001) });
    assert!(serde_json::from_value::<BrainMemoryExtractRequest>(too_long).is_err());
    let bad_id = json!({ "conversationId": "not-a-uuid", "content": "x" });
    assert!(serde_json::from_value::<BrainMemoryExtractRequest>(bad_id).is_err());
    let extra = json!({ "conversationId": CONVERSATION, "content": "x", "mode": "build" });
    assert!(serde_json::from_value::<BrainMemoryExtractRequest>(extra).is_err());
}
