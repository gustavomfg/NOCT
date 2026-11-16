//! Engineering intelligence, checked against `EngineeringIntelligenceRepository.ts`. The row gate
//! compares the reads and the evaluation write with the TypeScript repository; these tests cover
//! the write rules, the bounds of the listings, the evidence of a snapshot and the rollback of a
//! refused evaluation.

use noct_core::engineering_intelligence::EngineeringHealthCategory::{
    Architecture, Documentation, Testing,
};
use noct_core::engineering_intelligence::{
    ENGINEERING_HEALTH_CATEGORIES, EngineeringEvidence, EngineeringEvidenceSource,
    EngineeringHealthCategory, EngineeringHealthSnapshot, EngineeringInsight, EngineeringMetric,
    EngineeringMetricValue, EngineeringSignal, EngineeringSignalSeverity, EngineeringSignalSource,
    EngineeringSignalStatus, EngineeringSourceRuns, HealthAssessmentStatus, HealthCategoryResult,
    HealthEvidenceCoverage,
};
use noct_core::workspace_evidence::EvidenceKind;
use noct_db::Database;
use noct_db::repositories::engineering_intelligence::{EngineeringIntelligence, EvaluationOptions};
use noct_db::repositories::workspace_evidence::{EvidenceInput, WorkspaceEvidence};

mod common;
use common::scratch;

/// The policy version of `shared/engineeringIntelligence.ts`.
const POLICY_VERSION: i64 = 2;
const AT: &str = "2026-09-08T12:00:00.000Z";
const LATER: &str = "2026-09-08T12:05:00.000Z";
const LATEST: &str = "2026-09-08T12:10:00.000Z";
const WORKSPACE: &str = "/work/engineering";
const OTHER: &str = "/work/other";

fn open(name: &str) -> Database {
    let db = Database::open_runtime(&scratch(name)).expect("open database");
    db.connection()
        .execute_batch(&format!(
            "INSERT INTO workspaces(path,name,created_at,last_opened_at) VALUES \
                 ('{WORKSPACE}','engineering','{AT}','{AT}'),('{OTHER}','other','{AT}','{AT}');"
        ))
        .expect("workspaces");
    db
}

fn digest(character: char) -> String {
    character.to_string().repeat(64)
}

fn evidence() -> EngineeringEvidence {
    EngineeringEvidence {
        id: "evidence-1".to_string(),
        source: EngineeringEvidenceSource::ProjectIndexFile,
        source_id: "project-file-1".to_string(),
        relative_path: Some("src/app.ts".to_string()),
        source_hash: Some(digest('a')),
        run_id: None,
        location: None,
        validity: None,
        detail: "Arquivo indexado pelo Project Index.".to_string(),
        observed_at: AT.to_string(),
    }
}

/// A signal identified by one character, so that its id and fingerprint are both readable.
fn signal(character: char, category: EngineeringHealthCategory) -> EngineeringSignal {
    EngineeringSignal {
        id: format!("signal-{character}"),
        workspace: WORKSPACE.to_string(),
        category,
        kind: "example-deterministic-signal".to_string(),
        title: "Sinal de teste".to_string(),
        description: "Sinal determinístico usado para validar a persistência.".to_string(),
        severity: EngineeringSignalSeverity::Medium,
        confidence: 90,
        evidence: vec![evidence()],
        metric: Some(EngineeringMetric {
            name: "affectedFiles".to_string(),
            value: EngineeringMetricValue::Number(1.0),
            unit: Some("files".to_string()),
        }),
        source: EngineeringSignalSource::Deterministic,
        policy_version: POLICY_VERSION,
        detected_at: AT.to_string(),
        fingerprint: digest(character),
        status: EngineeringSignalStatus::Active,
        first_seen_at: AT.to_string(),
        last_seen_at: AT.to_string(),
        resolved_at: None,
    }
}

fn categories(assessed: &[EngineeringHealthCategory], at: &str) -> Vec<HealthCategoryResult> {
    ENGINEERING_HEALTH_CATEGORIES
        .into_iter()
        .map(|category| {
            let is_assessed = assessed.contains(&category);
            HealthCategoryResult {
                category,
                status: if is_assessed {
                    HealthAssessmentStatus::Assessed
                } else {
                    HealthAssessmentStatus::NotAssessed
                },
                score: None,
                coverage: HealthEvidenceCoverage {
                    available: i64::from(is_assessed),
                    expected: None,
                    percent: None,
                },
                signal_ids: Vec::new(),
                evidence: Vec::new(),
                evaluated_at: at.to_string(),
            }
        })
        .collect()
}

