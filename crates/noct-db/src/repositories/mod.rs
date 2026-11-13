//! Repositories over the NOCT schema. Port of `electron/database/*Repository.ts`.
//!
//! Each repository borrows the connection owned by [`crate::Database`]. Writes that
//! touch more than one row run through [`crate::transaction::run`].

pub mod approvals;
pub mod artifacts;
pub mod conversations;
pub mod model_catalog;
pub mod provider_configurations;
pub mod settings;
pub mod workspace_memory;
pub mod workspace_model_bindings;
pub mod workspaces;

/// One page of a list, plus whether a further page exists.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Page<T> {
    pub items: Vec<T>,
    pub has_more: bool,
}
