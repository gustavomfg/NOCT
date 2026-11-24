//! Revisar › Sugestões: the analysis of the current project and its suggestions.
//!
//! "Analisar projeto" asks the shell for an analysis (`suggestions_analyze`), which runs with the chat's checks and context
//! and stores only a valid reply; the tab sees a character count while it runs, then the project's list. The list is read
//! from the shell (`suggestions_list`) when the tab opens and when the project changes, so saved suggestions come back when
//! the project is opened again. Aceitar, Adiar and Recusar only change the status (`suggestions_status`). "Gerar alteração"
//! on an accepted suggestion asks the shell for pending changes, which wait for approval in Construir.
//!
//! Everything the model wrote is shown as text: the affected files are plain text, never links, and the reasoning and the
//! proposed changes go through the safe Markdown reader (`markdown.rs`).

use leptos::prelude::*;
use noct_core::suggestion_analysis::AnalysisFocus;
use noct_core::suggestions::{
    Suggestion, SuggestionCategory, SuggestionLevel, SuggestionSeverity, SuggestionStatus,
};
use serde::Deserialize;
use serde_json::json;

use crate::chat::{self, ProposedChange};
use crate::ipc;
use crate::markdown;
use crate::select::{self, Choice};
use crate::sidebar::{self, Shared};

const LIST: &str = "suggestions_list";
const ANALYZE: &str = "suggestions_analyze";
const STATUS: &str = "suggestions_status";
const GENERATE: &str = "suggestion_generate_change";

/// One event of an analysis or of a generated change, as the shell sends it.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
enum Event {
    Progress {
        characters: usize,
    },
    Saved {
        suggestions: Vec<Suggestion>,
        discarded: usize,
    },
    Proposed {
        changes: Vec<ProposedChange>,
        refused: usize,
    },
    Failed {
        code: String,
    },
}

#[derive(Debug, Clone, PartialEq)]
enum ListView {
    NoProject,
    Loading,
    Ready(Vec<Suggestion>),
    Failed(String),
}

#[derive(Debug, Clone, PartialEq)]
enum RunView {
    Idle,
    /// The analysis (`None`) or the change of a suggestion (`Some(id)`) is running.
    Running {
        target: Option<String>,
        characters: usize,
    },
    Note(String),
    Failed(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Filter {
    All,
    New,
    Accepted,
    Deferred,
    Rejected,
}

impl Filter {
    const ALL: [Filter; 5] = [
        Self::All,
        Self::New,
        Self::Accepted,
        Self::Deferred,
        Self::Rejected,
    ];

    fn value(self) -> &'static str {
        match self {
            Self::All => "all",
            Self::New => "new",
            Self::Accepted => "accepted",
            Self::Deferred => "deferred",
            Self::Rejected => "rejected",
        }
    }

    fn choices() -> Vec<Choice> {
        Self::ALL
            .iter()
            .map(|filter| Choice {
                value: filter.value(),
                label: filter.label(),
            })
            .collect()
    }

    fn label(self) -> &'static str {
        match self {
            Self::All => "Todas",
            Self::New => "Novas",
            Self::Accepted => "Aceitas",
            Self::Deferred => "Adiadas",
            Self::Rejected => "Recusadas",
        }
    }

    fn keeps(self, status: SuggestionStatus) -> bool {
        match self {
            Self::All => true,
            Self::New => matches!(status, SuggestionStatus::New | SuggestionStatus::InAnalysis),
            Self::Accepted => status == SuggestionStatus::Accepted,
            Self::Deferred => status == SuggestionStatus::Deferred,
            Self::Rejected => status == SuggestionStatus::Rejected,
        }
    }
}

/// The focus of an analysis: the value the select keeps, and the words shown.
const FOCI: [(AnalysisFocus, &str, &str); 5] = [
    (AnalysisFocus::Free, "free", "Livre"),
    (AnalysisFocus::Security, "security", "Segurança"),
    (AnalysisFocus::Quality, "quality", "Qualidade"),
    (AnalysisFocus::Performance, "performance", "Desempenho"),
    (AnalysisFocus::Testing, "testing", "Testes"),
];