/// A snapshot of one state. Its categories are assessed only when named in `assessed`.
fn snapshot(
    id: &str,
    evaluated_at: &str,
    state: char,
    assessed: &[EngineeringHealthCategory],
) -> EngineeringHealthSnapshot {
    EngineeringHealthSnapshot {
        id: id.to_string(),
        workspace: WORKSPACE.to_string(),
        policy_version: POLICY_VERSION,
        evaluated_at: evaluated_at.to_string(),
        previous_snapshot_id: None,
        sources: EngineeringSourceRuns {
            project_index_run_id: Some("project-run-1".to_string()),
            semantic_index_run_id: None,
            validation_run_ids: Vec::new(),
            execution_ids: Vec::new(),
            change_set_ids: Vec::new(),
        },
        categories: categories(assessed, evaluated_at),
        signal_fingerprints: Vec::new(),
        signal_states: Vec::new(),
        state_fingerprint: digest(state),
    }
}

fn evaluate(
    store: &EngineeringIntelligence<'_>,
    signals: &[EngineeringSignal],
    snapshot: &EngineeringHealthSnapshot,
) -> EngineeringHealthSnapshot {
    store
        .save_evaluation(signals, snapshot, &EvaluationOptions::default())
        .expect("evaluation")
}

fn insight(id: &str) -> EngineeringInsight {
    EngineeringInsight {
        id: id.to_string(),
        workspace: WORKSPACE.to_string(),
        fingerprint: digest('c'),
        title: "Insight de teste".to_string(),
        explanation: "Explicação determinística.".to_string(),
        related_signal_ids: vec!["signal-a".to_string()],
        evidence: vec![evidence()],
        suggested_action: None,
        confidence: 80,
        status: EngineeringSignalStatus::Active,
        created_at: AT.to_string(),
        updated_at: AT.to_string(),
        last_seen_at: AT.to_string(),
    }
}

#[test]
fn persists_each_fingerprint_once_and_keeps_the_first_sighting() {
    let db = open("engineering-first-sighting");
    let store = EngineeringIntelligence::new(db.connection());
    evaluate(
        &store,
        &[signal('a', Architecture)],
        &snapshot("snapshot-1", AT, 'a', &[Architecture]),
    );

    let changed = EngineeringSignal {
        severity: EngineeringSignalSeverity::High,
        detected_at: LATER.to_string(),
        last_seen_at: LATER.to_string(),
        ..signal('a', Architecture)
    };
    let latest = evaluate(
        &store,
        &[changed],
        &snapshot("snapshot-2", LATER, 'b', &[Architecture]),
    );

    let signals = store.list_signals(WORKSPACE, None, None).expect("signals");
    assert_eq!(signals.len(), 1);
    assert_eq!(signals[0].severity, EngineeringSignalSeverity::High);
    assert_eq!(signals[0].first_seen_at, AT);
    assert_eq!(signals[0].last_seen_at, LATER);
    assert_eq!(
        store
            .list_snapshots(WORKSPACE, None)
            .expect("snapshots")
            .len(),
        2
    );
    assert_eq!(
        store.latest_snapshot(WORKSPACE).expect("latest"),
        Some(latest)
    );
}

#[test]
fn resolves_only_signals_of_assessed_categories() {
    let db = open("engineering-assessed-only");
    let store = EngineeringIntelligence::new(db.connection());
    evaluate(
        &store,
        &[signal('a', Architecture), signal('b', Testing)],
        &snapshot("snapshot-1", AT, 'a', &[Architecture, Testing]),
    );

    evaluate(
        &store,
        &[],
        &snapshot("snapshot-2", LATEST, 'c', &[Architecture]),
    );

    let resolved = store
        .list_signals(WORKSPACE, Some(EngineeringSignalStatus::Resolved), None)
        .expect("resolved");
    assert_eq!(resolved.len(), 1);
    assert_eq!(resolved[0].fingerprint, digest('a'));
    assert_eq!(resolved[0].resolved_at.as_deref(), Some(LATEST));

    let active = store
        .list_signals(WORKSPACE, Some(EngineeringSignalStatus::Active), None)
        .expect("active");
    assert_eq!(active.len(), 1);
    assert_eq!(active[0].fingerprint, digest('b'));
}

#[test]
fn skipped_categories_keep_their_signals_active() {
    let db = open("engineering-skip-category");
    let store = EngineeringIntelligence::new(db.connection());
    evaluate(
        &store,
        &[signal('a', Architecture)],
        &snapshot("snapshot-1", AT, 'a', &[Architecture]),
    );

    store
        .save_evaluation(
            &[],
            &snapshot("snapshot-2", LATEST, 'c', &[Architecture]),
            &EvaluationOptions {
                skip_category_resolution: vec![Architecture],
                resolve_signal_fingerprints: Vec::new(),
            },
        )
        .expect("evaluation");

    let active = store
        .list_signals(WORKSPACE, Some(EngineeringSignalStatus::Active), None)
        .expect("active");
    assert_eq!(active.len(), 1);
    assert_eq!(active[0].fingerprint, digest('a'));
}

