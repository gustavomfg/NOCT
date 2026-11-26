//! The agents of the modes in Ajustes › Agentes: each agent's instruction and model, the save of either, and the way back to
//! the default. The shell keeps the agents and checks every value; this view sends what the person typed and shows what the
//! shell answered. Each editor keeps its own signals, so a save updates the values in place and the field keeps the focus.
//! The rules that do not touch the page are here too, so that they are tested natively.

use leptos::prelude::*;
use noct_core::limits::agents::MAXIMUM_INSTRUCTION_BYTES;
use serde::Deserialize;
use serde_json::json;

use crate::ipc::{self, Failure, Outcome};
use crate::select::{self, Choice};

/// The shell's commands of the agents.
pub const LIST: &str = "agents_list";
pub const SET: &str = "agents_set";
pub const RESTORE: &str = "agents_restore";

/// The modes in the order the shell lists them.
const MODES: [&str; 3] = ["review", "build", "docs"];

/// The two choices of an agent's model: the project's model, or one of its own.
const PROJECT: &str = "project";
const OWN: &str = "own";
const MODEL_CHOICES: [Choice; 2] = [
    Choice {
        value: PROJECT,
        label: "Modelo do projeto",
    },
    Choice {
        value: OWN,
        label: "Modelo próprio",
    },
];

/// One agent as the shell sends it. The name is the shell's and is not edited.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Agent {
    pub mode: String,
    pub name: String,
    pub instruction: String,
    #[serde(default)]
    pub model: Option<String>,
}

/// Reads a reply that carries the agents: one agent for each mode, and nothing else.
pub fn settle(outcome: Outcome) -> Result<Vec<Agent>, Failure> {
    let text = ipc::reply(outcome)?;
    let agents: Vec<Agent> = serde_json::from_str(&text).map_err(|_| ipc::invalid_response())?;
    let complete = agents.len() == MODES.len()
        && MODES
            .iter()
            .all(|mode| agents.iter().filter(|agent| agent.mode == *mode).count() == 1);
    if complete {
        Ok(agents)
    } else {
        Err(ipc::invalid_response())
    }
}

/// The page ids of one agent's controls. Each mode has its own, so each label names exactly one control.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Ids {
    pub testid: &'static str,
    pub choice: &'static str,
    pub instruction: &'static str,
    pub size: &'static str,
    pub model: &'static str,
}

pub fn ids_of(mode: &str) -> Ids {
    match mode {
        "build" => Ids {
            testid: "agent-build",
            choice: "agent-build-choice",
            instruction: "agent-build-instruction",
            size: "agent-build-size",
            model: "agent-build-model",
        },
        "docs" => Ids {
            testid: "agent-docs",
            choice: "agent-docs-choice",
            instruction: "agent-docs-instruction",
            size: "agent-docs-size",
            model: "agent-docs-model",
        },
        _ => Ids {
            testid: "agent-review",
            choice: "agent-review-choice",
            instruction: "agent-review-instruction",
            size: "agent-review-size",
            model: "agent-review-model",
        },
    }
}

/// The line under an agent's name. Only the chat calls the model of Revisão today; the other two keep their instruction
/// until a call uses it, and the line says so.
pub fn description_of(mode: &str) -> &'static str {
    match mode {
        "review" => "Responde no chat do modo Revisar.",
        _ => "Ainda sem chamada de IA própria. A instrução fica guardada.",
    }
}

/// The mode name of a value of the shell, as a static name, or `None` when it is not a mode.
pub fn static_mode(mode: &str) -> Option<&'static str> {
    MODES.iter().copied().find(|known| *known == mode)
}

/// The payload of `agents_set`: the mode, the instruction, and the model, which is `null` for the project's.
pub fn set_payload(mode: &str, instruction: &str, model: Option<&str>) -> String {
    json!({"input": {"mode": mode, "instruction": instruction, "model": model}}).to_string()
}

/// The payload of `agents_restore`.
pub fn restore_payload(mode: &str) -> String {
    json!({"input": {"mode": mode}}).to_string()
}

/// What the person typed for an agent: its instruction, and its model when the agent has its own.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Draft {
    pub instruction: String,
    pub model: Option<String>,
}

