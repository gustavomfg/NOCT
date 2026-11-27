//! The provider chat of the review panel: the shell's switch, the request, the frames of an answer, and the state of one
//! question. The chat is read-only, and the shell decides whether it is on: the interface shows what the shell reports and
//! never calls a provider itself. The logic is here, apart from the view, so that it is tested natively.
//!
//! A question has one run. A frame of an older run, or one that arrives after a cancel, is ignored, so a late event cannot
//! change the answer of a newer question.

use noct_core::limits::provider_chat::MAXIMUM_QUESTION_CHARACTERS;
use serde::Deserialize;
use serde_json::json;

/// The shell's command that reports the switch.
pub const STATUS: &str = "provider_chat_status";
/// The command that turns the chat on or off, from Ajustes or from the line over the composer.
pub const SET_ENABLED: &str = "provider_chat_set_enabled";

/// The payload of [`SET_ENABLED`].
pub fn switch_payload(enabled: bool) -> String {
    format!(r#"{{"input":{{"enabled":{enabled}}}}}"#)
}
/// The shell's command that starts a question. Its answer comes on the channel it is given.
pub const SEND: &str = "provider_chat_send";
/// The shell's command that stops the running question.
pub const CANCEL: &str = "provider_chat_cancel";
/// The shell's commands that keep an answer in a conversation, which the person asks for.
pub const CONVERSATION_CREATE: &str = "conversations_create";
pub const SAVE_ASSISTANT: &str = "ai_save_assistant";

/// The switch as the shell reports it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ChatStatus {
    pub enabled: bool,
    /// Whether the findings of an answer are saved in Sugestões. On when the shell does not say.
    #[serde(default = "on")]
    pub findings: bool,
}

fn on() -> bool {
    true
}

/// The command that turns the saving of findings on or off, from Ajustes.
pub const SET_FINDINGS: &str = "provider_chat_set_findings";

/// One event of an answer, as the shell sends it on the channel.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Frame {
    /// The agent that answers and the model it uses, sent once before the first delta, so the answer can name them.
    Agent {
        name: String,
        model: String,
    },
    Delta {
        text: String,
    },
    /// The changes the answer proposed, recorded by the shell as pending, the blocks it refused, and the notice of the
    /// mode's rule when it refused one.
    Proposed {
        changes: Vec<ProposedChange>,
        refused: usize,
        #[serde(default)]
        notice: Option<String>,
    },
    /// What became of the findings of the answer, sent after a complete answer whatever happened to them.
    Findings(FindingsOutcome),
    Done,
    Failed {
        code: String,
    },
}

/// A change an answer proposed: the change in Construir, and its file.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ProposedChange {
    pub id: String,
    pub path: String,
}

/// What became of the findings of an answer, as the shell reports them. The status and the counts are the shell's; the
/// words under the answer are the interface's, chosen by the status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FindingsOutcome {
    pub status: FindingsStatus,
    pub saved: usize,
    pub discarded: usize,
}

impl FindingsOutcome {
    /// Whether findings were stored in Sugestões, so the list there must be read again.
    pub fn saved_any(&self) -> bool {
        self.status == FindingsStatus::Saved && self.saved > 0
    }
}

/// The status of the findings of an answer, as the shell names it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum FindingsStatus {
    Off,
    Absent,
    Invalid,
    Empty,
    Discarded,
    Saved,
    Failed,
}

