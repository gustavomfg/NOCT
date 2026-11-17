//! The lifecycle of a suggestion, checked against the table of `SuggestionRepository.setStatus` in
//! `electron/database/SuggestionRepository.ts`, one row per current status.

use SuggestionStatus::{Accepted, Deferred, InAnalysis, Invalid, New, Rejected, Resolved};
use noct_core::suggestions::{SuggestionStatus, allowed_suggestion_transition};

const ALL: [SuggestionStatus; 7] = [
    New, InAnalysis, Accepted, Rejected, Resolved, Deferred, Invalid,
];

fn expected(from: SuggestionStatus) -> &'static [SuggestionStatus] {
    match from {
        New => &[InAnalysis, Accepted, Rejected, Resolved, Deferred, Invalid],
        InAnalysis => &[Accepted, Rejected, Resolved, Deferred, Invalid],
        Accepted => &[Resolved, Rejected, Deferred, Invalid],
        Deferred => &[InAnalysis, Accepted, Rejected, Resolved, Invalid],
        Rejected | Resolved | Invalid => &[],
    }
}

#[test]
fn every_pair_of_statuses_follows_the_typescript_table() {
    for from in ALL {
        for to in ALL {
            assert_eq!(
                allowed_suggestion_transition(from, to),
                expected(from).contains(&to),
                "{from:?} -> {to:?}"
            );
        }
    }
}

#[test]
fn a_rejected_resolved_or_invalid_suggestion_is_final() {
    for from in [Rejected, Resolved, Invalid] {
        assert!(expected(from).is_empty(), "{from:?} has no way out");
    }
}
