//! The consent of a project to send its context to a provider. The consent is one setting per project, under the
//! `maintenance.` prefix, so the settings reads of the shell and of the Electron app hide it. Revoking writes an empty
//! value, as the key reference does. A project that is not saved cannot consent, and the store lock is held only while
//! the setting is read or written.

use std::collections::BTreeMap;
use std::sync::Mutex;

use noct_core::ai::consent::{CONSENT_VERSION, ProjectConsentRequest, ProjectConsentStatus};
use noct_core::ai::provider_key::ProviderKeyProvider;
use noct_db::store::Store;

use crate::error::CommandError;
use crate::services;

/// The prefix of every consent setting. The settings reads hide it, as they hide the runtime keys.
pub const CONSENT_PREFIX: &str = "maintenance.consent.openrouter.";

fn setting_of(workspace: &str) -> String {
    format!("{CONSENT_PREFIX}{workspace}")
}

/// Whether the project has consented. A project that is not saved is refused.
pub fn status(
    store: &Mutex<Store>,
    request: ProjectConsentRequest,
) -> Result<ProjectConsentStatus, CommandError> {
    let request = request
        .validated()
        .map_err(|_| CommandError::InvalidInput)?;
    let guard = store.lock().map_err(|_| CommandError::Internal)?;
    ensure_saved(&guard, &request.workspace)?;
    let stored = guard
        .settings()
        .value(&setting_of(&request.workspace))
        .map_err(|_| CommandError::Persistence)?;
    Ok(ProjectConsentStatus {
        workspace: request.workspace,
        provider: ProviderKeyProvider::OpenRouter,
        granted: stored.as_deref() == Some(CONSENT_VERSION),
    })
}

/// Records the consent of the project for the current version of what is sent.
pub fn grant(
    store: &Mutex<Store>,
    request: ProjectConsentRequest,
) -> Result<ProjectConsentStatus, CommandError> {
    write(store, request, CONSENT_VERSION)
}

/// Removes the consent of the project. The next request from the project needs a new consent.
pub fn revoke(
    store: &Mutex<Store>,
    request: ProjectConsentRequest,
) -> Result<ProjectConsentStatus, CommandError> {
    write(store, request, "")
}

fn write(
    store: &Mutex<Store>,
    request: ProjectConsentRequest,
    value: &str,
) -> Result<ProjectConsentStatus, CommandError> {
    let request = request
        .validated()
        .map_err(|_| CommandError::InvalidInput)?;
    {
        let guard = store.lock().map_err(|_| CommandError::Internal)?;
        ensure_saved(&guard, &request.workspace)?;
        let values = BTreeMap::from([(setting_of(&request.workspace), value.to_string())]);
        guard
            .settings()
            .set(&values)
            .map_err(|_| CommandError::Persistence)?;
    }
    Ok(ProjectConsentStatus {
        granted: value == CONSENT_VERSION,
        workspace: request.workspace,
        provider: ProviderKeyProvider::OpenRouter,
    })
}

/// The project must be saved and authorized. A restored project is unauthorized until it is chosen again, and consent is
/// not recorded for it: the same rule `services` applies to a conversation. Each refusal is `UnknownWorkspace`.
fn ensure_saved(store: &Store, workspace: &str) -> Result<(), CommandError> {
    let authorized = services::workspace_rows(store)?
        .iter()
        .any(|row| row.path == workspace && row.authorized);
    if authorized {
        Ok(())
    } else {
        Err(CommandError::UnknownWorkspace)
    }
}