/// The line under an answer that says what became of its findings. Every status has words, so nothing is silent. The words
/// carry the counts only, never the text of the model.
pub fn findings_words(outcome: &FindingsOutcome) -> String {
    let plural = |count: usize, one: &str, many: &str| {
        if count == 1 {
            format!("1 {one}")
        } else {
            format!("{count} {many}")
        }
    };
    let refused = |count: usize| {
        format!(
            "{}, por limite ou repetição.",
            plural(count, "achado rejeitado", "achados rejeitados")
        )
    };
    match outcome.status {
        FindingsStatus::Off => {
            "Achados não salvos: o salvamento está desligado em Ajustes.".to_string()
        }
        FindingsStatus::Absent => {
            "O modelo não devolveu achados estruturados. Nada foi enviado para Sugestões."
                .to_string()
        }
        FindingsStatus::Invalid => {
            "Achados rejeitados: o bloco não lê como achados válidos. Nada foi enviado para Sugestões."
                .to_string()
        }
        FindingsStatus::Empty => "O bloco veio vazio: nenhum achado para Sugestões.".to_string(),
        FindingsStatus::Discarded => format!(
            "{} Nada foi enviado para Sugestões.",
            refused(outcome.discarded)
        ),
        FindingsStatus::Saved => {
            let saved = plural(
                outcome.saved,
                "achado enviado para Sugestões.",
                "achados enviados para Sugestões.",
            );
            if outcome.discarded == 0 {
                saved
            } else {
                format!("{saved} {}", refused(outcome.discarded))
            }
        }
        FindingsStatus::Failed => {
            "Não foi possível enviar os achados para Sugestões. Nada foi salvo.".to_string()
        }
    }
}

/// Reads one event from the text the channel handed on. An event that is not one of the known kinds is ignored.
pub fn parse_frame(text: &str) -> Option<Frame> {
    serde_json::from_str(text).ok()
}

/// The argument of a question: the project, the text, the mode it is asked in, and the channel, which the shell's command
/// takes as `onEvent`. The mode is `review`, `build` or `docs`: it picks the agent and the rule of the answer.
pub fn send_payload(workspace: &str, question: &str, mode: &str, channel: &str) -> String {
    json!({
        "input": { "workspace": workspace, "question": question, "mode": mode },
        "onEvent": channel
    })
    .to_string()
}

/// The argument that creates a conversation in a project.
pub fn conversation_payload(workspace: &str) -> String {
    json!({ "input": workspace }).to_string()
}

/// The argument that keeps an answer in a conversation.
pub fn save_payload(conversation_id: &str, content: &str) -> String {
    json!({ "input": { "conversationId": conversation_id, "content": content } }).to_string()
}

/// The part of a created conversation that the save needs.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct CreatedConversation {
    pub id: String,
}

/// Whether a question can be sent: it has text, and no more than the shell accepts.
pub fn question_is_sendable(question: &str) -> bool {
    let trimmed = question.trim();
    !trimmed.is_empty() && trimmed.chars().count() <= MAXIMUM_QUESTION_CHARACTERS
}

/// The words for the switch.
pub fn switch_words(enabled: bool) -> &'static str {
    if enabled {
        "Ligado. Vão os arquivos escolhidos e a lista de nomes do projeto."
    } else {
        "Desligado. Nada é enviado ao OpenRouter."
    }
}

/// The line that says where the answer stands. A failure has its own words, so this line is empty for it.
pub fn phase_words(phase: &Phase) -> &'static str {
    match phase {
        Phase::Idle | Phase::Failed(_) => "",
        Phase::Asking => "Enviando a pergunta…",
        Phase::Answering => "Respondendo…",
        Phase::Done => "Resposta completa. Você pode salvá-la na conversa.",
        Phase::Cancelled => "Cancelado. A resposta parcial não é salva.",
    }
}

/// The words for a failure that ends an answer. The shell sends a code, never the key or the provider's text, so the
/// words are chosen by the code. A code the interface does not know gets the general words.
pub fn stream_failure_words(code: &str) -> &'static str {
    match code {
        "provider-unauthorized" => {
            "O OpenRouter recusou a chave. Confira a chave em Ajustes. Nada foi salvo."
        }
        "provider-rate-limited" => {
            "O OpenRouter limitou as chamadas. Tente de novo em instantes. Nada foi salvo."
        }
        "provider-unavailable" | "transport" => {
            "Não foi possível falar com o OpenRouter. Verifique a conexão. Nada foi salvo."
        }
        "timeout" => "O OpenRouter demorou demais e a resposta foi interrompida. Nada foi salvo.",
        "stream-truncated" => "A resposta foi interrompida antes do fim. Nada foi salvo.",
        "stream-malformed" => {
            "A resposta veio num formato que o NOCT não reconhece. Nada foi salvo."
        }
        "response-too-large" => "A resposta passou do limite e foi interrompida. Nada foi salvo.",
        "provider-error" | "provider-rejected" => {
            "O OpenRouter não conseguiu responder. Nada foi salvo."
        }
        _ => "A resposta foi interrompida por um erro do provedor. Nada foi salvo.",
    }
}