#[test]
fn explicit_fingerprints_resolve_absent_signals_only() {
    let db = open("engineering-explicit-fingerprints");
    let store = EngineeringIntelligence::new(db.connection());
    evaluate(
        &store,
        &[signal('a', Documentation), signal('b', Documentation)],
        &snapshot("snapshot-1", AT, 'a', &[]),
    );

    store
        .save_evaluation(
            &[signal('b', Documentation)],
            &snapshot("snapshot-2", LATEST, 'c', &[]),
            &EvaluationOptions {
                skip_category_resolution: Vec::new(),
                resolve_signal_fingerprints: vec![digest('a'), digest('b')],
            },
        )
        .expect("evaluation");

    let resolved = store
        .list_signals(WORKSPACE, Some(EngineeringSignalStatus::Resolved), None)
        .expect("resolved");
    assert_eq!(resolved.len(), 1);
    assert_eq!(resolved[0].fingerprint, digest('a'));
}

#[test]
fn a_repeated_state_returns_the_stored_snapshot_without_new_writes() {
    let db = open("engineering-repeated-state");
    let store = EngineeringIntelligence::new(db.connection());
    let first = evaluate(
        &store,
        &[],
        &snapshot("snapshot-1", AT, 'a', &[Architecture]),
    );
    let again = evaluate(
        &store,
        &[],
        &snapshot("snapshot-2", LATER, 'a', &[Architecture]),
    );

    assert_eq!(first.id, "snapshot-1");
    assert_eq!(again, first);
    assert_eq!(
        store
            .list_snapshots(WORKSPACE, None)
            .expect("snapshots")
            .len(),
        1
    );
    let evidence = WorkspaceEvidence::new(db.connection());
    assert!(
        evidence
            .by_source(EvidenceKind::Engineering, "snapshot-2")
            .expect("evidence")
            .is_none()
    );
}

#[test]
fn a_new_snapshot_records_its_sources_as_evidence_once() {
    let db = open("engineering-evidence");
    let store = EngineeringIntelligence::new(db.connection());
    evaluate(
        &store,
        &[],
        &snapshot("snapshot-1", AT, 'a', &[Architecture]),
    );

    let manifest = WorkspaceEvidence::new(db.connection())
        .by_source(EvidenceKind::Engineering, "snapshot-1")
        .expect("evidence")
        .expect("recorded");
    assert_eq!(manifest.workspace, WORKSPACE);
    assert_eq!(manifest.references.len(), 1);
    assert_eq!(manifest.references[0].kind, "project-index");
    assert_eq!(manifest.references[0].id, "project-run-1");
}

#[test]
fn a_refused_evidence_identity_rolls_back_the_whole_evaluation() {
    let db = open("engineering-rollback");
    let store = EngineeringIntelligence::new(db.connection());
    WorkspaceEvidence::new(db.connection())
        .record(EvidenceInput {
            kind: EvidenceKind::Engineering,
            workspace: OTHER.to_string(),
            source_id: "snapshot-1".to_string(),
            execution_id: None,
            started_at: None,
            paths: Vec::new(),
            references: Vec::new(),
        })
        .expect("conflicting evidence");

    let result = store.save_evaluation(
        &[signal('a', Architecture)],
        &snapshot("snapshot-1", AT, 'a', &[Architecture]),
        &EvaluationOptions::default(),
    );

    assert!(result.is_err());
    assert!(
        store
            .list_signals(WORKSPACE, None, None)
            .expect("signals")
            .is_empty()
    );
    assert!(
        store
            .list_snapshots(WORKSPACE, None)
            .expect("snapshots")
            .is_empty()
    );
}

#[test]
fn invalid_values_are_refused_before_anything_is_written() {
    let db = open("engineering-validation");
    let store = EngineeringIntelligence::new(db.connection());

    let invalid = EngineeringSignal {
        confidence: 101,
        ..signal('b', Architecture)
    };
    let result = store.save_evaluation(
        &[signal('a', Architecture), invalid],
        &snapshot("snapshot-1", AT, 'a', &[Architecture]),
        &EvaluationOptions::default(),
    );
    assert!(result.is_err());
    assert!(
        store
            .list_signals(WORKSPACE, None, None)
            .expect("signals")
            .is_empty()
    );

    let mut short = snapshot("snapshot-1", AT, 'a', &[Architecture]);
    short.categories.pop();
    assert!(
        store
            .save_evaluation(&[], &short, &EvaluationOptions::default())
            .is_err()
    );
    assert!(
        store
            .list_snapshots(WORKSPACE, None)
            .expect("snapshots")
            .is_empty()
    );
}