/// The draft as it would be saved, or `None` when it cannot be: an own model with no name is not a model.
pub fn draft_of(instruction: &str, own_model: bool, model_text: &str) -> Option<Draft> {
    let model = if own_model {
        let id = model_text.trim();
        if id.is_empty() {
            return None;
        }
        Some(id.to_string())
    } else {
        None
    };
    Some(Draft {
        instruction: instruction.to_string(),
        model,
    })
}

/// Whether a draft can be saved: its instruction has text, fits the limit in bytes, and differs from the saved one. The
/// shell checks the same limit; this check only keeps the button off when the answer is known in advance.
pub fn can_save(draft: &Draft, saved: &Agent) -> bool {
    let instruction = draft.instruction.trim();
    let fits = !instruction.is_empty() && draft.instruction.len() <= MAXIMUM_INSTRUCTION_BYTES;
    let changed = instruction != saved.instruction.trim() || draft.model != saved.model;
    fits && changed
}

/// The size of an instruction under its limit, in the bytes the shell counts.
pub fn size_words(text: &str) -> String {
    format!("{} de {} bytes", text.len(), MAXIMUM_INSTRUCTION_BYTES)
}

/// One agent being edited: what the shell last saved, and the draft the person is changing. Its signals belong to the group,
/// so a save or a restore updates them in place.
#[derive(Clone, Copy)]
struct Editor {
    mode: &'static str,
    saved: RwSignal<Agent>,
    instruction: RwSignal<String>,
    own_model: RwSignal<bool>,
    model: RwSignal<String>,
    busy: RwSignal<bool>,
    notice: RwSignal<Option<&'static str>>,
    failure: RwSignal<Option<String>>,
}

impl Editor {
    fn of(agent: Agent) -> Option<Self> {
        let mode = static_mode(&agent.mode)?;
        Some(Self {
            mode,
            instruction: RwSignal::new(agent.instruction.clone()),
            own_model: RwSignal::new(agent.model.is_some()),
            model: RwSignal::new(agent.model.clone().unwrap_or_default()),
            saved: RwSignal::new(agent),
            busy: RwSignal::new(false),
            notice: RwSignal::new(None),
            failure: RwSignal::new(None),
        })
    }

    fn draft(&self) -> Option<Draft> {
        draft_of(
            &self.instruction.get(),
            self.own_model.get(),
            &self.model.get(),
        )
    }

    fn savable(&self) -> bool {
        self.draft()
            .is_some_and(|draft| can_save(&draft, &self.saved.get()))
    }
}

/// Puts the shell's values of one mode into its editor, and leaves the other editors as the person left them.
fn apply(editors: RwSignal<Vec<Editor>>, mode: &str, agents: Vec<Agent>) {
    let Some(agent) = agents.into_iter().find(|agent| agent.mode == mode) else {
        return;
    };
    let Some(editor) =
        editors.with_untracked(|list| list.iter().find(|editor| editor.mode == mode).copied())
    else {
        return;
    };
    editor.instruction.set(agent.instruction.clone());
    editor.own_model.set(agent.model.is_some());
    editor.model.set(agent.model.clone().unwrap_or_default());
    editor.saved.set(agent);
}

/// Sends one command for an editor and shows what became of it.
fn run(
    editor: Editor,
    editors: RwSignal<Vec<Editor>>,
    command: &'static str,
    payload: String,
    done: &'static str,
) {
    editor.busy.set(true);
    editor.notice.set(None);
    editor.failure.set(None);
    ipc::call_with(command, &payload, move |outcome| {
        editor.busy.set(false);
        match settle(outcome) {
            Ok(agents) => {
                apply(editors, editor.mode, agents);
                editor.notice.set(Some(done));
            }
            Err(failure) => editor.failure.set(Some(failure.message)),
        }
    });
}

fn save(editor: Editor, editors: RwSignal<Vec<Editor>>) {
    let Some(draft) = editor.draft() else {
        return;
    };
    let payload = set_payload(editor.mode, &draft.instruction, draft.model.as_deref());
    run(editor, editors, SET, payload, "Agente salvo.");
}

fn restore(editor: Editor, editors: RwSignal<Vec<Editor>>) {
    let payload = restore_payload(editor.mode);
    run(editor, editors, RESTORE, payload, "Padrão restaurado.");
}