/// The tab's state. It lives with the window, so a running analysis survives a switch to the conversation tab.
#[derive(Clone, Copy)]
pub struct SuggestionsState {
    shared: Shared,
    list: RwSignal<ListView>,
    /// The project the list belongs to.
    listed: RwSignal<Option<String>>,
    focus: RwSignal<AnalysisFocus>,
    filter: RwSignal<Filter>,
    run: RwSignal<RunView>,
    /// The project of the running call, and its number; an event of an older call is ignored.
    run_of: RwSignal<(Option<String>, u64)>,
    /// The suggestion whose status is being changed.
    deciding: RwSignal<Option<String>>,
    action_error: RwSignal<Option<String>>,
}

impl SuggestionsState {
    pub fn new(shared: Shared) -> Self {
        Self {
            shared,
            list: RwSignal::new(ListView::NoProject),
            listed: RwSignal::new(None),
            focus: RwSignal::new(AnalysisFocus::Free),
            filter: RwSignal::new(Filter::All),
            run: RwSignal::new(RunView::Idle),
            run_of: RwSignal::new((None, 0)),
            deciding: RwSignal::new(None),
            action_error: RwSignal::new(None),
        }
    }

    /// Keeps the list of the current project read, so the count in the top bar is right before the list is opened.
    pub fn watch(self) {
        Effect::new(move |_| {
            self.shared.current.track();
            self.load(false);
        });
        // Findings saved from the chat: the list is read again.
        Effect::new(move |previous: Option<u64>| {
            let count = self.shared.suggestions_version.get();
            if previous.is_some_and(|previous| previous != count) {
                self.load(true);
            }
            count
        });
    }

    /// How many suggestions of the current project are still open, or `None` while the list is not read.
    pub fn open_count(self) -> Option<usize> {
        self.list.with(|list| match list {
            ListView::Ready(items) => {
                Some(items.iter().filter(|item| item.status.is_active()).count())
            }
            _ => None,
        })
    }

    fn running(self) -> bool {
        self.run
            .with_untracked(|run| matches!(run, RunView::Running { .. }))
    }

    /// Reads the list of the current project, unless it is already the one on screen.
    fn load(self, force: bool) {
        let current = self.shared.current.get_untracked();
        if !force && current == self.listed.get_untracked() {
            return;
        }
        self.listed.set(current.clone());
        self.action_error.set(None);
        let Some(workspace) = current else {
            self.list.set(ListView::NoProject);
            return;
        };
        self.list.set(ListView::Loading);
        let payload = json!({ "input": { "workspace": workspace } }).to_string();
        ipc::call_with(LIST, &payload, move |outcome| {
            if self.listed.get_untracked().as_deref() != Some(workspace.as_str()) {
                return;
            }
            self.list.set(match ipc::reply(outcome) {
                Ok(text) => match serde_json::from_str::<Vec<Suggestion>>(&text) {
                    Ok(items) => ListView::Ready(items),
                    Err(_) => ListView::Failed(ipc::UNEXPECTED_MESSAGE.to_string()),
                },
                Err(failure) => ListView::Failed(failure.message),
            });
        });
    }

    /// Starts a call on a new channel. `target` is `None` for an analysis, or the suggestion whose change is asked for.
    fn start(self, command: &'static str, input: serde_json::Value, target: Option<String>) {
        let Some(workspace) = self.shared.current.get_untracked() else {
            return;
        };
        if self.running() {
            return;
        }
        let number = self.run_of.get_untracked().1 + 1;
        self.run_of.set((Some(workspace.clone()), number));
        self.run.set(RunView::Running {
            target: target.clone(),
            characters: 0,
        });
        self.action_error.set(None);
        let current = move || self.run_of.get_untracked().1 == number;
        let Some(channel) = ipc::channel(move |text: String| {
            if !current() {
                return;
            }
            let Ok(event) = serde_json::from_str::<Event>(&text) else {
                return;
            };
            self.event(&workspace, event);
        }) else {
            self.run.set(RunView::Failed(
                "O NOCT não conseguiu abrir a resposta.".to_string(),
            ));
            return;
        };
        let payload = json!({ "input": input, "onEvent": channel }).to_string();
        ipc::call_with(command, &payload, move |outcome| {
            if let Err(failure) = ipc::reply(outcome)
                && current()
            {
                self.run.set(RunView::Failed(failure.message));
            }
        });
    }

