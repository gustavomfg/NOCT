//! The agents of the three modes (Ajustes › Agentes). Each mode has one agent: a name, an instruction and an optional
//! model. The name follows the mode, and the instruction and the model are the person's to change.
//!
//! An agent has no permissions of its own. What a mode may do is enforced by the shell for that mode, and no stored value,
//! instruction or model can widen it: the profile does not carry a field for it, and a stored profile with an unknown field
//! is refused. A stored value that is not valid is replaced by the default of its mode, so a damaged value never stops
//! the chat.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::agent::AgentMode;
use crate::ai::openrouter::validated_model_id;
use crate::limits::agents::MAXIMUM_INSTRUCTION_BYTES;
use crate::validation::ContractError;

/// The modes in the order Ajustes lists them.
pub const MODES: [AgentMode; 3] = [AgentMode::Review, AgentMode::Build, AgentMode::Docs];

/// The instruction of the Revisão agent, which is the chat's default. The rules the shell adds to the chat follow it.
pub const REVIEW_INSTRUCTION: &str =
    "Você responde perguntas sobre o projeto. Use o contexto abaixo e diga quando ele não bastar.";

/// The instruction of Construção and Documentação: the first line of the prompt of "Gerar alteração", which is the only
/// prompt that writes a change today.
pub const CHANGE_INSTRUCTION: &str = "Você gera a alteração de uma sugestão aceita no projeto.";

/// The name of a mode's agent. It comes from the mode, so no stored value sets it.
pub fn name_of(mode: AgentMode) -> &'static str {
    match mode {
        AgentMode::Review => "Revisão",
        AgentMode::Build => "Construção",
        AgentMode::Docs => "Documentação",
    }
}

/// The default instruction of a mode: the text its prompt already had.
pub fn default_instruction(mode: AgentMode) -> &'static str {
    match mode {
        AgentMode::Review => REVIEW_INSTRUCTION,
        AgentMode::Build | AgentMode::Docs => CHANGE_INSTRUCTION,
    }
}

/// One agent as it is stored. `model` is `None` when the agent uses the project's model.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct AgentProfile {
    pub mode: AgentMode,
    pub instruction: String,
    pub model: Option<String>,
}

fn default_profile(mode: AgentMode) -> AgentProfile {
    AgentProfile {
        mode,
        instruction: default_instruction(mode).to_string(),
        model: None,
    }
}

/// The agents of the three modes: one profile per mode, so there is always an agent for each.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Agents {
    review: AgentProfile,
    build: AgentProfile,
    docs: AgentProfile,
}

impl Agents {
    /// The three agents with their defaults: the instruction of each mode and the project's model.
    pub fn defaults() -> Self {
        Self {
            review: default_profile(AgentMode::Review),
            build: default_profile(AgentMode::Build),
            docs: default_profile(AgentMode::Docs),
        }
    }

    pub fn get(&self, mode: AgentMode) -> &AgentProfile {
        match mode {
            AgentMode::Review => &self.review,
            AgentMode::Build => &self.build,
            AgentMode::Docs => &self.docs,
        }
    }

    fn slot_mut(&mut self, mode: AgentMode) -> &mut AgentProfile {
        match mode {
            AgentMode::Review => &mut self.review,
            AgentMode::Build => &mut self.build,
            AgentMode::Docs => &mut self.docs,
        }
    }

    /// Sets the instruction and the model of a mode's agent. Both are checked first: when one fails, nothing changes.
    pub fn update(
        &mut self,
        mode: AgentMode,
        instruction: &str,
        model: Option<&str>,
    ) -> Result<(), ContractError> {
        let instruction = validated_instruction(instruction)?;
        let model = validated_model(model)?;
        let slot = self.slot_mut(mode);
        slot.instruction = instruction;
        slot.model = model;
        Ok(())
    }

    /// Puts a mode's agent back to its default instruction and the project's model.
    pub fn restore(&mut self, mode: AgentMode) {
        *self.slot_mut(mode) = default_profile(mode);
    }

    /// The agents as the interface shows them, in the order of [`MODES`].
    pub fn views(&self) -> Vec<AgentView> {
        MODES
            .iter()
            .map(|&mode| {
                let profile = self.get(mode);
                AgentView {
                    mode,
                    name: name_of(mode),
                    instruction: profile.instruction.clone(),
                    model: profile.model.clone(),
                }
            })
            .collect()
    }

    /// The text kept in the settings: a JSON list with one profile per mode.
    pub fn to_stored(&self) -> String {
        serde_json::to_string(&[&self.review, &self.build, &self.docs]).unwrap_or_default()
    }

    /// Reads the stored text. Each profile is checked on its own: a profile that is not valid, or a second one for a
    /// mode, is ignored, and a mode without a valid profile has its default. Nothing here can fail, so a damaged value
    /// falls back to the defaults and the chat still runs.
    pub fn from_stored(text: &str) -> Self {
        let items: Vec<Value> = serde_json::from_str(text).unwrap_or_default();
        let mut agents = Self::defaults();
        let mut seen: Vec<AgentMode> = Vec::new();
        for item in items {
            let Ok(profile) = serde_json::from_value::<AgentProfile>(item) else {
                continue;
            };
            if seen.contains(&profile.mode) {
                continue;
            }
            let (Ok(instruction), Ok(model)) = (
                validated_instruction(&profile.instruction),
                validated_model(profile.model.as_deref()),
            ) else {
                continue;
            };
            seen.push(profile.mode);
            *agents.slot_mut(profile.mode) = AgentProfile {
                mode: profile.mode,
                instruction,
                model,
            };
        }
        agents
    }
}

/// An agent as the interface shows it. The name is not editable.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentView {
    pub mode: AgentMode,
    pub name: &'static str,
    pub instruction: String,
    pub model: Option<String>,
}

/// The input of `agents_set`: the mode, the instruction, and the model (`null` or empty for the project's model).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct UpdateRequest {
    pub mode: AgentMode,
    pub instruction: String,
    #[serde(default)]
    pub model: Option<String>,
}

/// The input of `agents_restore`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct RestoreRequest {
    pub mode: AgentMode,
}

/// Checks an instruction and returns it as it is kept: line endings become `\n`, the ends are trimmed, and the size is
/// counted in bytes after that. It must not be empty, must not hold a control character other than a line break or a tab,
/// and must fit in [`MAXIMUM_INSTRUCTION_BYTES`].
pub fn validated_instruction(text: &str) -> Result<String, ContractError> {
    let normalized = text.replace("\r\n", "\n").replace('\r', "\n");
    let trimmed = normalized.trim();
    let allowed = trimmed
        .chars()
        .all(|c| !c.is_control() || c == '\n' || c == '\t');
    if trimmed.is_empty() || !allowed || trimmed.len() > MAXIMUM_INSTRUCTION_BYTES {
        return Err(ContractError::invalid("instruction"));
    }
    Ok(trimmed.to_string())
}

/// Checks a model of an agent. Empty or absent means the project's model; otherwise it must be a model identifier.
pub fn validated_model(model: Option<&str>) -> Result<Option<String>, ContractError> {
    match model.map(str::trim).filter(|value| !value.is_empty()) {
        None => Ok(None),
        Some(id) => validated_model_id(id).map(|id| Some(id.to_string())),
    }
}
