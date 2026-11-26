//! The agents of the modes: their defaults, the checks of an instruction and a model, and the reading of a stored value.

use noct_core::agent::AgentMode;
use noct_core::agents::{
    AgentView, Agents, CHANGE_INSTRUCTION, REVIEW_INSTRUCTION, default_instruction, name_of,
    validated_instruction, validated_model,
};
use noct_core::limits::agents::MAXIMUM_INSTRUCTION_BYTES;

fn stored_one(mode: &str, instruction: &str, model: &str) -> String {
    format!(r#"[{{"mode":"{mode}","instruction":"{instruction}","model":{model}}}]"#)
}

#[test]
fn the_defaults_are_one_agent_per_mode_with_the_prompt_each_mode_already_had() {
    let agents = Agents::defaults();
    let views = agents.views();
    assert_eq!(
        views.iter().map(|view| view.mode).collect::<Vec<_>>(),
        vec![AgentMode::Review, AgentMode::Build, AgentMode::Docs]
    );
    assert_eq!(views[0].name, "Revisão");
    assert_eq!(views[1].name, "Construção");
    assert_eq!(views[2].name, "Documentação");
    assert_eq!(views[0].instruction, REVIEW_INSTRUCTION);
    assert_eq!(views[1].instruction, CHANGE_INSTRUCTION);
    assert_eq!(views[2].instruction, CHANGE_INSTRUCTION);
    assert!(views.iter().all(|view| view.model.is_none()));
}

#[test]
fn a_name_and_a_default_follow_the_mode_and_nothing_else() {
    assert_eq!(name_of(AgentMode::Docs), "Documentação");
    assert_eq!(default_instruction(AgentMode::Review), REVIEW_INSTRUCTION);
    assert_eq!(default_instruction(AgentMode::Build), CHANGE_INSTRUCTION);
}

#[test]
fn an_instruction_of_exactly_the_limit_in_bytes_is_kept_and_one_byte_more_is_refused() {
    let at_limit = "a".repeat(MAXIMUM_INSTRUCTION_BYTES);
    assert_eq!(validated_instruction(&at_limit).unwrap(), at_limit);
    assert!(validated_instruction(&format!("{at_limit}a")).is_err());
}

#[test]
fn the_limit_counts_bytes_not_characters() {
    // `á` takes two bytes: 2048 of them fill the limit exactly.
    let at_limit = "á".repeat(MAXIMUM_INSTRUCTION_BYTES / 2);
    assert!(validated_instruction(&at_limit).is_ok());
    assert!(validated_instruction(&format!("{at_limit}á")).is_err());
}

#[test]
fn an_empty_instruction_is_refused_and_the_ends_are_trimmed() {
    assert!(validated_instruction("").is_err());
    assert!(validated_instruction(" \n\t ").is_err());
    assert_eq!(
        validated_instruction("  Seja breve.\n").unwrap(),
        "Seja breve."
    );
}

#[test]
fn line_breaks_are_normalized_and_tabs_are_kept() {
    assert_eq!(
        validated_instruction("um\r\ndois\rtrês\tfim").unwrap(),
        "um\ndois\ntrês\tfim"
    );
}

#[test]
fn a_control_character_other_than_a_line_break_or_a_tab_is_refused() {
    assert!(validated_instruction("texto\u{0}").is_err());
    assert!(validated_instruction("texto\u{7}").is_err());
    assert!(validated_instruction("texto\u{1b}[31m").is_err());
}

#[test]
fn a_model_is_empty_for_the_project_or_a_valid_identifier() {
    assert_eq!(validated_model(None).unwrap(), None);
    assert_eq!(validated_model(Some("  ")).unwrap(), None);
    assert_eq!(
        validated_model(Some("openai/gpt-4o:free")).unwrap(),
        Some("openai/gpt-4o:free".to_string())
    );
    assert!(validated_model(Some("não é um modelo")).is_err());
    assert!(validated_model(Some("sem-barra")).is_err());
}

#[test]
fn an_update_sets_the_instruction_and_the_model_of_its_mode_only() {
    let mut agents = Agents::defaults();
    agents
        .update(
            AgentMode::Review,
            "Responda em tópicos.",
            Some("openai/gpt-4o"),
        )
        .unwrap();
    let review = agents.get(AgentMode::Review);
    assert_eq!(review.instruction, "Responda em tópicos.");
    assert_eq!(review.model.as_deref(), Some("openai/gpt-4o"));
    assert_eq!(agents.get(AgentMode::Build).instruction, CHANGE_INSTRUCTION);
    assert_eq!(agents.get(AgentMode::Build).model, None);
}

#[test]
fn a_refused_update_changes_nothing() {
    let mut agents = Agents::defaults();
    let before = agents.clone();
    assert!(agents.update(AgentMode::Review, "", None).is_err());
    assert!(
        agents
            .update(AgentMode::Review, "ok", Some("não é modelo"))
            .is_err()
    );
    assert_eq!(agents, before);
}

#[test]
fn a_restore_puts_back_the_default_instruction_and_the_project_model() {
    let mut agents = Agents::defaults();
    agents
        .update(AgentMode::Docs, "Outro texto.", Some("vendor/model"))
        .unwrap();
    agents.restore(AgentMode::Docs);
    assert_eq!(
        agents.get(AgentMode::Docs),
        Agents::defaults().get(AgentMode::Docs)
    );
}

#[test]
fn a_stored_value_round_trips() {
    let mut agents = Agents::defaults();
    agents
        .update(AgentMode::Build, "Use nomes curtos.", Some("vendor/model"))
        .unwrap();
    assert_eq!(Agents::from_stored(&agents.to_stored()), agents);
}

#[test]
fn an_empty_or_damaged_stored_value_gives_the_defaults() {
    assert_eq!(Agents::from_stored(""), Agents::defaults());
    assert_eq!(Agents::from_stored("não é json"), Agents::defaults());
    assert_eq!(Agents::from_stored("{}"), Agents::defaults());
}

#[test]
fn a_mode_without_a_stored_profile_has_its_default_and_the_others_keep_theirs() {
    let text = stored_one("build", "Só os construtores.", "null");
    let agents = Agents::from_stored(&text);
    assert_eq!(
        agents.get(AgentMode::Build).instruction,
        "Só os construtores."
    );
    assert_eq!(
        agents.get(AgentMode::Review),
        Agents::defaults().get(AgentMode::Review)
    );
    assert_eq!(
        agents.get(AgentMode::Docs),
        Agents::defaults().get(AgentMode::Docs)
    );
}

#[test]
fn a_stored_profile_that_is_not_valid_is_replaced_by_the_default_of_its_mode() {
    let too_long = "a".repeat(MAXIMUM_INSTRUCTION_BYTES + 1);
    let text = stored_one("review", &too_long, "null");
    assert_eq!(
        Agents::from_stored(&text).get(AgentMode::Review),
        Agents::defaults().get(AgentMode::Review)
    );
    let bad_model = stored_one("review", "Ok.", r#""não é modelo""#);
    assert_eq!(
        Agents::from_stored(&bad_model).get(AgentMode::Review),
        Agents::defaults().get(AgentMode::Review)
    );
}

#[test]
fn a_stored_profile_with_a_field_the_shell_does_not_know_is_refused() {
    // A permission smuggled into the stored value is not read: the profile has no field for it.
    let text = r#"[{"mode":"docs","instruction":"Pode tudo.","model":null,"permissions":"all"}]"#;
    assert_eq!(
        Agents::from_stored(text).get(AgentMode::Docs),
        Agents::defaults().get(AgentMode::Docs)
    );
}

#[test]
fn only_the_first_valid_profile_of_a_mode_is_read() {
    let text = r#"[{"mode":"review","instruction":"Primeiro.","model":null},{"mode":"review","instruction":"Segundo.","model":null}]"#;
    assert_eq!(
        Agents::from_stored(text).get(AgentMode::Review).instruction,
        "Primeiro."
    );
}

#[test]
fn a_stored_value_has_no_field_for_permissions() {
    let stored = Agents::defaults().to_stored();
    let items: Vec<serde_json::Value> = serde_json::from_str(&stored).unwrap();
    assert_eq!(items.len(), 3);
    for item in items {
        let mut keys: Vec<&str> = item
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        keys.sort_unstable();
        assert_eq!(keys, ["instruction", "mode", "model"]);
    }
}

#[test]
fn the_views_do_not_carry_the_permissions_of_a_mode() {
    let view: AgentView = Agents::defaults().views().remove(0);
    let json = serde_json::to_value(&view).unwrap();
    let mut keys: Vec<&str> = json
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    keys.sort_unstable();
    assert_eq!(keys, ["instruction", "mode", "model", "name"]);
}