    fn event(self, workspace: &str, event: Event) {
        match event {
            Event::Progress { characters } => self.run.update(|run| {
                if let RunView::Running {
                    characters: seen, ..
                } = run
                {
                    *seen = characters;
                }
            }),
            Event::Saved {
                suggestions,
                discarded,
            } => {
                let words = match discarded {
                    0 => "Análise concluída.".to_string(),
                    1 => "Análise concluída. 1 sugestão inválida ou repetida foi descartada."
                        .to_string(),
                    n => format!(
                        "Análise concluída. {n} sugestões inválidas ou repetidas foram descartadas."
                    ),
                };
                self.run.set(RunView::Note(words));
                if self.listed.get_untracked().as_deref() == Some(workspace) {
                    self.list.set(ListView::Ready(suggestions));
                }
            }
            Event::Proposed { changes, refused } => {
                self.shared.changes_version.update(|n| *n += 1);
                let mut words = match changes.len() {
                    0 => "O modelo não propôs alteração para os arquivos da sugestão.".to_string(),
                    1 => "1 alteração espera aprovação em Construir.".to_string(),
                    n => format!("{n} alterações esperam aprovação em Construir."),
                };
                if refused > 0 {
                    words.push_str(&format!(" {refused} bloco(s) recusado(s)."));
                }
                self.run.set(RunView::Note(words));
            }
            Event::Failed { code } => self.run.set(RunView::Failed(failure_words(&code))),
        }
    }

    fn analyze(self) {
        let Some(workspace) = self.shared.current.get_untracked() else {
            return;
        };
        let focus = self.focus.get_untracked();
        self.start(
            ANALYZE,
            json!({ "workspace": workspace, "focus": focus }),
            None,
        );
    }

    fn generate(self, suggestion: &Suggestion) {
        self.start(
            GENERATE,
            json!({ "conversationId": suggestion.conversation_id, "suggestionId": suggestion.id }),
            Some(suggestion.id.clone()),
        );
    }

    fn cancel(self) {
        if !self.running() {
            return;
        }
        self.run_of.update(|(_, number)| *number += 1);
        self.run
            .set(RunView::Note("Cancelado. Nada foi salvo.".to_string()));
        ipc::call(chat::CANCEL, |_| {});
    }

    /// Moves a suggestion to `status`. Only the status changes; nothing is written to the project.
    fn decide(self, suggestion: &Suggestion, status: SuggestionStatus) {
        if self.deciding.get_untracked().is_some() {
            return;
        }
        let id = suggestion.id.clone();
        self.deciding.set(Some(id.clone()));
        self.action_error.set(None);
        let payload = json!({ "input": {
            "conversationId": suggestion.conversation_id,
            "suggestionId": id,
            "status": status.as_str(),
        }})
        .to_string();
        ipc::call_with(STATUS, &payload, move |outcome| {
            self.deciding.set(None);
            match ipc::reply(outcome).and_then(|text| {
                serde_json::from_str::<Suggestion>(&text).map_err(|_| ipc::invalid_response())
            }) {
                Ok(updated) => self.list.update(|list| {
                    if let ListView::Ready(items) = list
                        && let Some(item) = items.iter_mut().find(|item| item.id == updated.id)
                    {
                        *item = updated;
                    }
                }),
                Err(failure) => self.action_error.set(Some(failure.message)),
            }
        });
    }
}

fn failure_words(code: &str) -> String {
    match code {
        "analysis-absent" => {
            "A resposta não trouxe o bloco de sugestões. Nada foi salvo.".to_string()
        }
        "analysis-invalid" => {
            "As sugestões vieram inválidas ou incompletas. Nada foi salvo.".to_string()
        }
        other => chat::stream_failure_words(other).to_string(),
    }
}

