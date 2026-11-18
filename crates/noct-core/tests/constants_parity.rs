//! Migration guard: every canonical limit must match `ts-reference/constants.ts.txt` exactly.
//!
//! The TypeScript file stays the single source of truth until the TypeScript
//! implementation is retired; this test is removed with it.

use noct_core::limits as l;
use std::collections::HashMap;

const TS_CONSTANTS: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/fixtures/ts-reference/constants.ts.txt"
);

fn is_identifier(text: &str) -> bool {
    let mut chars = text.chars();
    matches!(chars.next(), Some(c) if c.is_ascii_alphabetic() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// Collects `key: number` pairs. Splitting on `{`, `}` and `,` catches both
/// multi-line objects and single-line ones such as `{ conversations: 100, ... }`.
fn typescript_values(source: &str) -> HashMap<String, u64> {
    let mut values = HashMap::new();
    for piece in source.split(['{', '}', ',', '\n']) {
        let Some((key, rest)) = piece.split_once(':') else {
            continue;
        };
        let key = key.trim();
        if !is_identifier(key) {
            continue;
        }
        let digits: String = rest
            .trim()
            .chars()
            .take_while(|c| c.is_ascii_digit() || *c == '_')
            .collect();
        if digits.is_empty() {
            continue;
        }
        let value: u64 = digits.replace('_', "").parse().expect("numeric limit");
        let previous = values.insert(key.to_string(), value);
        assert!(
            previous.is_none(),
            "duplicate key `{key}` in ts-reference/constants.ts.txt"
        );
    }
    values
}

fn schema_version(source: &str) -> u64 {
    let line = source
        .lines()
        .find(|line| line.starts_with("export const DATABASE_SCHEMA_VERSION = "))
        .expect("DATABASE_SCHEMA_VERSION declared in ts-reference/constants.ts.txt");
    line.rsplit('=')
        .next()
        .unwrap()
        .trim()
        .parse()
        .expect("numeric schema version")
}

#[test]
fn canonical_limits_match_typescript() {
    let source =
        std::fs::read_to_string(TS_CONSTANTS).expect("ts-reference/constants.ts.txt is readable");
    let ts = typescript_values(&source);

    let checks: &[(&str, u64)] = &[
        ("activities", l::renderer::ACTIVITIES),
        (
            "activityDetailCharacters",
            l::renderer::ACTIVITY_DETAIL_CHARACTERS,
        ),
        ("chatMessages", l::renderer::CHAT_MESSAGES),
        ("streamCharacters", l::renderer::STREAM_CHARACTERS),
        ("assistantCharacters", l::persistence::ASSISTANT_CHARACTERS),
        ("documentCharacters", l::persistence::DOCUMENT_CHARACTERS),
        (
            "documentNameCharacters",
            l::persistence::DOCUMENT_NAME_CHARACTERS,
        ),
        ("metadataCharacters", l::persistence::METADATA_CHARACTERS),
        ("attachmentBytes", l::workspace_read::ATTACHMENT_BYTES),
        (
            "workspaceContextBytes",
            l::workspace_read::WORKSPACE_CONTEXT_BYTES,
        ),
        (
            "projectMetadataBytes",
            l::workspace_read::PROJECT_METADATA_BYTES,
        ),
        (
            "packageMetadataBytes",
            l::workspace_read::PACKAGE_METADATA_BYTES,
        ),
        ("codeIndexBytes", l::workspace_read::CODE_INDEX_BYTES),
        (
            "suggestionHistoryBytes",
            l::workspace_read::SUGGESTION_HISTORY_BYTES,
        ),
        ("documentBytes", l::workspace_read::DOCUMENT_BYTES),
        ("maxFiles", l::code_intelligence::MAX_FILES),
        ("maxExclusions", l::code_intelligence::MAX_EXCLUSIONS),
        (
            "maxTraversalEntries",
            l::code_intelligence::MAX_TRAVERSAL_ENTRIES,
        ),
        (
            "maxIndexedFileBytes",
            l::code_intelligence::MAX_INDEXED_FILE_BYTES,
        ),
        ("maxParseBytes", l::code_intelligence::MAX_PARSE_BYTES),
        (
            "maxOutputCharacters",
            l::code_intelligence::MAX_OUTPUT_CHARACTERS,
        ),
        (
            "maxErrorCharacters",
            l::code_intelligence::MAX_ERROR_CHARACTERS,
        ),
        ("maxQueryResults", l::code_intelligence::MAX_QUERY_RESULTS),
        ("maxSignals", l::engineering_intelligence::MAX_SIGNALS),
        (
            "maxEvidencePerSignal",
            l::engineering_intelligence::MAX_EVIDENCE_PER_SIGNAL,
        ),
        ("maxSnapshots", l::engineering_intelligence::MAX_SNAPSHOTS),
        ("maxInsights", l::engineering_intelligence::MAX_INSIGHTS),
        (
            "maxTitleCharacters",
            l::engineering_intelligence::MAX_TITLE_CHARACTERS,
        ),
        (
            "maxDescriptionCharacters",
            l::engineering_intelligence::MAX_DESCRIPTION_CHARACTERS,
        ),
        (
            "maxEvidenceDetailCharacters",
            l::engineering_intelligence::MAX_EVIDENCE_DETAIL_CHARACTERS,
        ),
        ("streamFlushMs", l::ui_timing::STREAM_FLUSH_MS),
        ("activityFlushMs", l::ui_timing::ACTIVITY_FLUSH_MS),
        (
            "diagnosticsIntervalMs",
            l::ui_timing::DIAGNOSTICS_INTERVAL_MS,
        ),
        ("startupMs", l::renderer_performance_budgets::STARTUP_MS),
        (
            "conversationLoadMs",
            l::renderer_performance_budgets::CONVERSATION_LOAD_MS,
        ),
        ("longTaskMs", l::renderer_performance_budgets::LONG_TASK_MS),
        ("conversations", l::collection_page::CONVERSATIONS),
        ("artifacts", l::collection_page::ARTIFACTS),
        ("suggestions", l::collection_page::SUGGESTIONS),
        ("brainMemories", l::collection_page::BRAIN_MEMORIES),
        ("validation", l::collection_page::VALIDATION),
    ];

    for (key, expected) in checks {
        assert_eq!(
            ts.get(*key),
            Some(expected),
            "limit `{key}` drifted from ts-reference/constants.ts.txt"
        );
    }
    assert_eq!(
        checks.len(),
        ts.len(),
        "ts-reference/constants.ts.txt gained or lost a numeric key; mirror it in noct_core::limits"
    );
}

#[test]
fn schema_version_matches_typescript() {
    let source =
        std::fs::read_to_string(TS_CONSTANTS).expect("ts-reference/constants.ts.txt is readable");
    assert_eq!(schema_version(&source), l::DATABASE_SCHEMA_VERSION);
}

const TS_PROVIDER_CONFIGURATION: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/fixtures/ts-reference/providerConfiguration.ts.txt"
);

#[test]
fn provider_configuration_limits_match_typescript() {
    let source = std::fs::read_to_string(TS_PROVIDER_CONFIGURATION)
        .expect("ts-reference/providerConfiguration.ts.txt is readable");
    let ts = typescript_values(&source);

    let checks: &[(&str, u64)] = &[
        (
            "identifierCharacters",
            l::provider_configuration::IDENTIFIER_CHARACTERS,
        ),
        (
            "displayNameCharacters",
            l::provider_configuration::DISPLAY_NAME_CHARACTERS,
        ),
        (
            "baseUrlCharacters",
            l::provider_configuration::BASE_URL_CHARACTERS,
        ),
        (
            "minimumTimeoutMs",
            l::provider_configuration::MINIMUM_TIMEOUT_MS,
        ),
        (
            "maximumTimeoutMs",
            l::provider_configuration::MAXIMUM_TIMEOUT_MS,
        ),
    ];
    for (name, value) in checks {
        assert_eq!(
            ts.get(*name),
            Some(value),
            "{name} in providerConfiguration.ts"
        );
    }
    // The TypeScript application still defines credentialCharacters. Rust does not port it on
    // purpose, so the count is one more than the checks, and the difference is pinned here.
    assert_eq!(
        ts.get("credentialCharacters"),
        Some(&64_000),
        "the TypeScript constant stays until the TypeScript application is retired"
    );
    assert_eq!(
        ts.len(),
        checks.len() + 1,
        "every provider limit is covered, except the one that is not ported"
    );
}

const TS_MODEL: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/fixtures/ts-reference/model.ts.txt"
);

#[test]
fn model_limits_match_typescript() {
    let source = std::fs::read_to_string(TS_MODEL).expect("ts-reference/model.ts.txt is readable");
    let ts = typescript_values(&source);

    let checks: &[(&str, u64)] = &[
        ("identifierCharacters", l::model::IDENTIFIER_CHARACTERS),
        ("displayNameCharacters", l::model::DISPLAY_NAME_CHARACTERS),
        ("familyCharacters", l::model::FAMILY_CHARACTERS),
        ("versionCharacters", l::model::VERSION_CHARACTERS),
        (
            "pricingEffectiveAtCharacters",
            l::model::PRICING_EFFECTIVE_AT_CHARACTERS,
        ),
    ];
    for (name, value) in checks {
        assert_eq!(ts.get(*name), Some(value), "{name} in model.ts");
    }
    assert_eq!(ts.len(), checks.len(), "every model limit is covered");
}
