//! The persistence facade of the application: the runtime, every repository, and the workflows
//! that write to more than one of them. Port of `electron/database/Database.ts` (`LocalDatabase`).
//!
//! A repository is a view over the connection, so each accessor is cheap and holds no state.
//! Opening the store records the operations that the last run left running, as the TypeScript
//! facade does, and the two workflows that span several repositories run in one transaction or
//! in the order the TypeScript facade uses.

use std::path::Path;

use rusqlite::Connection;
use serde_json::Value;

use crate::repositories::approvals::Approvals;
use crate::repositories::artifacts::{Artifacts, NewArtifact};
use crate::repositories::backup::Backup;
use crate::repositories::brain_memories::BrainMemories;
use crate::repositories::change_sets::ChangeSets;
use crate::repositories::checkpoints::Checkpoints;
use crate::repositories::conversations::{
    ConversationRow, Conversations, MessageRole, MessageRow, insert_message,
};
use crate::repositories::engineering_intelligence::EngineeringIntelligence;
use crate::repositories::execution_evidence::ExecutionEvidence;
use crate::repositories::executions::Executions;
use crate::repositories::model_catalog::ModelCatalog;
use crate::repositories::operation_recovery::OperationRecovery;
use crate::repositories::project_index::ProjectIndex;
use crate::repositories::provider_configurations::ProviderConfigurations;
use crate::repositories::semantic_index::SemanticIndex;
use crate::repositories::settings::Settings;
use crate::repositories::suggestions::Suggestions;
use crate::repositories::validation_runs::ValidationRuns;
use crate::repositories::workspace_evidence::WorkspaceEvidence;
use crate::repositories::workspace_memory::WorkspaceMemory;
use crate::repositories::workspace_model_bindings::ModelBindings;
use crate::repositories::workspaces::Workspaces;
use crate::runtime::{Database, OpenOptions};
use crate::{Result, transaction};

/// The database of the application, with its repositories.
pub struct Store {
    database: Database,
}

/// An assistant reply and the artifacts it produced. `markdown_title` titles the markdown copy
/// of the reply. The caller formats it, because the title is shown in the user's language.
pub struct AssistantTurn<'a> {
    pub conversation_id: &'a str,
    pub workspace: &'a str,
    pub content: &'a str,
    pub metadata: Option<&'a Value>,
    pub markdown_title: &'a str,
    pub artifacts: &'a [TurnArtifact<'a>],
}

/// An artifact that accompanies an assistant reply.
pub struct TurnArtifact<'a> {
    pub kind: &'a str,
    pub title: &'a str,
    pub file_path: Option<&'a str>,
    pub content: Option<&'a str>,
    pub metadata: Option<&'a Value>,
}

impl Store {
    /// Opens the database with the given options and records the operations that the last run
    /// left running. The options carry the diagnostic sink and the operation observer of the
    /// application. Running it again on the same database changes nothing.
    pub fn open_with(data_directory: &Path, options: OpenOptions) -> Result<Self> {
        let database = Database::open_runtime_with(data_directory, options)?;
        OperationRecovery::new(database.connection()).reconcile()?;
        Ok(Self { database })
    }

    /// Opens the database with the default options. See [`Store::open_with`].
    pub fn open(data_directory: &Path) -> Result<Self> {
        Self::open_with(data_directory, OpenOptions::default())
    }

    pub fn data_directory(&self) -> &Path {
        self.database.data_directory()
    }

    pub fn connection(&self) -> &Connection {
        self.database.connection()
    }

    /// Checkpoints the write-ahead log and closes the database.
    pub fn close(self) -> Result<()> {
        self.database.close()
    }