fn severity_of(severity: SuggestionSeverity) -> (&'static str, &'static str) {
    match severity {
        SuggestionSeverity::Critical => ("sev sev--high", "Crítica"),
        SuggestionSeverity::High => ("sev sev--high", "Alta"),
        SuggestionSeverity::Medium => ("sev sev--mid", "Média"),
        SuggestionSeverity::Low => ("sev sev--low", "Baixa"),
        SuggestionSeverity::Info => ("sev sev--low", "Informativa"),
    }
}

fn category_word(category: SuggestionCategory) -> &'static str {
    match category {
        SuggestionCategory::Architecture => "Arquitetura",
        SuggestionCategory::Security => "Segurança",
        SuggestionCategory::Performance => "Desempenho",
        SuggestionCategory::Bug => "Defeito",
        SuggestionCategory::Cleanup => "Limpeza",
        SuggestionCategory::Testing => "Testes",
        SuggestionCategory::Documentation => "Documentação",
        SuggestionCategory::Dependency => "Dependência",
        SuggestionCategory::Accessibility => "Acessibilidade",
    }
}

fn level_word(level: SuggestionLevel) -> &'static str {
    match level {
        SuggestionLevel::Low => "baixa",
        SuggestionLevel::Medium => "média",
        SuggestionLevel::High => "alta",
    }
}

fn status_word(status: SuggestionStatus) -> &'static str {
    match status {
        SuggestionStatus::New => "Nova",
        SuggestionStatus::InAnalysis => "Em análise",
        SuggestionStatus::Accepted => "Aceita",
        SuggestionStatus::Rejected => "Recusada",
        SuggestionStatus::Resolved => "Resolvida",
        SuggestionStatus::Deferred => "Adiada",
        SuggestionStatus::Invalid => "Inválida",
    }
}

/// The list of Sugestões: a head with the focus and "Analisar projeto", a status filter once there is a suggestion, and one
/// row per suggestion, in the light list of Ajustes.
pub fn panel(state: SuggestionsState) -> impl IntoView {
    let has_project = move || state.shared.current.with(Option::is_some);
    let has_items = move || {
        state
            .list
            .with(|list| matches!(list, ListView::Ready(items) if !items.is_empty()))
    };
    let running = move || state.run.with(|run| matches!(run, RunView::Running { .. }));
    let analysis_running = move || {
        state
            .run
            .with(|run| matches!(run, RunView::Running { target: None, .. }))
    };
    view! {
        <div class="sugg" data-testid="suggestions">
            <div class="sugg__head">
                <div class="sugg__head-txt">
                    <h1 class="h1">"Sugestões"</h1>
                    <p class="lead">
                        {move || format!(
                            "O modelo lê os arquivos de texto do topo de {}, como no chat, e propõe até 10 sugestões. Nada é alterado.",
                            sidebar::current_name(state.shared).unwrap_or_else(|| "um projeto".to_string())
                        )}
                    </p>
                </div>
                <div class="sugg__head-actions">
                    <div class="pick">
                        <label class="pick__label" for="sugg-focus">"Foco:"</label>
                        {select::select(
                            "sugg-focus",
                            FOCI.iter().map(|(_, value, label)| Choice { value, label }).collect(),
                            Signal::derive(move || {
                                let focus = state.focus.get();
                                FOCI.iter().find(|(item, _, _)| *item == focus).map_or("free", |(_, value, _)| value).to_string()
                            }),
                            "",
                            Signal::derive(move || running() || !has_project()),
                            move |value| {
                                if let Some((focus, _, _)) = FOCI.iter().find(|(_, item, _)| *item == value) {
                                    state.focus.set(*focus);
                                }
                            },
                        )}
                    </div>
                    {move || {
                        if analysis_running() {
                            view! {
                                <button class="btn" type="button" on:click=move |_| state.cancel()>"Cancelar"</button>
                            }
                            .into_any()
                        } else if has_items() {
                            analyze_button(state, has_project, running).into_any()
                        } else {
                            ().into_any()
                        }
                    }}
                </div>
            </div>
            {move || match state.run.get() {
                RunView::Idle => ().into_any(),
                RunView::Running { target, characters } => {
                    let what = if target.is_some() { "Gerando a alteração…" } else { "Analisando…" };
                    view! {
                        <p class="note" role="status">{format!("{what} {characters} caracteres recebidos.")}</p>
                    }
                    .into_any()
                }
                RunView::Note(words) => view! { <p class="note note--ok" role="status">{words}</p> }.into_any(),
                RunView::Failed(words) => view! { <p class="note" role="alert">{words}</p> }.into_any(),
            }}
            {move || state.action_error.get().map(|message| view! { <p class="note" role="alert">{message}</p> })}
            {move || has_items().then(|| view! {
                <div class="pick sugg__filter">
                    <label class="pick__label" for="sugg-status">"Status:"</label>
                    {select::select(
                        "sugg-status",
                        Filter::choices(),
                        Signal::derive(move || state.filter.get().value().to_string()),
                        "",
                        Signal::derive(|| false),
                        move |value| {
                            if let Some(filter) = Filter::ALL.into_iter().find(|filter| filter.value() == value) {
                                state.filter.set(filter);
                            }
                        },
                    )}
                </div>
            })}
            {move || list_view(state, has_project, running)}
        </div>
    }
}