/// The Ajustes group of the agents. The list is read when the group is drawn; a failure says why and offers nothing to edit.
pub fn agents_group() -> impl IntoView {
    let editors = RwSignal::new(Vec::<Editor>::new());
    let loaded = RwSignal::new(false);
    let load_failure = RwSignal::new(None::<Failure>);
    ipc::call(LIST, move |outcome| match settle(outcome) {
        Ok(agents) => {
            editors.set(agents.into_iter().filter_map(Editor::of).collect());
            loaded.set(true);
        }
        Err(failure) => load_failure.set(Some(failure)),
    });
    view! {
        <div class="group" data-testid="agents">
            <div class="group__title">"Agentes"</div>
            {move || {
                load_failure
                    .get()
                    .map(|failure| {
                        view! {
                            <p class="note note--error" role="alert" data-testid="agents-error">{failure.message}</p>
                        }
                    })
            }}
            {move || {
                (!loaded.get() && load_failure.get().is_none())
                    .then(|| {
                        view! {
                            <p class="note" role="status" data-testid="agents-loading">"Carregando agentes…"</p>
                        }
                    })
            }}
            <For
                each=move || editors.get()
                key=|editor| editor.mode
                children=move |editor| agent_view(editor, editors)
            />
        </div>
    }
}

fn agent_view(editor: Editor, editors: RwSignal<Vec<Editor>>) -> impl IntoView {
    let ids = ids_of(editor.mode);
    let name = move || editor.saved.with(|agent| agent.name.clone());
    let size = move || size_words(&editor.instruction.get());
    let choice = Signal::derive(move || {
        if editor.own_model.get() {
            OWN.to_string()
        } else {
            PROJECT.to_string()
        }
    });
    let choose = move |value: &'static str| editor.own_model.set(value == OWN);
    view! {
        <div class="item agent" data-testid=ids.testid>
            <div class="item__txt">
                <div class="item__name">{name}</div>
                <div class="item__desc">{description_of(editor.mode)}</div>
            </div>
            <div class="agent-field">
                <label class="item__name" for=ids.instruction>"Instrução"</label>
                <textarea
                    class="field agent-instruction"
                    id=ids.instruction
                    rows="6"
                    aria-describedby=ids.size
                    prop:value=move || editor.instruction.get()
                    on:input=move |event| editor.instruction.set(event_target_value(&event))
                    disabled=move || editor.busy.get()
                ></textarea>
                <div class="note" id=ids.size>{size}</div>
            </div>
            <div class="agent-field">
                <label class="item__name" for=ids.choice>"Modelo"</label>
                {select::select(
                    ids.choice,
                    MODEL_CHOICES.to_vec(),
                    choice,
                    PROJECT,
                    Signal::derive(move || editor.busy.get()),
                    choose,
                )}
                <div class="agent-model" hidden=move || !editor.own_model.get()>
                    <label class="item__desc" for=ids.model>"Identificador do modelo"</label>
                    <input
                        class="field field--mono"
                        id=ids.model
                        type="text"
                        placeholder="vendor/modelo"
                        prop:value=move || editor.model.get()
                        on:input=move |event| editor.model.set(event_target_value(&event))
                        disabled=move || editor.busy.get()
                    />
                </div>
            </div>
            <div class="actions">
                <button
                    class="btn"
                    type="button"
                    disabled=move || editor.busy.get() || !editor.savable()
                    on:click=move |_| save(editor, editors)
                >
                    "Salvar"
                </button>
                <button
                    class="btn btn--ghost"
                    type="button"
                    disabled=move || editor.busy.get()
                    on:click=move |_| restore(editor, editors)
                >
                    "Restaurar padrão"
                </button>
            </div>
            {move || {
                editor
                    .notice
                    .get()
                    .map(|words| view! { <p class="note" role="status">{words}</p> })
            }}
            {move || {
                editor
                    .failure
                    .get()
                    .map(|message| view! { <p class="note note--error" role="alert">{message}</p> })
            }}
        </div>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn agent(mode: &str, instruction: &str, model: Option<&str>) -> Agent {
        Agent {
            mode: mode.to_string(),
            name: "Nome".to_string(),
            instruction: instruction.to_string(),
            model: model.map(str::to_string),
        }
    }

    fn all_three() -> String {
        serde_json::json!([
            {"mode": "review", "name": "Revisão", "instruction": "a", "model": null},
            {"mode": "build", "name": "Construção", "instruction": "b", "model": null},
            {"mode": "docs", "name": "Documentação", "instruction": "c", "model": "vendor/m"}
        ])
        .to_string()
    }

    #[test]
    fn a_reply_with_one_agent_per_mode_is_read() {
        let agents = settle(Outcome::Resolved(all_three())).expect("the three agents are read");
        assert_eq!(agents.len(), 3);
        assert_eq!(agents[2].model.as_deref(), Some("vendor/m"));
    }

    #[test]
    fn a_reply_that_misses_a_mode_or_repeats_one_is_unexpected() {
        let missing = r#"[{"mode":"review","name":"R","instruction":"a","model":null}]"#;
        assert!(settle(Outcome::Resolved(missing.to_string())).is_err());
        let repeated = serde_json::json!([
            {"mode": "review", "name": "R", "instruction": "a", "model": null},
            {"mode": "review", "name": "R", "instruction": "a", "model": null},
            {"mode": "docs", "name": "D", "instruction": "c", "model": null}
        ])
        .to_string();
        assert!(settle(Outcome::Resolved(repeated)).is_err());
    }

    #[test]
    fn a_reply_with_an_unknown_field_is_unexpected() {
        let extra = serde_json::json!([
            {"mode": "review", "name": "R", "instruction": "a", "model": null, "permissions": "all"},
            {"mode": "build", "name": "C", "instruction": "b", "model": null},
            {"mode": "docs", "name": "D", "instruction": "c", "model": null}
        ])
        .to_string();
        assert!(settle(Outcome::Resolved(extra)).is_err());
    }

    #[test]
    fn an_unavailable_shell_is_a_failure_with_its_message() {
        let failure = settle(Outcome::Unavailable).expect_err("no shell, no agents");
        assert_eq!(failure.code, "unavailable");
    }

    #[test]
    fn the_payloads_carry_the_mode_and_the_model_or_null() {
        let saved: serde_json::Value =
            serde_json::from_str(&set_payload("review", "Oi.", None)).expect("JSON");
        assert_eq!(saved["input"]["mode"], "review");
        assert_eq!(saved["input"]["instruction"], "Oi.");
        assert!(saved["input"]["model"].is_null());
        let own: serde_json::Value =
            serde_json::from_str(&set_payload("docs", "Oi.", Some("vendor/m"))).expect("JSON");
        assert_eq!(own["input"]["model"], "vendor/m");
        let restored: serde_json::Value =
            serde_json::from_str(&restore_payload("build")).expect("JSON");
        assert_eq!(restored["input"]["mode"], "build");
    }

    #[test]
    fn an_own_model_without_a_name_has_no_draft() {
        assert!(draft_of("Oi.", true, "   ").is_none());
        assert_eq!(draft_of("Oi.", false, "vendor/m").unwrap().model, None);
        assert_eq!(
            draft_of("Oi.", true, " vendor/m ")
                .unwrap()
                .model
                .as_deref(),
            Some("vendor/m")
        );
    }

    #[test]
    fn a_draft_can_be_saved_only_when_it_has_text_fits_and_changes() {
        let saved = agent("review", "Oi.", None);
        let same = draft_of("Oi.", false, "").unwrap();
        assert!(!can_save(&same, &saved), "an unchanged draft is not saved");
        let changed = draft_of("Outro.", false, "").unwrap();
        assert!(can_save(&changed, &saved));
        let empty = draft_of("  ", false, "").unwrap();
        assert!(!can_save(&empty, &saved));
        let too_long = draft_of(&"a".repeat(MAXIMUM_INSTRUCTION_BYTES + 1), false, "").unwrap();
        assert!(!can_save(&too_long, &saved));
        let only_model = draft_of("Oi.", true, "vendor/m").unwrap();
        assert!(
            can_save(&only_model, &saved),
            "a new model alone is a change"
        );
    }

    #[test]
    fn the_size_counts_the_bytes_the_shell_counts() {
        assert_eq!(size_words("abc"), "3 de 4096 bytes");
        assert_eq!(size_words("á"), "2 de 4096 bytes");
    }

    #[test]
    fn each_mode_has_its_own_ids_and_a_known_name() {
        let ids: Vec<&str> = MODES.iter().map(|mode| ids_of(mode).instruction).collect();
        assert_eq!(ids.len(), 3);
        assert!(ids[0] != ids[1] && ids[1] != ids[2] && ids[0] != ids[2]);
        assert_eq!(static_mode("docs"), Some("docs"));
        assert_eq!(static_mode("other"), None);
    }
}
