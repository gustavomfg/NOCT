//! The semantic index, checked against `SemanticIndexRepository.ts`. The row gate compares the
//! reads and the write scenario with the TypeScript repository; these tests cover the write rules,
//! the embedding layout, the bounds of the queries and the evidence of a run.

use noct_core::code_intelligence::{
    DiscoveryFileClassification, ProjectIndexFile, ProjectIndexFileState,
};
use noct_core::semantic_index::{
    SemanticEmbeddingSpace, SemanticIndexRun, SemanticRunKind, SemanticRunStatus,
    SemanticSearchFilters, SemanticUnit, SemanticUnitKind, SemanticUnitLocation,
    SemanticUnitStatus,
};
use noct_core::workspace_evidence::EvidenceKind;
use noct_db::Database;
use noct_db::repositories::project_index::ProjectIndex;
use noct_db::repositories::semantic_index::{
    PersistedSemanticUnit, SemanticIndex, StoredSemanticEmbedding,
};
use noct_db::repositories::workspace_evidence::WorkspaceEvidence;

mod common;
use common::scratch;

const AT: &str = "2026-01-01T00:00:00.000Z";
const ALPHA: &str = "/work/alpha";
const BETA: &str = "/work/beta";
const APP: &str = "src/app.ts";
const UTIL: &str = "src/util.ts";

/// Every unit of a file, read through the streaming iterator.
fn file_units(index: &SemanticIndex, workspace: &str, path: &str) -> Vec<SemanticUnit> {
    index
        .file_units(workspace, path)
        .collect::<noct_db::Result<Vec<_>>>()
        .expect("file units")
}

/// Every embedding of a space, read through the streaming iterator.
fn embeddings(
    index: &SemanticIndex,
    workspace: &str,
    space: &SemanticEmbeddingSpace,
    filters: &SemanticSearchFilters,
) -> Vec<StoredSemanticEmbedding> {
    index
        .indexed_embeddings(workspace, space, filters)
        .collect::<noct_db::Result<Vec<_>>>()
        .expect("embeddings")
}

fn open(name: &str) -> Database {
    let db = Database::open(&scratch(name)).expect("open database");
    db.connection()
        .execute_batch(&format!(
            "INSERT INTO workspaces(path,name,created_at,last_opened_at) VALUES \
                 ('{ALPHA}','alpha','{AT}','{AT}'),('{BETA}','beta','{AT}','{AT}');"
        ))
        .expect("workspaces");
    let index = ProjectIndex::new(db.connection());
    for (workspace, path) in [
        (ALPHA, APP),
        (ALPHA, UTIL),
        (ALPHA, "src/broken.ts"),
        (BETA, APP),
    ] {
        index
            .upsert_file(&project_file(workspace, path))
            .expect("project file");
    }
    db
}

fn project_file(workspace: &str, relative_path: &str) -> ProjectIndexFile {
    ProjectIndexFile {
        workspace: workspace.to_string(),
        relative_path: relative_path.to_string(),
        classification: DiscoveryFileClassification::Source,
        language: "TypeScript".to_string(),
        extension: ".ts".to_string(),
        size: 10,
        mtime_ms: 1000.0,
        ctime_ms: 1000.0,
        mode: 420,
        observed_hash: None,
        analyzed_hash: None,
        state: ProjectIndexFileState::Discovered,
        excluded: false,
        exclusion_reason: None,
        parser_id: None,
        parser_version: None,
        error: None,
        discovered_at: AT.to_string(),
        analyzed_at: None,
    }
}

fn digest(character: char) -> String {
    character.to_string().repeat(64)
}

fn space(dimensions: i64) -> SemanticEmbeddingSpace {
    SemanticEmbeddingSpace {
        provider_id: "local".to_string(),
        model_id: "mini".to_string(),
        model_version: None,
        dimensions,
    }
}

fn unit(
    id: &str,
    workspace: &str,
    relative_path: &str,
    status: SemanticUnitStatus,
    text: &str,
) -> SemanticUnit {
    SemanticUnit {
        id: id.to_string(),
        workspace: workspace.to_string(),
        relative_path: relative_path.to_string(),
        kind: SemanticUnitKind::Symbol,
        language: Some("TypeScript".to_string()),
        symbol_id: None,
        symbol_name: Some(id.to_string()),
        location: SemanticUnitLocation {
            start_line: 1,
            start_column: 1,
            end_line: 3,
            end_column: 2,
            start_offset: None,
            end_offset: None,
        },
        source_hash: digest('a'),
        chunk_hash: digest('b'),
        normalized_text: text.to_string(),
        chunk_strategy_version: "symbols-v2".to_string(),
        embedding_space: None,
        status,
        error: None,
        created_at: AT.to_string(),
        updated_at: AT.to_string(),
        indexed_at: None,
    }
}