fn analyze_button(
    state: SuggestionsState,
    has_project: impl Fn() -> bool + Copy + Send + Sync + 'static,
    running: impl Fn() -> bool + Copy + Send + Sync + 'static,
) -> impl IntoView {
    view! {
        <button
            class="btn btn--primary"
            type="button"
            data-testid="analyze"
            disabled=move || !has_project() || running()
            on:click=move |_| state.analyze()
        >
            "Analisar projeto"
        </button>
    }
}

fn list_view(
    state: SuggestionsState,
    has_project: impl Fn() -> bool + Copy + Send + Sync + 'static,
    running: impl Fn() -> bool + Copy + Send + Sync + 'static,
) -> AnyView {
    match state.list.get() {
        ListView::NoProject => view! {
            <div class="empty">
                <h2 class="empty__title">"Nenhum projeto escolhido"</h2>
                <p class="empty__lead">"Escolha um projeto na barra lateral para ver as sugestões dele."</p>
            </div>
        }
        .into_any(),
        ListView::Loading => view! { <p class="note" role="status">"Carregando sugestões…"</p> }.into_any(),
        ListView::Failed(message) => view! {
            <div class="sugg__error">
                <p class="note" role="alert">{format!("Não foi possível carregar as sugestões. {message}")}</p>
                <button class="btn btn--sm" type="button" on:click=move |_| state.load(true)>"Tentar de novo"</button>
            </div>
        }
        .into_any(),
        ListView::Ready(items) if items.is_empty() => view! {
            <div class="empty">
                <h2 class="empty__title">"Nenhuma sugestão ainda"</h2>
                <p class="empty__lead">"Peça uma análise do projeto. As sugestões ficam salvas e voltam quando você reabrir o projeto."</p>
                {analyze_button(state, has_project, running)}
            </div>
        }
        .into_any(),
        ListView::Ready(items) => {
            let filter = state.filter.get();
            let shown: Vec<Suggestion> = items
                .into_iter()
                .filter(|item| filter.keeps(item.status))
                .collect();
            if shown.is_empty() {
                return view! { <p class="note">"Nenhuma sugestão com este status."</p> }.into_any();
            }
            view! {
                <div class="list">
                    {shown.into_iter().map(|suggestion| row(state, suggestion)).collect_view()}
                </div>
            }
            .into_any()
        }
    }
}