/// What the answer is doing.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum Phase {
    #[default]
    Idle,
    /// The question is sent, and nothing has arrived yet.
    Asking,
    /// The answer is arriving.
    Answering,
    /// The answer is complete.
    Done,
    /// The person stopped the answer. What arrived stays on screen, and is not saved.
    Cancelled,
    /// The answer ended with a failure, or the shell refused the question. The words are for the person.
    Failed(String),
}

/// One question and its answer. The run number tells the current question apart from older ones.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Session {
    run: u64,
    phase: Phase,
    workspace: String,
    answer: String,
    saved: bool,
    /// The changes the answer proposed, the blocks refused and the notice of the mode's rule, once the shell has recorded them.
    proposals: Option<(Vec<ProposedChange>, usize, Option<String>)>,
    /// What became of the findings of the answer, once the shell has said.
    findings: Option<FindingsOutcome>,
    /// The agent that answers and its model, as the shell named them for this question.
    agent: Option<(String, String)>,
}

impl Session {
    /// Starts a question about `workspace`. The answer of the last question is cleared, and the new run is the only one
    /// whose events are taken.
    pub fn start(&mut self, workspace: &str) -> u64 {
        self.run += 1;
        self.phase = Phase::Asking;
        self.workspace = workspace.to_string();
        self.answer.clear();
        self.proposals = None;
        self.findings = None;
        self.agent = None;
        self.saved = false;
        self.run
    }

    /// Applies one event of the run. An event of another run, or after the answer has ended, is ignored.
    pub fn frame(&mut self, run: u64, frame: Frame) {
        if !self.accepts(run) {
            return;
        }
        match frame {
            Frame::Agent { name, model } => self.agent = Some((name, model)),
            Frame::Delta { text } => {
                self.answer.push_str(&text);
                self.phase = Phase::Answering;
            }
            Frame::Proposed {
                changes,
                refused,
                notice,
            } => self.proposals = Some((changes, refused, notice)),
            Frame::Findings(outcome) => self.findings = Some(outcome),
            Frame::Done => self.phase = Phase::Done,
            Frame::Failed { code } => {
                self.phase = Phase::Failed(stream_failure_words(&code).to_string());
            }
        }
    }

    /// Records that the shell refused the question of the run.
    pub fn refused(&mut self, run: u64, message: String) {
        if self.run == run {
            self.phase = Phase::Failed(message);
        }
    }

    /// Stops the answer that is running. The text so far stays, and is not saved.
    pub fn cancel(&mut self) {
        if self.busy() {
            self.phase = Phase::Cancelled;
        }
    }

    /// Records that the answer of the run was saved in a conversation.
    pub fn mark_saved(&mut self, run: u64) {
        if self.run == run && self.phase == Phase::Done {
            self.saved = true;
        }
    }

    fn accepts(&self, run: u64) -> bool {
        self.run == run && self.busy()
    }

    pub fn busy(&self) -> bool {
        matches!(self.phase, Phase::Asking | Phase::Answering)
    }

    /// Whether the answer can be saved: it is complete, it has text, and it is not saved yet.
    pub fn can_save(&self) -> bool {
        self.phase == Phase::Done && !self.saved && !self.answer.trim().is_empty()
    }

    pub fn phase(&self) -> &Phase {
        &self.phase
    }

    pub fn answer(&self) -> &str {
        &self.answer
    }