#[test]
fn stored_values_round_trip_unchanged() {
    let db = open("engineering-round-trip");
    let store = EngineeringIntelligence::new(db.connection());
    let original = signal('a', Architecture);
    evaluate(
        &store,
        std::slice::from_ref(&original),
        &snapshot("snapshot-1", AT, 'a', &[Architecture]),
    );

    assert_eq!(
        store.get_signal(WORKSPACE, &digest('a')).expect("signal"),
        Some(original)
    );
    assert_eq!(
        store.latest_snapshot(WORKSPACE).expect("snapshot"),
        Some(snapshot("snapshot-1", AT, 'a', &[Architecture]))
    );
}

#[test]
fn listings_are_bounded_between_one_and_the_maximum() {
    let db = open("engineering-limits");
    let store = EngineeringIntelligence::new(db.connection());
    evaluate(
        &store,
        &[
            signal('a', Architecture),
            signal('b', Architecture),
            signal('c', Architecture),
        ],
        &snapshot("snapshot-1", AT, 'a', &[Architecture]),
    );

    assert_eq!(
        store
            .list_signals(WORKSPACE, None, None)
            .expect("all")
            .len(),
        3
    );
    assert_eq!(
        store
            .list_signals(WORKSPACE, None, Some(0))
            .expect("min")
            .len(),
        1
    );
    assert_eq!(
        store
            .list_signals(WORKSPACE, None, Some(2))
            .expect("two")
            .len(),
        2
    );
    assert_eq!(
        store
            .list_signals(WORKSPACE, None, Some(10_000))
            .expect("max")
            .len(),
        3
    );
}

/// The schema checks the enum columns, so the shape of the JSON columns is the one a corrupt row
/// can break. A valid JSON text of the wrong shape must fail the read, not produce a signal.
#[test]
fn a_corrupt_stored_evidence_fails_the_read() {
    let db = open("engineering-corrupt-evidence");
    let store = EngineeringIntelligence::new(db.connection());
    evaluate(
        &store,
        &[signal('a', Architecture)],
        &snapshot("snapshot-1", AT, 'a', &[Architecture]),
    );
    db.connection()
        .execute(
            "UPDATE engineering_signals SET evidence_json='{\"not\":\"a list\"}' WHERE fingerprint=?1",
            [digest('a')],
        )
        .expect("corrupt row");

    assert!(store.get_signal(WORKSPACE, &digest('a')).is_err());
    assert!(store.list_signals(WORKSPACE, None, None).is_err());
}

#[test]
fn workspaces_are_isolated() {
    let db = open("engineering-isolation");
    let store = EngineeringIntelligence::new(db.connection());
    evaluate(
        &store,
        &[signal('a', Architecture)],
        &snapshot("snapshot-1", AT, 'a', &[Architecture]),
    );

    assert!(
        store
            .list_signals(OTHER, None, None)
            .expect("other")
            .is_empty()
    );
    assert!(
        store
            .get_signal(OTHER, &digest('a'))
            .expect("lookup")
            .is_none()
    );
    assert!(store.latest_snapshot(OTHER).expect("snapshot").is_none());
}

#[test]
fn an_insight_keeps_the_identity_and_creation_time_of_its_fingerprint() {
    let db = open("engineering-insight");
    let store = EngineeringIntelligence::new(db.connection());
    store.save_insight(&insight("insight-1")).expect("first");

    let revised = EngineeringInsight {
        title: "Insight revisado".to_string(),
        created_at: LATER.to_string(),
        updated_at: LATER.to_string(),
        last_seen_at: LATER.to_string(),
        ..insight("insight-2")
    };
    let stored = store.save_insight(&revised).expect("revised");

    assert_eq!(stored.id, "insight-1");
    assert_eq!(stored.created_at, AT);
    assert_eq!(stored.title, "Insight revisado");
    let listed = store
        .list_insights(WORKSPACE, None, None)
        .expect("insights");
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].updated_at, LATER);
}

#[test]
fn only_active_insights_are_resolved() {
    let db = open("engineering-resolve-insight");
    let store = EngineeringIntelligence::new(db.connection());
    store.save_insight(&insight("insight-1")).expect("insight");

    assert!(
        store
            .resolve_insight(WORKSPACE, "insight-1", LATEST)
            .expect("resolve")
    );
    assert!(
        !store
            .resolve_insight(WORKSPACE, "insight-1", LATER)
            .expect("resolve again")
    );
    assert!(
        store
            .list_insights(WORKSPACE, Some(EngineeringSignalStatus::Active), None)
            .expect("active")
            .is_empty()
    );
    let resolved = store
        .list_insights(WORKSPACE, Some(EngineeringSignalStatus::Resolved), None)
        .expect("resolved");
    assert_eq!(resolved.len(), 1);
    assert_eq!(resolved[0].updated_at, LATEST);
}