/// One suggestion: a row with its severity, title and status, which opens its details and actions.
fn row(state: SuggestionsState, suggestion: Suggestion) -> impl IntoView {
    let (sev_class, sev_word) = severity_of(suggestion.severity);
    let expanded = RwSignal::new(false);
    let details_id = format!("sugg-more-{}", suggestion.id);
    let status = suggestion.status;
    let id = suggestion.id.clone();
    let busy = move || {
        state
            .deciding
            .with(|deciding| deciding.as_deref() == Some(id.as_str()))
            || state.run.with(|run| matches!(run, RunView::Running { .. }))
    };
    let meta = format!(
        "{} · complexidade {} · risco {} · confiança {}%",
        category_word(suggestion.category),
        level_word(suggestion.complexity),
        level_word(suggestion.risk),
        suggestion.confidence
    );
    let title = suggestion.title.clone();
    view! {
        <div class="item sugg-row" data-testid="suggestion">
            <div class="item__txt">
                <div class="item__name">
                    <span class=sev_class>{sev_word}</span>
                    " "
                    {title}
                </div>
                <div class="item__desc">{meta}</div>
            </div>
            <div class="item__control">
                <span class="tag">{status_word(status)}</span>
                <button
                    class="btn btn--sm btn--ghost"
                    type="button"
                    aria-expanded=move || expanded.get().to_string()
                    aria-controls=details_id.clone()
                    on:click=move |_| expanded.update(|open| *open = !*open)
                >
                    {move || if expanded.get() { "Fechar" } else { "Detalhes" }}
                </button>
            </div>
            <div id=details_id class="sugg-row__more">
                {move || expanded.get().then(|| details(state, suggestion.clone(), busy.clone()))}
            </div>
        </div>
    }
}

fn details(
    state: SuggestionsState,
    suggestion: Suggestion,
    busy: impl Fn() -> bool + Clone + Send + Sync + 'static,
) -> impl IntoView {
    let status = suggestion.status;
    let button = |label: &'static str, class: &'static str, next: SuggestionStatus| {
        let suggestion = suggestion.clone();
        let busy = busy.clone();
        view! {
            <button class=class type="button" disabled=busy on:click=move |_| state.decide(&suggestion, next)>
                {label}
            </button>
        }
        .into_any()
    };
    let mut actions = Vec::new();
    if status == SuggestionStatus::Accepted {
        let target = suggestion.clone();
        let busy = busy.clone();
        actions.push(
            view! {
                <button class="btn btn--sm btn--primary" type="button" disabled=busy on:click=move |_| state.generate(&target)>
                    "Gerar alteração"
                </button>
            }
            .into_any(),
        );
    }
    if matches!(
        status,
        SuggestionStatus::New | SuggestionStatus::InAnalysis | SuggestionStatus::Deferred
    ) {
        actions.push(button(
            "Aceitar",
            "btn btn--sm btn--primary",
            SuggestionStatus::Accepted,
        ));
    }
    if matches!(
        status,
        SuggestionStatus::New | SuggestionStatus::InAnalysis | SuggestionStatus::Accepted
    ) {
        actions.push(button("Adiar", "btn btn--sm", SuggestionStatus::Deferred));
    }
    if status.is_active() {
        actions.push(button(
            "Recusar",
            "btn btn--sm btn--ghost",
            SuggestionStatus::Rejected,
        ));
    }
    let proposed = suggestion.proposed_changes.clone();
    view! {
        <p class="body">{suggestion.description.clone()}</p>
        {(!suggestion.expected_benefits.is_empty()).then(|| view! {
            <div>
                <div class="eyebrow">"Benefícios esperados"</div>
                <ul class="sugg-row__list">
                    {suggestion.expected_benefits.iter().map(|benefit| view! { <li>{benefit.clone()}</li> }).collect_view()}
                </ul>
            </div>
        })}
        {(!suggestion.affected_files.is_empty()).then(|| view! {
            <div>
                <div class="eyebrow">"Arquivos afetados"</div>
                <ul class="sugg-row__list">
                    {suggestion.affected_files.iter().map(|path| view! { <li class="path">{path.clone()}</li> }).collect_view()}
                </ul>
            </div>
        })}
        <div class="eyebrow">"Raciocínio"</div>
        <div class="md">{markdown::parse(&suggestion.reasoning).into_iter().map(markdown::view).collect_view()}</div>
        {(!proposed.trim().is_empty()).then(|| view! {
            <div class="eyebrow">"Mudanças propostas"</div>
            <div class="md">{markdown::parse(&proposed).into_iter().map(markdown::view).collect_view()}</div>
        })}
        {(!actions.is_empty()).then(|| view! { <div class="sugg-row__actions">{actions}</div> })}
    }
}