/// A unit stored with an embedding in `space`. The caller chooses the status; the vector's length
/// is not checked against `space` on write, as in the TypeScript repository.
fn embedded(
    mut value: SemanticUnit,
    embedding: &[f64],
    space: SemanticEmbeddingSpace,
) -> PersistedSemanticUnit {
    value.embedding_space = Some(space);
    PersistedSemanticUnit {
        unit: value,
        embedding: Some(embedding.to_vec()),
    }
}

fn plain(value: SemanticUnit) -> PersistedSemanticUnit {
    PersistedSemanticUnit {
        unit: value,
        embedding: None,
    }
}

fn run(id: &str, status: SemanticRunStatus, started_at: &str) -> SemanticIndexRun {
    SemanticIndexRun {
        id: id.to_string(),
        workspace: ALPHA.to_string(),
        kind: SemanticRunKind::Initial,
        status,
        total_files: 2,
        processed_files: 1,
        indexed_units: 0,
        lexical_only_units: 0,
        failed_files: 0,
        excluded_files: 0,
        started_at: started_at.to_string(),
        completed_at: None,
        updated_at: started_at.to_string(),
        error: None,
    }
}

fn filters() -> SemanticSearchFilters {
    SemanticSearchFilters::default()
}

#[test]
fn records_an_ended_run_as_semantic_evidence_only() {
    let db = open("semantic-evidence");
    let conn = db.connection();
    let index = SemanticIndex::new(conn);
    index
        .create_run(&run("sem-1", SemanticRunStatus::Running, AT))
        .expect("create");
    assert_eq!(
        index.latest_run(ALPHA).expect("latest"),
        Some(run("sem-1", SemanticRunStatus::Running, AT))
    );

    index
        .update_run(&SemanticIndexRun {
            status: SemanticRunStatus::Completed,
            completed_at: Some(AT.to_string()),
            ..run("sem-1", SemanticRunStatus::Completed, AT)
        })
        .expect("complete");

    let evidence = WorkspaceEvidence::new(conn)
        .list(ALPHA, None)
        .expect("evidence");
    let semantic: Vec<_> = evidence
        .iter()
        .filter(|manifest| manifest.kind == EvidenceKind::SemanticIndex)
        .collect();
    assert_eq!(semantic.len(), 1);
    assert_eq!(semantic[0].source_id, "sem-1");
}

#[test]
fn replaces_a_file_and_removes_the_units_it_no_longer_has() {
    let db = open("semantic-replace");
    let index = SemanticIndex::new(db.connection());
    let mut first = unit("u1", ALPHA, APP, SemanticUnitStatus::Indexed, "main entry");
    first.location.start_offset = Some(0);
    first.location.end_offset = Some(12);
    index
        .replace_file_units(
            ALPHA,
            APP,
            &[
                embedded(first.clone(), &[0.5, -0.25, 1.5], space(3)),
                plain(unit(
                    "u2",
                    ALPHA,
                    APP,
                    SemanticUnitStatus::LexicalOnly,
                    "notes",
                )),
            ],
        )
        .expect("first replacement");

    let mut second = unit(
        "u2",
        ALPHA,
        APP,
        SemanticUnitStatus::Pending,
        "notes changed",
    );
    second.error = Some("Pendente.".to_string());
    index
        .replace_file_units(
            ALPHA,
            APP,
            &[
                plain(second.clone()),
                plain(unit("u3", ALPHA, APP, SemanticUnitStatus::Pending, "third")),
            ],
        )
        .expect("second replacement");

    let units = file_units(&index, ALPHA, APP);
    assert_eq!(
        units
            .iter()
            .map(|unit| unit.id.as_str())
            .collect::<Vec<_>>(),
        ["u2", "u3"]
    );
    assert_eq!(units[0], second);
    assert_eq!(index.get_unit(ALPHA, "u1").expect("get"), None);
    let stored = index.get_unit(ALPHA, "u3").expect("get").expect("u3");
    assert_eq!(stored.location.start_offset, None);
}