    pub fn approvals(&self) -> Approvals<'_> {
        Approvals::new(self.connection())
    }

    pub fn artifacts(&self) -> Artifacts<'_> {
        Artifacts::new(self.connection())
    }

    pub fn backup(&self) -> Backup<'_> {
        Backup::new(self.connection())
    }

    pub fn brain_memories(&self) -> BrainMemories<'_> {
        BrainMemories::new(self.connection())
    }

    pub fn change_sets(&self) -> ChangeSets<'_> {
        ChangeSets::new(self.connection())
    }

    pub fn checkpoints(&self) -> Checkpoints<'_> {
        Checkpoints::new(self.connection())
    }

    pub fn conversations(&self) -> Conversations<'_> {
        Conversations::new(self.connection())
    }

    pub fn engineering_intelligence(&self) -> EngineeringIntelligence<'_> {
        EngineeringIntelligence::new(self.connection())
    }

    pub fn execution_evidence(&self) -> ExecutionEvidence<'_> {
        ExecutionEvidence::new(self.connection())
    }

    pub fn executions(&self) -> Executions<'_> {
        Executions::new(self.connection())
    }

    pub fn model_catalog(&self) -> ModelCatalog<'_> {
        ModelCatalog::new(self.connection())
    }

    pub fn operation_recovery(&self) -> OperationRecovery<'_> {
        OperationRecovery::new(self.connection())
    }

    pub fn project_index(&self) -> ProjectIndex<'_> {
        ProjectIndex::new(self.connection())
    }

    pub fn provider_configurations(&self) -> ProviderConfigurations<'_> {
        ProviderConfigurations::new(self.connection())
    }

    pub fn semantic_index(&self) -> SemanticIndex<'_> {
        SemanticIndex::with_diagnostics(self.connection(), self.database.diagnostics())
    }

    pub fn settings(&self) -> Settings<'_> {
        Settings::new(self.connection())
    }

    pub fn suggestions(&self) -> Suggestions<'_> {
        Suggestions::new(self.connection())
    }

    pub fn validation_runs(&self) -> ValidationRuns<'_> {
        ValidationRuns::new(self.connection())
    }

    pub fn workspace_evidence(&self) -> WorkspaceEvidence<'_> {
        WorkspaceEvidence::new(self.connection())
    }

    pub fn workspace_memory(&self) -> WorkspaceMemory<'_> {
        WorkspaceMemory::new(self.connection())
    }

    pub fn workspace_model_bindings(&self) -> ModelBindings<'_> {
        ModelBindings::new(self.connection())
    }

    pub fn workspaces(&self) -> Workspaces<'_> {
        Workspaces::new(self.connection())
    }

    /// Opens the workspace if it is not known yet, then creates a conversation in it. Both writes run in one
    /// transaction: a failed conversation insert leaves the workspace row as it was.
    pub fn create_conversation(&self, workspace: &str) -> Result<ConversationRow> {
        transaction::run(self.connection(), |conn| {
            Workspaces::new(conn).touch(workspace)?;
            Conversations::new(conn).create(workspace)
        })
    }

    /// Stores an assistant reply, its markdown copy and its artifacts in one transaction. A
    /// refused artifact rolls back the whole reply, so a conversation never shows half of it.
    pub fn save_assistant_turn(&self, turn: &AssistantTurn<'_>) -> Result<MessageRow> {
        transaction::run(self.connection(), |conn| {
            let message = insert_message(
                conn,
                turn.conversation_id,
                MessageRole::Assistant,
                turn.content,
                turn.metadata,
            )?;
            let artifacts = Artifacts::new(conn);
            artifacts.add(&NewArtifact {
                conversation_id: turn.conversation_id,
                workspace: turn.workspace,
                kind: "markdown",
                title: turn.markdown_title,
                file_path: None,
                content: Some(turn.content),
                metadata: None,
            })?;
            for artifact in turn.artifacts {
                artifacts.add(&NewArtifact {
                    conversation_id: turn.conversation_id,
                    workspace: turn.workspace,
                    kind: artifact.kind,
                    title: artifact.title,
                    file_path: artifact.file_path,
                    content: artifact.content,
                    metadata: artifact.metadata,
                })?;
            }
            Ok(message)
        })
    }
}
