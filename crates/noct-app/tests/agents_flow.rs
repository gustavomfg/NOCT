//! The agents of the modes as the shell keeps them: the first run, a save that survives a restart, a refused save, a restore of
//! one agent, a damaged stored value, and the settings the window reads, which do not carry the agents.

mod common;

use std::collections::BTreeMap;

use common::Scratch;
use noct_app::agent_profiles::{self, SETTINGS_KEY};
use noct_app::bootstrap::{self, Persistence};
use noct_app::error::CommandError;
use noct_core::agent::AgentMode;
use noct_core::agents::{
    BUILD_INSTRUCTION, DOCS_INSTRUCTION, REVIEW_INSTRUCTION, RestoreRequest, UpdateRequest,
};
use noct_core::limits::agents::MAXIMUM_INSTRUCTION_BYTES;

fn open(scratch: &Scratch) -> Persistence {
    bootstrap::open(&scratch.path().join("data")).expect("the shell opens")
}

fn update(mode: AgentMode, instruction: &str, model: Option<&str>) -> UpdateRequest {
    UpdateRequest {
        mode,
        instruction: instruction.to_string(),
        model: model.map(str::to_string),
    }
}

#[test]
fn the_first_run_stores_the_three_defaults() {
    let scratch = Scratch::new("agents-first-run");
    let persistence = open(&scratch);
    let guard = persistence.store.lock().expect("the store is not poisoned");
    let views = agent_profiles::list(&guard).expect("the agents are listed");
    assert_eq!(views.len(), 3);
    assert_eq!(views[0].instruction, REVIEW_INSTRUCTION);
    assert_eq!(views[1].instruction, BUILD_INSTRUCTION);
    assert_eq!(views[2].instruction, DOCS_INSTRUCTION);
    assert!(
        guard
            .settings()
            .value(SETTINGS_KEY)
            .expect("the setting is readable")
            .is_some(),
        "the first run writes the defaults"
    );
}

#[test]
fn a_saved_agent_is_still_there_after_a_restart() {
    let scratch = Scratch::new("agents-restart");
    {
        let persistence = open(&scratch);
        let guard = persistence.store.lock().expect("the store is not poisoned");
        agent_profiles::set(
            &guard,
            &update(
                AgentMode::Review,
                "Responda em tópicos.",
                Some("vendor/model"),
            ),
        )
        .expect("the agent is saved");
    }
    let persistence = open(&scratch);
    let guard = persistence.store.lock().expect("the store is not poisoned");
    let agents = agent_profiles::load(&guard).expect("the agents are loaded");
    let review = agents.get(AgentMode::Review);
    assert_eq!(review.instruction, "Responda em tópicos.");
    assert_eq!(review.model.as_deref(), Some("vendor/model"));
}

#[test]
fn a_refused_save_changes_nothing() {
    let scratch = Scratch::new("agents-refused");
    let persistence = open(&scratch);
    let guard = persistence.store.lock().expect("the store is not poisoned");
    let too_long = "a".repeat(MAXIMUM_INSTRUCTION_BYTES + 1);
    let refusal = agent_profiles::set(&guard, &update(AgentMode::Review, &too_long, None))
        .expect_err("an instruction over the limit is refused");
    assert!(matches!(refusal, CommandError::InvalidInput));
    let views = agent_profiles::list(&guard).expect("the agents are listed");
    assert_eq!(views[0].instruction, REVIEW_INSTRUCTION);
}

#[test]
fn a_restore_puts_back_one_agent_only() {
    let scratch = Scratch::new("agents-restore");
    let persistence = open(&scratch);
    let guard = persistence.store.lock().expect("the store is not poisoned");
    agent_profiles::set(&guard, &update(AgentMode::Review, "Revisão própria.", None))
        .expect("the review agent is saved");
    agent_profiles::set(
        &guard,
        &update(AgentMode::Docs, "Documentação própria.", None),
    )
    .expect("the docs agent is saved");
    let views = agent_profiles::restore(
        &guard,
        &RestoreRequest {
            mode: AgentMode::Review,
        },
    )
    .expect("the review agent is restored");
    assert_eq!(views[0].instruction, REVIEW_INSTRUCTION);
    assert_eq!(views[2].instruction, "Documentação própria.");
}

#[test]
fn a_damaged_stored_value_reads_as_the_defaults_and_the_next_save_repairs_it() {
    let scratch = Scratch::new("agents-damaged");
    let persistence = open(&scratch);
    let guard = persistence.store.lock().expect("the store is not poisoned");
    let mut values = BTreeMap::new();
    values.insert(SETTINGS_KEY.to_string(), "não é json".to_string());
    guard
        .settings()
        .set(&values)
        .expect("the damaged value is written");

    let views = agent_profiles::list(&guard).expect("the agents are listed");
    assert_eq!(views[0].instruction, REVIEW_INSTRUCTION);

    agent_profiles::set(&guard, &update(AgentMode::Build, "Texto.", None))
        .expect("the agent is saved");
    let stored = guard
        .settings()
        .value(SETTINGS_KEY)
        .expect("the setting is readable")
        .expect("the setting is stored");
    assert!(
        stored.starts_with('['),
        "the save stores a valid list again"
    );
}

#[test]
fn the_settings_the_window_reads_do_not_carry_the_agents() {
    let scratch = Scratch::new("agents-settings");
    let persistence = open(&scratch);
    let guard = persistence.store.lock().expect("the store is not poisoned");
    let settings = noct_app::services::settings_get(&guard).expect("the settings are read");
    let text = serde_json::to_string(&settings).expect("the settings are JSON");
    assert!(!text.contains(SETTINGS_KEY));
}