#[test]
fn an_empty_replacement_removes_every_unit_of_the_file() {
    let db = open("semantic-empty");
    let index = SemanticIndex::new(db.connection());
    index
        .replace_file_units(
            ALPHA,
            UTIL,
            &[plain(unit(
                "u1",
                ALPHA,
                UTIL,
                SemanticUnitStatus::Pending,
                "helper",
            ))],
        )
        .expect("insert");
    index.replace_file_units(ALPHA, UTIL, &[]).expect("clear");
    assert!(file_units(&index, ALPHA, UTIL).is_empty());
}

#[test]
fn an_invalid_embedding_rolls_back_the_whole_replacement() {
    let db = open("semantic-rollback");
    let index = SemanticIndex::new(db.connection());
    index
        .replace_file_units(
            ALPHA,
            APP,
            &[embedded(
                unit("u1", ALPHA, APP, SemanticUnitStatus::Indexed, "kept"),
                &[1.0, 2.0, 3.0],
                space(3),
            )],
        )
        .expect("first replacement");

    for invalid in [Vec::new(), vec![f64::NAN], vec![1.0, f64::INFINITY, 2.0]] {
        let mut broken = embedded(
            unit("u9", ALPHA, APP, SemanticUnitStatus::Indexed, "broken"),
            &[1.0],
            space(1),
        );
        broken.embedding = Some(invalid);
        let error = index
            .replace_file_units(
                ALPHA,
                APP,
                &[
                    plain(unit("u2", ALPHA, APP, SemanticUnitStatus::Pending, "later")),
                    broken,
                ],
            )
            .expect_err("invalid embedding");
        assert_eq!(error.to_string(), "Vetor de embedding inválido.");
    }

    let ids: Vec<String> = file_units(&index, ALPHA, APP)
        .into_iter()
        .map(|unit| unit.id)
        .collect();
    assert_eq!(ids, ["u1"]);
}

#[test]
fn stores_embeddings_as_single_precision_values() {
    let db = open("semantic-embeddings");
    let index = SemanticIndex::new(db.connection());
    index
        .replace_file_units(
            ALPHA,
            APP,
            &[embedded(
                unit("u1", ALPHA, APP, SemanticUnitStatus::Indexed, "vector"),
                &[0.1, -0.25, 1.5],
                space(3),
            )],
        )
        .expect("insert");

    let stored = embeddings(&index, ALPHA, &space(3), &filters());
    assert_eq!(stored.len(), 1);
    assert_eq!(stored[0].embedding, vec![f64::from(0.1f32), -0.25, 1.5]);
    assert_eq!(stored[0].unit.embedding_space, Some(space(3)));
}

#[test]
fn reads_an_embedding_only_in_its_own_space_and_when_indexed() {
    let db = open("semantic-spaces");
    let index = SemanticIndex::new(db.connection());
    let mut versioned = space(2);
    versioned.model_version = Some("2".to_string());
    index
        .replace_file_units(
            ALPHA,
            APP,
            &[
                embedded(
                    unit("u1", ALPHA, APP, SemanticUnitStatus::Indexed, "one"),
                    &[1.0, 2.0],
                    space(2),
                ),
                embedded(
                    unit("u2", ALPHA, APP, SemanticUnitStatus::Indexed, "two"),
                    &[3.0, 4.0],
                    versioned.clone(),
                ),
            ],
        )
        .expect("insert");
    index
        .replace_file_units(
            ALPHA,
            UTIL,
            &[embedded(
                unit("u3", ALPHA, UTIL, SemanticUnitStatus::Stale, "three"),
                &[5.0, 6.0],
                space(2),
            )],
        )
        .expect("stale unit");

    let unversioned: Vec<String> = embeddings(&index, ALPHA, &space(2), &filters())
        .into_iter()
        .map(|stored| stored.unit.id)
        .collect();
    assert_eq!(unversioned, ["u1"]);

    let versioned_ids: Vec<String> = embeddings(&index, ALPHA, &versioned, &filters())
        .into_iter()
        .map(|stored| stored.unit.id)
        .collect();
    assert_eq!(versioned_ids, ["u2"]);

    let any_dimensions = embeddings(
        &index,
        ALPHA,
        &SemanticEmbeddingSpace {
            dimensions: 0,
            ..space(2)
        },
        &filters(),
    );
    assert_eq!(any_dimensions.len(), 1);
}

#[test]
fn a_stored_vector_that_disagrees_with_its_dimensions_fails_the_read() {
    let db = open("semantic-dimensions");
    let index = SemanticIndex::new(db.connection());
    index
        .replace_file_units(
            ALPHA,
            APP,
            &[embedded(
                unit("u1", ALPHA, APP, SemanticUnitStatus::Indexed, "three"),
                &[1.0, 2.0, 3.0],
                space(2),
            )],
        )
        .expect("insert");
    let error = index
        .list_indexed_embeddings(ALPHA, &space(2), &filters(), None, 10)
        .expect_err("dimensions disagree");
    assert_eq!(
        error.to_string(),
        "Vetor persistido com dimensões inválidas."
    );
}