    /// The changes the answer proposed, and how many blocks were refused.
    pub fn proposals(&self) -> Option<&(Vec<ProposedChange>, usize, Option<String>)> {
        self.proposals.as_ref()
    }

    /// What became of the findings of the answer, or `None` while the shell has not said.
    pub fn findings(&self) -> Option<&FindingsOutcome> {
        self.findings.as_ref()
    }

    /// The agent of this question and its model, once the shell has named them.
    pub fn agent(&self) -> Option<&(String, String)> {
        self.agent.as_ref()
    }

    pub fn workspace(&self) -> &str {
        &self.workspace
    }

    pub fn run(&self) -> u64 {
        self.run
    }

    pub fn saved(&self) -> bool {
        self.saved
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_agent_frame_names_the_agent_and_its_model_for_one_question_only() {
        let frame = parse_frame(r#"{"kind":"agent","name":"Revisão","model":"vendor/m"}"#)
            .expect("the agent frame parses");
        let mut session = Session::default();
        let run = session.start("/projeto");
        session.frame(run, frame);
        assert_eq!(
            session.agent(),
            Some(&("Revisão".to_string(), "vendor/m".to_string()))
        );
        session.start("/projeto");
        assert_eq!(session.agent(), None, "a new question names no agent yet");
    }

    fn delta(text: &str) -> Frame {
        Frame::Delta {
            text: text.to_string(),
        }
    }

    #[test]
    fn the_send_argument_names_the_project_the_question_and_the_channel() {
        let value: serde_json::Value = serde_json::from_str(&send_payload(
            "/projetos/app",
            "o que mudou?",
            "build",
            "__CHANNEL__:7",
        ))
        .unwrap();
        assert_eq!(value["input"]["workspace"], "/projetos/app");
        assert_eq!(value["input"]["question"], "o que mudou?");
        assert_eq!(value["input"]["mode"], "build");
        assert_eq!(value["onEvent"], "__CHANNEL__:7");
        assert!(
            value.get("key").is_none(),
            "no key is ever sent by the interface"
        );
    }

    #[test]
    fn the_save_arguments_name_the_conversation_and_the_content() {
        let created: serde_json::Value =
            serde_json::from_str(&conversation_payload("/projetos/app")).unwrap();
        assert_eq!(created["input"], "/projetos/app");
        let saved: serde_json::Value =
            serde_json::from_str(&save_payload("conv-1", "resposta")).unwrap();
        assert_eq!(saved["input"]["conversationId"], "conv-1");
        assert_eq!(saved["input"]["content"], "resposta");
    }

    #[test]
    fn a_frame_is_read_only_when_it_is_a_known_kind() {
        assert_eq!(
            parse_frame(r#"{"kind":"delta","text":"oi"}"#),
            Some(delta("oi"))
        );
        assert_eq!(parse_frame(r#"{"kind":"done"}"#), Some(Frame::Done));
        assert_eq!(
            parse_frame(r#"{"kind":"failed","code":"timeout"}"#),
            Some(Frame::Failed {
                code: "timeout".to_string()
            })
        );
        assert_eq!(parse_frame(r#"{"kind":"command","text":"rm"}"#), None);
        assert_eq!(parse_frame("não é json"), None);
    }

    fn outcome(status: FindingsStatus, saved: usize, discarded: usize) -> FindingsOutcome {
        FindingsOutcome {
            status,
            saved,
            discarded,
        }
    }

    #[test]
    fn the_findings_frame_carries_the_status_and_the_counts() {
        assert_eq!(
            parse_frame(r#"{"kind":"findings","status":"saved","saved":2,"discarded":1}"#),
            Some(Frame::Findings(outcome(FindingsStatus::Saved, 2, 1)))
        );
        assert_eq!(
            parse_frame(r#"{"kind":"findings","status":"absent","saved":0,"discarded":0}"#),
            Some(Frame::Findings(outcome(FindingsStatus::Absent, 0, 0)))
        );
    }

    #[test]
    fn every_findings_status_says_what_happened() {
        assert_eq!(
            findings_words(&outcome(FindingsStatus::Saved, 1, 0)),
            "1 achado enviado para Sugestões."
        );
        assert_eq!(
            findings_words(&outcome(FindingsStatus::Saved, 2, 1)),
            "2 achados enviados para Sugestões. 1 achado rejeitado, por limite ou repetição."
        );
        assert_eq!(
            findings_words(&outcome(FindingsStatus::Discarded, 0, 3)),
            "3 achados rejeitados, por limite ou repetição. Nada foi enviado para Sugestões."
        );
        assert!(
            findings_words(&outcome(FindingsStatus::Absent, 0, 0))
                .contains("não devolveu achados estruturados")
        );
        assert!(
            findings_words(&outcome(FindingsStatus::Invalid, 0, 0))
                .contains("não lê como achados válidos")
        );
        assert!(
            findings_words(&outcome(FindingsStatus::Off, 0, 0)).contains("desligado em Ajustes")
        );
        assert!(findings_words(&outcome(FindingsStatus::Empty, 0, 0)).contains("nenhum achado"));
        assert!(
            findings_words(&outcome(FindingsStatus::Failed, 0, 0)).contains("Não foi possível")
        );
    }

    #[test]
    fn only_saved_findings_ask_the_list_to_be_read_again() {
        assert!(outcome(FindingsStatus::Saved, 1, 0).saved_any());
        assert!(!outcome(FindingsStatus::Saved, 0, 0).saved_any());
        assert!(!outcome(FindingsStatus::Discarded, 0, 2).saved_any());
    }

    #[test]
    fn a_session_keeps_the_findings_of_its_run_only() {
        let mut session = Session::default();
        let first = session.start("/projeto");
        session.frame(first, delta("Texto."));
        session.frame(first, Frame::Findings(outcome(FindingsStatus::Saved, 1, 0)));
        assert_eq!(
            session.findings(),
            Some(&outcome(FindingsStatus::Saved, 1, 0))
        );
        session.frame(first, Frame::Done);
        session.start("/projeto");
        assert_eq!(session.findings(), None);
    }

    #[test]
    fn a_question_is_sent_only_with_text_and_within_the_limit() {
        assert!(!question_is_sendable(""));
        assert!(!question_is_sendable("   \n "));
        assert!(question_is_sendable(" pergunta "));
        assert!(question_is_sendable(
            &"x".repeat(MAXIMUM_QUESTION_CHARACTERS)
        ));
        assert!(!question_is_sendable(
            &"x".repeat(MAXIMUM_QUESTION_CHARACTERS + 1)
        ));
    }

    #[test]
    fn the_switch_words_say_what_is_sent_when_it_is_off() {
        assert!(switch_words(false).contains("Desligado"));
        assert!(switch_words(false).contains("Nada é enviado"));
        assert!(switch_words(true).contains("Ligado"));
    }

    #[test]
    fn every_known_failure_code_has_its_own_words_and_an_unknown_one_has_the_general_words() {
        let codes = [
            "provider-unauthorized",
            "provider-rate-limited",
            "provider-unavailable",
            "transport",
            "timeout",
            "stream-truncated",
            "stream-malformed",
            "response-too-large",
            "provider-error",
            "provider-rejected",
        ];
        let general = stream_failure_words("nao-existe");
        for code in codes {
            assert_ne!(
                stream_failure_words(code),
                general,
                "{code} has its own words"
            );
            assert!(
                stream_failure_words(code).contains("Nada foi salvo"),
                "{code} says nothing was saved"
            );
        }
    }

    #[test]
    fn an_answer_collects_its_deltas_in_order_until_done_and_then_can_be_saved() {
        let mut session = Session::default();
        let run = session.start("/projetos/app");
        assert_eq!(*session.phase(), Phase::Asking);
        session.frame(run, delta("ol"));
        session.frame(run, delta("á"));
        assert_eq!(*session.phase(), Phase::Answering);
        assert_eq!(session.answer(), "olá");
        assert!(
            !session.can_save(),
            "an answer that is still arriving is not saved"
        );
        session.frame(run, Frame::Done);
        assert_eq!(*session.phase(), Phase::Done);
        assert!(session.can_save());
        session.mark_saved(run);
        assert!(!session.can_save(), "an answer is saved once");
        assert!(session.saved());
    }

    #[test]
    fn a_late_event_of_an_older_question_cannot_change_the_newer_answer() {
        let mut session = Session::default();
        let first = session.start("/projetos/app");
        session.frame(first, delta("antiga"));
        session.frame(first, Frame::Done);
        let second = session.start("/projetos/app");
        session.frame(first, delta(" atrasada"));
        session.frame(first, Frame::Done);
        assert_eq!(session.answer(), "");
        assert_eq!(
            *session.phase(),
            Phase::Asking,
            "the older run cannot finish the newer one"
        );
        session.frame(second, delta("nova"));
        assert_eq!(session.answer(), "nova");
    }

    #[test]
    fn a_cancel_keeps_the_text_so_far_and_ignores_what_comes_after_it() {
        let mut session = Session::default();
        let run = session.start("/projetos/app");
        session.frame(run, delta("parcial"));
        session.cancel();
        assert_eq!(*session.phase(), Phase::Cancelled);
        assert_eq!(session.answer(), "parcial");
        session.frame(run, delta(" depois"));
        session.frame(run, Frame::Done);
        assert_eq!(
            session.answer(),
            "parcial",
            "nothing is added after the cancel"
        );
        assert!(!session.can_save(), "a cancelled answer is not saved");
        assert!(!session.busy());
    }

    #[test]
    fn a_cancel_with_nothing_running_changes_nothing() {
        let mut session = Session::default();
        session.cancel();
        assert_eq!(*session.phase(), Phase::Idle);
        let run = session.start("/projetos/app");
        session.frame(run, Frame::Done);
        session.cancel();
        assert_eq!(*session.phase(), Phase::Done);
    }

    #[test]
    fn a_failed_answer_keeps_its_text_but_cannot_be_saved() {
        let mut session = Session::default();
        let run = session.start("/projetos/app");
        session.frame(run, delta("parcial"));
        session.frame(
            run,
            Frame::Failed {
                code: "stream-truncated".to_string(),
            },
        );
        assert_eq!(
            *session.phase(),
            Phase::Failed(stream_failure_words("stream-truncated").to_string())
        );
        assert_eq!(session.answer(), "parcial");
        assert!(!session.can_save());
    }

    #[test]
    fn a_refusal_of_the_shell_is_shown_for_the_run_it_refused() {
        let mut session = Session::default();
        let run = session.start("/projetos/app");
        session.refused(run, "Este projeto ainda não tem consentimento.".to_string());
        assert_eq!(
            *session.phase(),
            Phase::Failed("Este projeto ainda não tem consentimento.".to_string())
        );
        session.refused(run + 1, "outra".to_string());
        assert_eq!(
            *session.phase(),
            Phase::Failed("Este projeto ainda não tem consentimento.".to_string()),
            "a refusal of another run is ignored"
        );
    }

    #[test]
    fn the_phase_line_is_empty_for_a_failure_and_says_what_is_saved_for_a_cancel() {
        assert_eq!(phase_words(&Phase::Failed("x".to_string())), "");
        assert_eq!(phase_words(&Phase::Idle), "");
        assert!(phase_words(&Phase::Cancelled).contains("não é salva"));
        assert!(phase_words(&Phase::Done).contains("salvá-la"));
    }

    #[test]
    fn an_empty_answer_is_not_saved_even_when_it_is_complete() {
        let mut session = Session::default();
        let run = session.start("/projetos/app");
        session.frame(run, Frame::Done);
        assert!(!session.can_save());
    }

    #[test]
    fn the_save_keeps_the_project_the_answer_came_from() {
        let mut session = Session::default();
        session.start("/projetos/app");
        assert_eq!(session.workspace(), "/projetos/app");
    }
}
