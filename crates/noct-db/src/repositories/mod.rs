//! Repositories over the NOCT schema. Port of `electron/database/*Repository.ts`.
//!
//! Each repository borrows the connection owned by [`crate::Database`]. Writes that
//! touch more than one row run through [`crate::transaction::run`].

pub mod approvals;
pub mod artifacts;
pub mod change_sets;
pub mod checkpoints;
pub mod conversations;
pub mod execution_evidence;
pub mod executions;
pub mod model_catalog;
pub mod operation_recovery;
pub mod provider_configurations;
pub mod settings;
pub mod validation_runs;
pub mod workspace_evidence;
pub mod workspace_memory;
pub mod workspace_model_bindings;
pub mod workspaces;

/// One page of a list, plus whether a further page exists.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Page<T> {
    pub items: Vec<T>,
    pub has_more: bool,
}