#[test]
fn lexical_search_requires_every_token_and_skips_units_not_searchable() {
    let db = open("semantic-lexical");
    let index = SemanticIndex::new(db.connection());
    index
        .replace_file_units(
            ALPHA,
            APP,
            &[
                plain(unit(
                    "u1",
                    ALPHA,
                    APP,
                    SemanticUnitStatus::Indexed,
                    "exported helper function",
                )),
                plain(unit(
                    "u2",
                    ALPHA,
                    APP,
                    SemanticUnitStatus::LexicalOnly,
                    "helper",
                )),
                plain(unit(
                    "u3",
                    ALPHA,
                    APP,
                    SemanticUnitStatus::Failed,
                    "helper failed",
                )),
                plain(unit(
                    "u4",
                    ALPHA,
                    APP,
                    SemanticUnitStatus::Stale,
                    "helper stale",
                )),
            ],
        )
        .expect("insert");

    let both: Vec<String> = index
        .search_lexical(ALPHA, "helper  function", &filters(), None)
        .expect("search")
        .into_iter()
        .map(|found| found.unit.id)
        .collect();
    assert_eq!(both, ["u1"]);

    let helper: Vec<String> = index
        .search_lexical(ALPHA, "\thelper\n", &filters(), None)
        .expect("search")
        .into_iter()
        .map(|found| found.unit.id)
        .collect();
    assert_eq!(helper.len(), 2);
    assert!(helper.contains(&"u1".to_string()) && helper.contains(&"u2".to_string()));

    assert!(
        index
            .search_lexical(ALPHA, "   ", &filters(), None)
            .expect("blank")
            .is_empty()
    );
    assert!(
        index
            .search_lexical(ALPHA, "say \"hi\"", &filters(), None)
            .is_ok()
    );
}

#[test]
fn lexical_search_limits_its_results() {
    let db = open("semantic-limits");
    let index = SemanticIndex::new(db.connection());
    let many: Vec<PersistedSemanticUnit> = (0..120)
        .map(|number| {
            plain(unit(
                &format!("u{number:03}"),
                ALPHA,
                APP,
                SemanticUnitStatus::Indexed,
                "shared token",
            ))
        })
        .collect();
    index.replace_file_units(ALPHA, APP, &many).expect("insert");

    assert_eq!(
        index
            .search_lexical(ALPHA, "shared", &filters(), None)
            .expect("default")
            .len(),
        50
    );
    assert_eq!(
        index
            .search_lexical(ALPHA, "shared", &filters(), Some(0))
            .expect("minimum")
            .len(),
        1
    );
    assert_eq!(
        index
            .search_lexical(ALPHA, "shared", &filters(), Some(1_000))
            .expect("maximum")
            .len(),
        100
    );
}

#[test]
fn unit_listings_apply_filters_order_and_limits() {
    let db = open("semantic-listing");
    let index = SemanticIndex::new(db.connection());
    let mut section = unit("doc", ALPHA, APP, SemanticUnitStatus::Indexed, "section");
    section.kind = SemanticUnitKind::MarkdownSection;
    index
        .replace_file_units(
            ALPHA,
            APP,
            &[
                plain(unit("b", ALPHA, APP, SemanticUnitStatus::Indexed, "b")),
                plain(section),
            ],
        )
        .expect("app units");
    index
        .replace_file_units(
            ALPHA,
            UTIL,
            &[plain(unit(
                "a",
                ALPHA,
                UTIL,
                SemanticUnitStatus::Indexed,
                "a",
            ))],
        )
        .expect("util units");

    let all: Vec<String> = index
        .list_units(ALPHA, &filters(), None)
        .expect("all")
        .into_iter()
        .map(|unit| unit.id)
        .collect();
    assert_eq!(all, ["b", "doc", "a"]);

    let by_path = SemanticSearchFilters {
        paths: vec![UTIL.to_string()],
        ..filters()
    };
    assert_eq!(
        index.list_units(ALPHA, &by_path, None).expect("path").len(),
        1
    );

    let by_kind = SemanticSearchFilters {
        kinds: vec![SemanticUnitKind::MarkdownSection],
        ..filters()
    };
    let sections: Vec<String> = index
        .list_units(ALPHA, &by_kind, None)
        .expect("kind")
        .into_iter()
        .map(|unit| unit.id)
        .collect();
    assert_eq!(sections, ["doc"]);

    let by_symbol = SemanticSearchFilters {
        symbols: vec!["a".to_string(), "missing".to_string()],
        ..filters()
    };
    assert_eq!(
        index
            .list_units(ALPHA, &by_symbol, None)
            .expect("symbol")
            .len(),
        1
    );
    assert_eq!(
        index
            .list_units(ALPHA, &filters(), Some(0))
            .expect("minimum")
            .len(),
        1
    );
}

