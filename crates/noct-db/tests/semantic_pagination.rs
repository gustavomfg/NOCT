//! Keyset pagination of the semantic index listings. A file's units and the indexed embeddings are
//! read a page at a time: the page limit is mandatory and clamped to the canonical ceiling, the
//! cursor resumes strictly after the last row, and the internal streams yield every row exactly
//! once, in the listing's order, even across page boundaries.

use noct_core::code_intelligence::{
    DiscoveryFileClassification, ProjectIndexFile, ProjectIndexFileState,
};
use noct_core::limits::semantic_index::PAGE_LIMIT_MAX;
use noct_core::semantic_index::{
    SemanticEmbeddingSpace, SemanticSearchFilters, SemanticUnit, SemanticUnitKind,
    SemanticUnitLocation, SemanticUnitStatus,
};
use noct_db::Database;
use noct_db::repositories::project_index::ProjectIndex;
use noct_db::repositories::semantic_index::{PersistedSemanticUnit, SemanticIndex, UnitPosition};

mod common;
use common::scratch;

const AT: &str = "2026-01-01T00:00:00.000Z";
const ALPHA: &str = "/work/alpha";
const APP: &str = "src/app.ts";
const MAX: usize = PAGE_LIMIT_MAX as usize;

fn open(name: &str) -> Database {
    let db = Database::open(&scratch(name)).expect("open database");
    db.connection()
        .execute_batch(&format!(
            "INSERT INTO workspaces(path,name,created_at,last_opened_at) VALUES ('{ALPHA}','alpha','{AT}','{AT}');"
        ))
        .expect("workspace");
    ProjectIndex::new(db.connection())
        .upsert_file(&project_file(APP))
        .expect("project file");
    db
}

