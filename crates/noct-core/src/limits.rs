//! Canonical limits, ported from `shared/constants.ts`.
//!
//! Values must stay identical to the TypeScript source while both implementations
//! coexist. `tests/constants_parity.rs` reads `shared/constants.ts` and fails on any
//! drift. Character limits and byte limits are separate on purpose: UTF-8 characters
//! have no fixed width, so the I/O boundary is enforced in bytes first.

/// Current SQLite schema version. Migrations must keep existing databases readable.
pub const DATABASE_SCHEMA_VERSION: u64 = 32;

pub mod renderer {
    pub const ACTIVITIES: u64 = 300;
    pub const ACTIVITY_DETAIL_CHARACTERS: u64 = 64_000;
    pub const CHAT_MESSAGES: u64 = 200;
    pub const STREAM_CHARACTERS: u64 = 2_000_000;
}

pub mod persistence {
    pub const ASSISTANT_CHARACTERS: u64 = 2_000_000;
    pub const DOCUMENT_CHARACTERS: u64 = 2_000_000;
    pub const DOCUMENT_NAME_CHARACTERS: u64 = 200;
    pub const METADATA_CHARACTERS: u64 = 500_000;
    /// The longest brain memory content, in UTF-16 units after trimming. `safeBrainMemoryContent` in
    /// `shared/ipc/schemas.ts` uses the same number.
    pub const BRAIN_MEMORY_CHARACTERS: u64 = 8_000;
    /// The longest result of a suggestion decision, in UTF-16 units. `suggestionStatusSchema` in `shared/ipc/schemas.ts`
    /// uses the same number.
    pub const SUGGESTION_RESULT_CHARACTERS: u64 = 20_000;
}

/// Byte limits for files read from an authorized workspace.
pub mod workspace_read {
    pub const ATTACHMENT_BYTES: u64 = 1_000_000;
    pub const WORKSPACE_CONTEXT_BYTES: u64 = 256_000;
    pub const PROJECT_METADATA_BYTES: u64 = 256_000;
    pub const PACKAGE_METADATA_BYTES: u64 = 1_000_000;
    pub const CODE_INDEX_BYTES: u64 = 8_000_000;
    pub const SUGGESTION_HISTORY_BYTES: u64 = 1_000_000;
    pub const DOCUMENT_BYTES: u64 = 2_000_000;
}

pub mod code_intelligence {
    pub const MAX_FILES: u64 = 50_000;
    pub const MAX_EXCLUSIONS: u64 = 2_000;
    pub const MAX_TRAVERSAL_ENTRIES: u64 = 250_000;
    pub const MAX_INDEXED_FILE_BYTES: u64 = 8_000_000;
    pub const MAX_PARSE_BYTES: u64 = 2_000_000;
    pub const MAX_OUTPUT_CHARACTERS: u64 = 20_000;
    pub const MAX_ERROR_CHARACTERS: u64 = 2_000;
    pub const MAX_QUERY_RESULTS: u64 = 100;
}

pub mod semantic_index {
    /// Units returned by a unit listing, when the caller asks for no limit or a larger one.
    pub const MAX_UNITS: u64 = 50_000;
    /// Lexical matches returned by one search, and the default when the caller gives no limit.
    pub const MAX_LEXICAL_RESULTS: u64 = 100;
    pub const DEFAULT_LEXICAL_RESULTS: u64 = 50;
    /// Rows returned by one page of a keyset listing (file units, indexed embeddings). A page
    /// request above this value is clamped to it.
    pub const PAGE_LIMIT_MAX: u64 = 500;
}

pub mod engineering_intelligence {
    pub const MAX_SIGNALS: u64 = 500;
    pub const MAX_EVIDENCE_PER_SIGNAL: u64 = 20;
    pub const MAX_SNAPSHOTS: u64 = 100;
    pub const MAX_INSIGHTS: u64 = 200;
    pub const MAX_TITLE_CHARACTERS: u64 = 200;
    pub const MAX_DESCRIPTION_CHARACTERS: u64 = 4_000;
    pub const MAX_EVIDENCE_DETAIL_CHARACTERS: u64 = 4_000;
}

pub mod ui_timing {
    pub const STREAM_FLUSH_MS: u64 = 80;
    pub const ACTIVITY_FLUSH_MS: u64 = 150;
    pub const DIAGNOSTICS_INTERVAL_MS: u64 = 10_000;
}

pub mod renderer_performance_budgets {
    pub const STARTUP_MS: u64 = 5_000;
    pub const CONVERSATION_LOAD_MS: u64 = 2_000;
    pub const LONG_TASK_MS: u64 = 50;
}

pub mod collection_page {
    pub const CONVERSATIONS: u64 = 100;
    pub const ARTIFACTS: u64 = 50;
    pub const SUGGESTIONS: u64 = 50;
    pub const BRAIN_MEMORIES: u64 = 50;
    pub const VALIDATION: u64 = 20;
}

/// Limits of provider definitions. Port of `PROVIDER_CONFIGURATION_LIMITS` in
/// `shared/ai/providerConfiguration.ts`.
///
/// `credentialCharacters` (64,000) is deliberately not ported. It was sized for the Windows
/// Credential Manager, whose 2,560-byte blob limit a UTF-16 secret of that length exceeds. The
/// Rust vault keeps secrets in an encrypted file and bounds the file instead (see `noct-secrets`).
/// The TypeScript constant stays until the TypeScript application is retired, and the difference
/// is recorded in the migration ADR.
pub mod provider_configuration {
    pub const IDENTIFIER_CHARACTERS: u64 = 512;
    pub const DISPLAY_NAME_CHARACTERS: u64 = 500;
    pub const BASE_URL_CHARACTERS: u64 = 2_048;
    pub const MINIMUM_TIMEOUT_MS: u64 = 1_000;
    pub const MAXIMUM_TIMEOUT_MS: u64 = 120_000;
}

/// Limits of a provider key typed in the settings. The key is sent as an HTTP header, so only visible ASCII is
/// accepted; the bounds are the ones the shell enforces before it stores anything.
pub mod provider_key {
    pub const MINIMUM_KEY_CHARACTERS: u64 = 8;
    pub const MAXIMUM_KEY_CHARACTERS: u64 = 512;
}

/// Limits of model descriptors. Port of `MODEL_LIMITS` in `shared/ai/model.ts`.
pub mod model {
    pub const IDENTIFIER_CHARACTERS: u64 = 512;
    pub const DISPLAY_NAME_CHARACTERS: u64 = 500;
    pub const FAMILY_CHARACTERS: u64 = 500;
    pub const VERSION_CHARACTERS: u64 = 200;
    pub const PRICING_EFFECTIVE_AT_CHARACTERS: u64 = 100;
}