#[test]
fn marks_a_file_stale_and_deletes_it() {
    let db = open("semantic-stale");
    let index = SemanticIndex::new(db.connection());
    index
        .replace_file_units(
            ALPHA,
            APP,
            &[plain(unit(
                "u1",
                ALPHA,
                APP,
                SemanticUnitStatus::Indexed,
                "one",
            ))],
        )
        .expect("insert");

    index
        .mark_file_stale(ALPHA, APP, None)
        .expect("default reason");
    let stale = index.get_unit(ALPHA, "u1").expect("get").expect("unit");
    assert_eq!(stale.status, SemanticUnitStatus::Stale);
    assert_eq!(
        stale.error.as_deref(),
        Some("O arquivo mudou após a análise.")
    );
    assert_ne!(stale.updated_at, AT);

    index
        .mark_file_stale(ALPHA, APP, Some("Outro motivo."))
        .expect("custom reason");
    assert_eq!(
        index
            .get_unit(ALPHA, "u1")
            .expect("get")
            .expect("unit")
            .error
            .as_deref(),
        Some("Outro motivo.")
    );

    index.delete_file(ALPHA, APP).expect("delete");
    assert!(file_units(&index, ALPHA, APP).is_empty());
}

#[test]
fn summary_counts_each_status_and_the_latest_run() {
    let db = open("semantic-summary");
    let index = SemanticIndex::new(db.connection());
    let empty = index.summary(ALPHA).expect("empty summary");
    assert_eq!(empty.units, 0);
    assert_eq!(empty.indexed_units, 0);
    assert_eq!(empty.latest_run, None);

    index
        .replace_file_units(
            ALPHA,
            APP,
            &[
                embedded(
                    unit("u1", ALPHA, APP, SemanticUnitStatus::Indexed, "one"),
                    &[1.0],
                    space(1),
                ),
                plain(unit(
                    "u2",
                    ALPHA,
                    APP,
                    SemanticUnitStatus::LexicalOnly,
                    "two",
                )),
                plain(unit(
                    "u3",
                    ALPHA,
                    APP,
                    SemanticUnitStatus::Incompatible,
                    "three",
                )),
                plain(unit("u4", ALPHA, APP, SemanticUnitStatus::Excluded, "four")),
            ],
        )
        .expect("app units");
    index
        .replace_file_units(
            ALPHA,
            "src/broken.ts",
            &[plain(unit(
                "u5",
                ALPHA,
                "src/broken.ts",
                SemanticUnitStatus::Failed,
                "five",
            ))],
        )
        .expect("broken units");
    index
        .create_run(&run("sem-2", SemanticRunStatus::Queued, AT))
        .expect("run");

    let summary = index.summary(ALPHA).expect("summary");
    assert_eq!(summary.files, 2);
    assert_eq!(summary.units, 5);
    assert_eq!(summary.indexed_units, 1);
    assert_eq!(summary.lexical_only_units, 1);
    assert_eq!(summary.failed_units, 2);
    assert_eq!(summary.excluded_units, 1);
    assert_eq!(summary.stale_units, 0);
    assert_eq!(
        summary.latest_run.map(|run| run.id),
        Some("sem-2".to_string())
    );
    assert_eq!(index.summary(BETA).expect("beta").units, 0);
}

#[test]
fn keeps_workspaces_apart() {
    let db = open("semantic-isolation");
    let index = SemanticIndex::new(db.connection());
    index
        .replace_file_units(
            BETA,
            APP,
            &[plain(unit(
                "beta-only",
                BETA,
                APP,
                SemanticUnitStatus::Indexed,
                "beta",
            ))],
        )
        .expect("beta units");
    assert_eq!(index.get_unit(ALPHA, "beta-only").expect("get"), None);
    assert!(
        index
            .list_units(ALPHA, &filters(), None)
            .expect("alpha")
            .is_empty()
    );
    assert_eq!(
        index
            .list_units(BETA, &filters(), None)
            .expect("beta")
            .len(),
        1
    );
}