fn project_file(relative_path: &str) -> ProjectIndexFile {
    ProjectIndexFile {
        workspace: ALPHA.to_string(),
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

fn space() -> SemanticEmbeddingSpace {
    SemanticEmbeddingSpace {
        provider_id: "local".to_string(),
        model_id: "mini".to_string(),
        model_version: None,
        dimensions: 2,
    }
}

/// A unit whose identifier and position are chosen separately, so that identifier order and
/// source order can differ.
fn unit(id: &str, start_line: i64, status: SemanticUnitStatus) -> SemanticUnit {
    SemanticUnit {
        id: id.to_string(),
        workspace: ALPHA.to_string(),
        relative_path: APP.to_string(),
        kind: SemanticUnitKind::Symbol,
        language: Some("TypeScript".to_string()),
        symbol_id: None,
        symbol_name: Some(id.to_string()),
        location: SemanticUnitLocation {
            start_line,
            start_column: 1,
            end_line: start_line,
            end_column: 2,
            start_offset: None,
            end_offset: None,
        },
        source_hash: digest('a'),
        chunk_hash: digest('b'),
        normalized_text: format!("texto {id}"),
        chunk_strategy_version: "symbols-v2".to_string(),
        embedding_space: None,
        status,
        error: None,
        created_at: AT.to_string(),
        updated_at: AT.to_string(),
        indexed_at: None,
    }
}

fn position_of(unit: &SemanticUnit) -> UnitPosition {
    UnitPosition {
        start_line: unit.location.start_line,
        start_column: unit.location.start_column,
        id: unit.id.clone(),
    }
}

/// Walks a file's units page by page, and returns the identifiers of every page. A cursor exists
/// exactly when more rows follow, which the walk checks on every page.
fn walk_file(index: &SemanticIndex, limit: usize) -> Vec<Vec<String>> {
    let mut pages = Vec::new();
    let mut after: Option<UnitPosition> = None;
    loop {
        let page = index
            .list_file_units(ALPHA, APP, after.as_ref(), limit)
            .expect("page");
        assert_eq!(
            page.has_more,
            page.next.is_some(),
            "has_more and next agree"
        );
        pages.push(page.items.iter().map(|unit| unit.id.clone()).collect());
        match page.next {
            Some(next) => after = Some(next),
            None => return pages,
        }
    }
}

#[test]
fn file_units_stream_in_source_order_across_page_boundaries() {
    let db = open("pages-stream");
    let index = SemanticIndex::new(db.connection());
    // Identifiers ascend while source lines descend, so the listing order is not the id order.
    let total = MAX * 2 + 203;
    let units: Vec<PersistedSemanticUnit> = (0..total)
        .map(|n| PersistedSemanticUnit {
            unit: unit(
                &format!("u{n:05}"),
                (total - n) as i64,
                SemanticUnitStatus::Indexed,
            ),
            embedding: None,
        })
        .collect();
    index
        .replace_file_units(ALPHA, APP, &units)
        .expect("insert");

    let streamed: Vec<String> = index
        .file_units(ALPHA, APP)
        .map(|unit| unit.expect("unit").id)
        .collect();

    assert_eq!(streamed.len(), total);
    let expected: Vec<String> = (0..total).rev().map(|n| format!("u{n:05}")).collect();
    assert_eq!(streamed, expected, "source order, not identifier order");
}

#[test]
fn the_page_limit_is_clamped_to_the_ceiling_and_the_cursor_is_exclusive() {
    let db = open("pages-clamp");
    let index = SemanticIndex::new(db.connection());
    let units: Vec<PersistedSemanticUnit> = (1..=600)
        .map(|line| PersistedSemanticUnit {
            unit: unit(&format!("u{line:04}"), line, SemanticUnitStatus::Indexed),
            embedding: None,
        })
        .collect();
    index
        .replace_file_units(ALPHA, APP, &units)
        .expect("insert");

    let first = index
        .list_file_units(ALPHA, APP, None, 10_000)
        .expect("first page");
    assert_eq!(
        first.items.len(),
        MAX,
        "a larger request is clamped to the ceiling"
    );
    assert!(first.has_more, "rows follow the first page");
    let cursor = first
        .next
        .clone()
        .expect("a page with rows after it has a cursor");
    assert_eq!(cursor, position_of(first.items.last().expect("last")));

    let zero = index.list_file_units(ALPHA, APP, None, 0).expect("zero");
    assert_eq!(zero.items.len(), 1, "a zero limit is clamped to one");

    let second = index
        .list_file_units(ALPHA, APP, Some(&cursor), 10_000)
        .expect("second page");
    assert_eq!(
        second.items.len(),
        100,
        "the cursor resumes after its own row"
    );
    assert_eq!(second.items[0].id, "u0501");
    assert!(!second.has_more, "a short page reports no more rows");
    assert!(second.next.is_none(), "a short page ends the listing");
}

#[test]
fn walking_by_pages_yields_every_unit_once_and_matches_the_stream() {
    let db = open("pages-walk");
    let index = SemanticIndex::new(db.connection());
    let units: Vec<PersistedSemanticUnit> = (1..=40)
        .map(|line| PersistedSemanticUnit {
            unit: unit(&format!("w{line:02}"), line, SemanticUnitStatus::Indexed),
            embedding: None,
        })
        .collect();
    index
        .replace_file_units(ALPHA, APP, &units)
        .expect("insert");

    let walked: Vec<String> = walk_file(&index, 7).into_iter().flatten().collect();
    let streamed: Vec<String> = index
        .file_units(ALPHA, APP)
        .map(|unit| unit.expect("unit").id)
        .collect();

    assert_eq!(walked.len(), 40);
    assert_eq!(walked, streamed);
    let mut unique = walked.clone();
    unique.sort();
    unique.dedup();
    assert_eq!(unique.len(), 40, "no unit appears on two pages");
}

#[test]
fn a_last_full_page_ends_the_listing_without_an_empty_page() {
    let db = open("pages-exact");
    let index = SemanticIndex::new(db.connection());
    let units: Vec<PersistedSemanticUnit> = (1..=4)
        .map(|line| PersistedSemanticUnit {
            unit: unit(&format!("e{line}"), line, SemanticUnitStatus::Indexed),
            embedding: None,
        })
        .collect();
    index
        .replace_file_units(ALPHA, APP, &units)
        .expect("insert");

    assert_eq!(
        walk_file(&index, 2),
        vec![
            vec!["e1".to_string(), "e2".to_string()],
            vec!["e3".to_string(), "e4".to_string()],
        ],
        "the second page is full, yet it reports that no rows follow"
    );

    let whole = index.list_file_units(ALPHA, APP, None, 4).expect("whole");
    assert_eq!(whole.items.len(), 4);
    assert!(
        !whole.has_more,
        "a page that exactly fills the limit is not followed by more rows"
    );
    assert!(whole.next.is_none());

    let first = index.list_file_units(ALPHA, APP, None, 2).expect("first");
    assert!(first.has_more);
    let last = index
        .list_file_units(ALPHA, APP, first.next.as_ref(), 2)
        .expect("last");
    assert!(!last.has_more);
}

#[test]
fn embeddings_page_in_identifier_order_within_their_space() {
    let db = open("pages-embeddings");
    let index = SemanticIndex::new(db.connection());
    let stored: Vec<PersistedSemanticUnit> = ["c", "a", "e", "b", "d"]
        .iter()
        .enumerate()
        .map(|(line, id)| {
            let mut value = unit(id, line as i64 + 1, SemanticUnitStatus::Indexed);
            value.embedding_space = Some(space());
            PersistedSemanticUnit {
                unit: value,
                embedding: Some(vec![1.0, 2.0]),
            }
        })
        .collect();
    index
        .replace_file_units(ALPHA, APP, &stored)
        .expect("insert");
    let filters = SemanticSearchFilters::default();

    let mut after: Option<String> = None;
    let mut ids = Vec::new();
    loop {
        let page = index
            .list_indexed_embeddings(ALPHA, &space(), &filters, after.as_deref(), 2)
            .expect("page");
        ids.extend(page.items.iter().map(|item| item.unit.id.clone()));
        match page.next {
            Some(next) => after = Some(next),
            None => break,
        }
    }

    assert_eq!(ids, ["a", "b", "c", "d", "e"]);
    let streamed: Vec<String> = index
        .indexed_embeddings(ALPHA, &space(), &filters)
        .map(|item| item.expect("embedding").unit.id)
        .collect();
    assert_eq!(streamed, ids);

    let other_space = SemanticEmbeddingSpace {
        dimensions: 3,
        ..space()
    };
    let none = index
        .list_indexed_embeddings(ALPHA, &other_space, &filters, None, 10)
        .expect("other space");
    assert!(
        none.items.is_empty(),
        "a space of other dimensions reads nothing"
    );
}

#[test]
fn an_unreadable_embedding_fails_its_page_and_ends_the_stream() {
    let db = open("pages-corrupt");
    let index = SemanticIndex::new(db.connection());
    let mut stored: Vec<PersistedSemanticUnit> = ["a", "b", "c"]
        .iter()
        .enumerate()
        .map(|(line, id)| {
            let mut value = unit(id, line as i64 + 1, SemanticUnitStatus::Indexed);
            value.embedding_space = Some(space());
            PersistedSemanticUnit {
                unit: value,
                embedding: Some(vec![1.0, 2.0]),
            }
        })
        .collect();
    // A vector of three values stored under a space of two dimensions.
    stored[1].embedding = Some(vec![1.0, 2.0, 3.0]);
    index
        .replace_file_units(ALPHA, APP, &stored)
        .expect("insert");

    let outcomes: Vec<bool> = index
        .indexed_embeddings(ALPHA, &space(), &SemanticSearchFilters::default())
        .map(|item| item.is_ok())
        .collect();

    // The page is read in one query, so the bad row fails the whole page, and the stream then stops
    // with that single error rather than yielding the rows read before it.
    assert_eq!(
        outcomes,
        [false],
        "the page fails as a whole, and the stream ends"
    );
}
