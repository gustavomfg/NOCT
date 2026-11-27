//! The agents of the three modes as they are stored (Ajustes › Agentes). The value is one settings key holding a JSON list;
//! `noct_core::agents` checks it. The first run writes the defaults, and a value that is not valid reads as the defaults
//! until it is saved again.

use std::collections::BTreeMap;

use noct_core::agents::{AgentView, Agents, RestoreRequest, UpdateRequest};
use noct_db::store::Store;

use crate::error::CommandError;

/// The settings key of the agents. The settings read returns only the keys the shell names, so this key is not sent to the
/// window as a setting.
pub const SETTINGS_KEY: &str = "agentProfiles";

/// The agents as stored, or the defaults when none are stored or what is stored is not valid.
pub fn load(store: &Store) -> Result<Agents, CommandError> {
    let saved = store
        .settings()
        .value(SETTINGS_KEY)
        .map_err(|_| CommandError::Persistence)?;
    Ok(saved.map_or_else(Agents::defaults, |text| Agents::from_stored(&text)))
}

/// Writes the defaults when no agents are stored yet, which is the first run. A stored value is never overwritten here.
pub fn ensure(store: &Store) -> noct_db::Result<()> {
    if store.settings().value(SETTINGS_KEY)?.is_none() {
        write(store, &Agents::defaults())?;
    }
    Ok(())
}

fn write(store: &Store, agents: &Agents) -> noct_db::Result<()> {
    let mut values = BTreeMap::new();
    values.insert(SETTINGS_KEY.to_string(), agents.to_stored());
    store.settings().set(&values)
}

fn save(store: &Store, agents: &Agents) -> Result<(), CommandError> {
    write(store, agents).map_err(|_| CommandError::Persistence)
}

/// The agents of the three modes, in the order Ajustes lists them.
pub fn list(store: &Store) -> Result<Vec<AgentView>, CommandError> {
    Ok(load(store)?.views())
}

/// Saves the instruction and the model of one agent, and returns the agents as stored afterwards. A refused instruction or
/// model is `InvalidInput`, and nothing is saved.
pub fn set(store: &Store, request: &UpdateRequest) -> Result<Vec<AgentView>, CommandError> {
    let mut agents = load(store)?;
    agents
        .update(request.mode, &request.instruction, request.model.as_deref())
        .map_err(|_| CommandError::InvalidInput)?;
    save(store, &agents)?;
    Ok(agents.views())
}

/// Puts one agent back to its default instruction and the project's model, and returns the agents as stored afterwards.
pub fn restore(store: &Store, request: &RestoreRequest) -> Result<Vec<AgentView>, CommandError> {
    let mut agents = load(store)?;
    agents.restore(request.mode);
    save(store, &agents)?;
    Ok(agents.views())
}
