//! The conversation of the review mode: the questions and the answers of this window, the composer under them, and the save
//! of an answer into a conversation of its project, which the person asks for. The logic is in `chat.rs`; this module draws
//! it and calls the shell's commands.
//!
//! The question goes to the current project, which the sidebar chooses. A question is sent alone: the shell receives the
//! question and the project's files, not the earlier turns on screen. The state of each turn is its own signal, so the
//! stream of one answer updates only that answer.

use leptos::prelude::*;
use leptos::web_sys::KeyboardEvent;
use noct_core::limits::provider_chat::MAXIMUM_QUESTION_CHARACTERS;

use crate::chat::{self, CreatedConversation, Phase, Session};
use crate::dom;
use crate::ipc;
use crate::markdown;
use crate::model_picker;
use crate::provider::{self, ConsentStatus, ProviderView};
use crate::sidebar::{self, Shared};
use crate::workspaces::WorkspacesView;

/// One question of the window and its answer. The session is the answer's state; the turn keeps the question as it was
/// sent, and the note of the copy button.
#[derive(Clone)]
pub struct Turn {
    pub id: u64,
    pub question: String,
    pub session: RwSignal<Session>,
    pub copy_note: RwSignal<Option<&'static str>>,
}

/// The chat's state. Every field is a signal, so the value is copied into each closure.
#[derive(Clone, Copy)]
pub struct ChatState {
    shared: Shared,
    question: RwSignal<String>,
    turns: RwSignal<Vec<Turn>>,
    next_id: RwSignal<u64>,
    /// The conversation of the last save, with the project it belongs to. A save for that project reuses it.
    conversation: RwSignal<Option<(String, String)>>,
    save_busy: RwSignal<bool>,
    save_error: RwSignal<Option<String>>,
    /// The owner of the window. The signals of each question belong to it, not to the button that sent the question: that
    /// button is drawn again when the answer starts, and a signal that belonged to it would be dropped with it.
    owner: StoredValue<Option<Owner>, LocalStorage>,
    /// The consent of the current project, as the shell last said it, with the project it is about.
    consent: RwSignal<Option<(String, ProviderView<ConsentStatus>)>>,
    /// The project whose question waits for the permission card. The card shows only for the current project.
    awaiting: RwSignal<Option<String>>,
    consent_busy: RwSignal<bool>,
    consent_error: RwSignal<Option<String>>,
}

impl ChatState {
    pub fn new(shared: Shared) -> Self {
        Self {
            shared,
            question: RwSignal::new(String::new()),
            turns: RwSignal::new(Vec::new()),
            next_id: RwSignal::new(0),
            conversation: RwSignal::new(None),
            save_busy: RwSignal::new(false),
            save_error: RwSignal::new(None),
            owner: StoredValue::new_local(Owner::current()),
            consent: RwSignal::new(None),
            awaiting: RwSignal::new(None),
            consent_busy: RwSignal::new(false),
            consent_error: RwSignal::new(None),
        }
    }

    /// Makes a signal that lives as long as the window.
    fn in_window<T>(self, make: impl FnOnce() -> T) -> T {
        // The owner is copied out before it is used, so the arena is not borrowed while the signal is made.
        match self.owner.get_value() {
            Some(owner) => owner.with(make),
            None => make(),
        }
    }

    /// Reads the switch. Until the shell answers, nothing is sent.
    pub fn read_status(self) {
        let chat = self.shared.chat;
        ipc::call(chat::STATUS, move |outcome| {
            chat.set(provider::settle(outcome));
        });
    }

    fn enabled(self) -> bool {
        self.chat_gate() == Gate::Ready
    }

    /// The chat switch, as the shell last said it.
    fn chat_gate(self) -> Gate {
        self.shared.chat.with(|view| match view {
            ProviderView::Ready(status) if status.enabled => Gate::Ready,
            ProviderView::Ready(_) => Gate::Missing,
            ProviderView::Loading => Gate::Loading,
            ProviderView::Failed(_) => Gate::Failed,
        })
    }

    /// The default model of the current project, as the shell last said it.
    fn model_gate(self) -> Gate {
        self.shared.bindings.with(|bindings| match bindings {
            Some(ProviderView::Ready(Some(bindings))) if bindings.default_binding.is_some() => {
                Gate::Ready
            }
            Some(ProviderView::Ready(_)) => Gate::Missing,
            Some(ProviderView::Failed(_)) => Gate::Failed,
            Some(ProviderView::Loading) | None => Gate::Loading,
        })
    }

    /// Whether the last question is still running.
    fn busy(self) -> bool {
        self.turns.with(|turns| {
            turns
                .last()
                .is_some_and(|turn| turn.session.with(Session::busy))
        })
    }

    /// Whether the composer may send: the chat is on, a project with a model is chosen, the key is saved, the question has
    /// text, and no answer is running.
    fn can_send(self) -> bool {
        self.enabled()
            && self
                .shared
                .current
                .get()
                .is_some_and(|path| self.may_send(&path))
            && chat::question_is_sendable(&self.question.get())
            && !self.busy()
    }

    /// The OpenRouter key, as the shell last said it. The chat sends only when a key is configured.
    fn key_gate(self) -> Gate {
        self.shared.key.with(|key| match key {
            ProviderView::Ready(status) if status.configured => Gate::Ready,
            ProviderView::Ready(_) => Gate::Missing,
            ProviderView::Loading => Gate::Loading,
            ProviderView::Failed(_) => Gate::Failed,
        })
    }

    /// Reads the permission of the current project when the project changes, and again when Ajustes close, so a permission
    /// given there is seen here.
    pub fn watch_consent(self) {
        let shared = self.shared;
        Effect::new(move |_| {
            let path = shared.current.get();
            let _ = shared.settings_open.get();
            self.awaiting.set(None);
            let Some(path) = path else {
                self.consent.set(None);
                return;
            };
            self.consent
                .set(Some((path.clone(), ProviderView::Loading)));
            let asked = path.clone();
            ipc::call_with(
                provider::CONSENT_STATUS,
                &provider::consent_payload(&path),
                move |outcome| {
                    if shared.current.get_untracked().as_deref() == Some(asked.as_str()) {
                        self.consent.set(Some((asked, provider::settle(outcome))));
                    }
                },
            );
        });
    }

    /// The permission of a project, as the shell last said it. A project whose reply has not come, or whose reply is for
    /// another project, is loading.
    fn permission_of(self, path: &str) -> Gate {
        self.consent.with(|consent| match consent {
            Some((known, ProviderView::Ready(status))) if known.as_str() == path => {
                if status.granted {
                    Gate::Ready
                } else {
                    Gate::Missing
                }
            }
            Some((known, ProviderView::Failed(_))) if known.as_str() == path => Gate::Failed,
            _ => Gate::Loading,
        })
    }

    /// Whether a question may be sent to this project now: the key is configured, and the project's permission is known,
    /// granted or not, so that a missing one can be asked for.
    fn may_send(self, path: &str) -> bool {
        self.key_gate() == Gate::Ready
            && self.model_gate() == Gate::Ready
            && matches!(self.permission_of(path), Gate::Ready | Gate::Missing)
    }

    /// Turns the chat on, from the line over the composer. The shell's reply is what the window then holds.
    fn turn_on(self) {
        let chat = self.shared.chat;
        ipc::call_with(
            chat::SET_ENABLED,
            &chat::switch_payload(true),
            move |outcome| {
                chat.set(provider::settle(outcome));
            },
        );
    }

    /// Gives the permission of a project, as the card asks. The shell's reply is what the chat then holds.
    fn grant(self, path: String) {
        self.consent_busy.set(true);
        self.consent_error.set(None);
        let asked = path.clone();
        ipc::call_with(
            provider::CONSENT_GRANT,
            &provider::consent_payload(&path),
            move |outcome| {
                self.consent_busy.set(false);
                match provider::settle::<ConsentStatus>(outcome) {
                    ProviderView::Ready(status) => {
                        if self.shared.current.get_untracked().as_deref() == Some(asked.as_str()) {
                            self.consent.set(Some((asked, ProviderView::Ready(status))));
                        }
                    }
                    ProviderView::Failed(failure) => self.consent_error.set(Some(failure.message)),
                    ProviderView::Loading => {}
                }
            },
        );
    }

    /// Sends the question about the current project. The turn is on screen before the call, so the first event has a place
    /// to go. A refusal of the shell is shown in that turn only.
    fn ask(self) {
        let Some(workspace) = self.shared.current.get_untracked() else {
            return;
        };
        let question = self.question.get_untracked();
        if !self.enabled() || !chat::question_is_sendable(&question) || self.busy() {
            return;
        }
        if self.key_gate() != Gate::Ready || self.model_gate() != Gate::Ready {
            return;
        }
        match self.permission_of(&workspace) {
            Gate::Ready => {}
            Gate::Missing => {
                // Nothing is sent: the question stays in the composer, and the card asks for the permission.
                self.awaiting.set(Some(workspace));
                return;
            }
            Gate::Loading | Gate::Failed => return,
        }
        self.awaiting.set(None);
        let question = question.trim().to_string();
        let session = self.in_window(|| RwSignal::new(Session::default()));
        let copy_note = self.in_window(|| RwSignal::new(None));
        let mut run = 0;
        session.update(|session| run = session.start(&workspace));
        let id = self.next_id.get_untracked();
        self.next_id.set(id + 1);
        self.turns.update(|turns| {
            turns.push(Turn {
                id,
                question: question.clone(),
                session,
                copy_note,
            })
        });
        self.question.set(String::new());
        self.save_error.set(None);

        let shared = self.shared;
        let Some(channel) = ipc::channel(move |text: String| {
            if let Some(frame) = chat::parse_frame(&text) {
                if matches!(frame, chat::Frame::Proposed { .. }) {
                    shared.changes_version.update(|n| *n += 1);
                }
                session.update(|session| session.frame(run, frame));
            }
        }) else {
            session.update(|session| {
                session.refused(run, "O NOCT não conseguiu abrir a resposta.".to_string())
            });
            return;
        };
        let payload = chat::send_payload(&workspace, &question, &channel);
        ipc::call_with(chat::SEND, &payload, move |outcome| {
            if let Err(failure) = ipc::reply(outcome) {
                session.update(|session| session.refused(run, failure.message));
            }
        });
    }

    /// Stops the answer that is running. What arrived stays on screen, and nothing more is shown after the stop.
    fn cancel(self) {
        self.turns.with_untracked(|turns| {
            if let Some(turn) = turns.last()
                && turn.session.with_untracked(Session::busy)
            {
                turn.session.update(|session| session.cancel());
            }
        });
        ipc::call(chat::CANCEL, |_| {});
    }

    /// Starts a new conversation: the turns on screen are cleared, and the next save creates a conversation of its own. A
    /// question still running is stopped first, so nothing arrives into the new conversation.
    pub fn new_conversation(self) {
        if self.busy() {
            self.cancel();
        }
        self.turns.set(Vec::new());
        self.conversation.set(None);
        self.question.set(String::new());
        self.save_error.set(None);
    }

    /// Keeps one answer in a conversation of its project. The conversation is created the first time, and reused after.
    fn save(self, answer: RwSignal<Session>) {
        let (run, workspace, content) = answer.with_untracked(|session| {
            (
                session.run(),
                session.workspace().to_string(),
                session.answer().to_string(),
            )
        });
        if !answer.with_untracked(Session::can_save) || self.save_busy.get_untracked() {
            return;
        }
        self.save_busy.set(true);
        self.save_error.set(None);
        let known = self
            .conversation
            .get_untracked()
            .filter(|(known, _)| *known == workspace)
            .map(|(_, id)| id);
        match known {
            Some(id) => self.write_answer(answer, run, id, content),
            None => ipc::call_with(
                chat::CONVERSATION_CREATE,
                &chat::conversation_payload(&workspace),
                move |outcome| match ipc::reply(outcome) {
                    Ok(text) => match serde_json::from_str::<CreatedConversation>(&text) {
                        Ok(created) => {
                            self.conversation.set(Some((workspace, created.id.clone())));
                            self.write_answer(answer, run, created.id, content);
                        }
                        Err(_) => self.fail_save(ipc::invalid_response().message),
                    },
                    Err(failure) => self.fail_save(failure.message),
                },
            ),
        }
    }

    fn write_answer(
        self,
        session: RwSignal<Session>,
        run: u64,
        conversation: String,
        content: String,
    ) {
        ipc::call_with(
            chat::SAVE_ASSISTANT,
            &chat::save_payload(&conversation, &content),
            move |outcome| {
                self.save_busy.set(false);
                match ipc::reply(outcome) {
                    Ok(_) => session.update(|session| session.mark_saved(run)),
                    Err(failure) => {
                        // A conversation that was removed is created again on the next save.
                        if failure.code == "unknown-conversation" {
                            self.conversation.set(None);
                        }
                        self.save_error.set(Some(failure.message));
                    }
                }
            },
        );
    }

    fn fail_save(self, message: String) {
        self.save_busy.set(false);
        self.save_error.set(Some(message));
    }

    /// The model of the current project, as the shell saved it, or the words for its absence.
    fn model_label(self) -> String {
        self.shared.bindings.with(|bindings| match bindings {
            Some(ProviderView::Ready(Some(bindings))) => bindings
                .default_binding
                .as_ref()
                .map(|model| model.model_id.clone())
                .unwrap_or_else(|| "Sem modelo".to_string()),
            _ => "Sem modelo".to_string(),
        })
    }
}

/// The conversation: the switch, then each question and its answer, in order.
pub fn thread(state: ChatState) -> impl IntoView {
    // The conversation follows its end while the person is at it: a new question and the permission card scroll it into
    // view, and an answer that grows keeps it there. A person who scrolled up to read is left where they are, also when
    // the answer ends.
    let height = StoredValue::new(0.0_f64);
    Effect::new(move |previous: Option<usize>| {
        let count = state.turns.with(Vec::len);
        let _ = state.turns.with(|turns| {
            turns
                .last()
                .map(|turn| turn.session.with(|session| session.answer().len()))
        });
        let awaiting = state.awaiting.with(Option::is_some);
        let force = previous != Some(count) || awaiting;
        // The blocks of the answer are drawn by their own effects, after this one: the heights are read in the next frame.
        dom::next_frame(move || {
            if let Some(previous) = height.try_get_value() {
                let _ = height.try_set_value(dom::follow_end(".main", previous, force));
            }
        });
        count
    });
    view! {
        <section class="thread" aria-label="Conversa" data-testid="chat">
            {move || empty_state(state)}
            <For
                each=move || state.turns.get()
                key=|turn| turn.id
                children=move |turn| turn_view(state, turn)
            />
            {consent_card(state)}
            {move || {
                state
                    .save_error
                    .get()
                    .map(|message| {
                        view! {
                            <p class="note" role="alert" data-testid="chat-save-error">{message}</p>
                        }
                    })
            }}
        </section>
    }
}

/// One question and its answer: the question on the right, then the answer with its model, its text, and the tools that
/// copy it or keep it in a conversation.
fn turn_view(state: ChatState, turn: Turn) -> impl IntoView {
    let session = turn.session;
    let copy_note = turn.copy_note;
    let phase = move || session.with(|session| session.phase().clone());
    let answer = move || session.with(|session| session.answer().to_string());
    // The answer as Markdown blocks, each keyed by its position and content, so a streaming answer redraws only the
    // block that grows. A proposal block is shown as its one line, as before.
    let blocks = Memo::new(move |_| {
        session.with(|session| {
            markdown::keyed(&noct_core::proposal_block::shown_answer(session.answer()))
        })
    });
    // Only whether there is text decides that the answer area exists, so a new piece does not draw it again.
    let has_text = Memo::new(move |_| !blocks.with(Vec::is_empty));
    let copy = move |_| {
        let text = session.with_untracked(|session| session.answer().to_string());
        dom::copy_text(&text, move |copied| {
            copy_note.set(Some(if copied {
                "Copiado."
            } else {
                "Não foi possível copiar."
            }));
        });
    };
    view! {
        <div class="msg-user">{turn.question.clone()}</div>
        <div class="msg-ai">
            <div class="msg-ai__head">
                <span class="avatar" aria-hidden="true">"N"</span>
                <span>{move || state.model_label()}</span>
                {move || match phase() {
                    Phase::Asking => " · enviando",
                    _ => "",
                }}
            </div>
            {move || {
                has_text.get().then(|| {
                    view! {
                        <div
                            class=move || {
                                if matches!(phase(), Phase::Answering) {
                                    "msg-ai__body chat-answer md is-streaming"
                                } else {
                                    "msg-ai__body chat-answer md"
                                }
                            }
                            data-testid="chat-answer"
                        >
                            <For
                                each=move || blocks.get()
                                key=|(key, _)| *key
                                children=|(_, node)| markdown::view(node)
                            />
                        </div>
                    }
                })
            }}
            {move || match phase() {
                Phase::Failed(message) => view! {
                    <p class="note" role="alert" data-testid="chat-error">{message}</p>
                }
                .into_any(),
                _ => ().into_any(),
            }}
            {move || {
                session.with(|session| session.proposals().cloned()).map(|(changes, refused)| {
                    proposals_view(state.shared, changes, refused)
                })
            }}
            <p class="note" role="status" data-testid="chat-status">
                {move || chat::phase_words(&phase())}
            </p>
            <div class="msg-tools">
                <button
                    class="tool"
                    type="button"
                    prop:disabled=move || answer().is_empty()
                    on:click=copy
                >
                    {copy_icon()}
                    "Copiar"
                </button>
                <button
                    class="tool"
                    type="button"
                    data-testid="chat-save"
                    prop:disabled=move || {
                        !session.with(Session::can_save) || state.save_busy.get()
                    }
                    on:click=move |_| state.save(session)
                >
                    {bookmark_icon()}
                    "Salvar"
                </button>
                {move || copy_note.get().map(|words| view! { <span class="note" role="status">{words}</span> })}
            </div>
            {move || {
                session
                    .with(Session::saved)
                    .then(|| {
                        view! {
                            <p class="note" role="status" data-testid="chat-saved">
                                "Resposta salva na conversa do projeto."
                            </p>
                        }
                    })
            }}
        </div>
    }
}

/// What an answer proposed: the files of its changes, waiting in Construir, and how many blocks were not taken.
fn proposals_view(
    shared: Shared,
    changes: Vec<chat::ProposedChange>,
    refused: usize,
) -> impl IntoView {
    let count = changes.len();
    // Markdown files wait in Documentar and the others in Construir; each place present gets its button.
    let docs = changes
        .iter()
        .any(|change| noct_core::file_change::is_markdown(&change.path));
    let code = changes
        .iter()
        .any(|change| !noct_core::file_change::is_markdown(&change.path));
    let title = match count {
        0 => "Nenhuma alteração proposta foi aceita.".to_string(),
        1 => "1 alteração proposta, esperando a sua aprovação.".to_string(),
        many => format!("{many} alterações propostas, esperando a sua aprovação."),
    };
    view! {
        <section class="card proposals" data-testid="chat-proposals">
            <p class="proposals__title">{title}</p>
            {(count > 0).then(|| {
                view! {
                    <ul class="proposals__files">
                        {changes
                            .into_iter()
                            .map(|change| view! { <li class="mono">{change.path}</li> })
                            .collect_view()}
                    </ul>
                }
            })}
            {(refused > 0).then(|| {
                view! {
                    <p class="note" data-testid="chat-proposals-refused">
                        {if refused == 1 {
                            "1 bloco não foi aceito: caminho, nome ou tamanho não permitido.".to_string()
                        } else {
                            format!("{refused} blocos não foram aceitos: caminho, nome ou tamanho não permitido.")
                        }}
                    </p>
                }
            })}
            {(count > 0).then(|| {
                view! {
                    <div class="actions actions--start">
                        {code.then(|| view! {
                            <button
                                class="btn btn--primary btn--sm"
                                type="button"
                                data-testid="open-build"
                                on:click=move |_| shared.build_requests.update(|n| *n += 1)
                            >
                                "Revisar em Construir"
                            </button>
                        })}
                        {docs.then(|| view! {
                            <button
                                class=if code { "btn btn--sm" } else { "btn btn--primary btn--sm" }
                                type="button"
                                data-testid="open-docs"
                                on:click=move |_| shared.docs_requests.update(|n| *n += 1)
                            >
                                "Revisar em Documentar"
                            </button>
                        })}
                    </div>
                }
            })}
        </section>
    }
}

/// The id of the model pill, which takes the focus back when the picker closes.
const PILL_ID: &str = "composer-model-pill";

/// The composer under the conversation: the question box, the model, and the button that sends or stops the answer.
pub fn composer(state: ChatState) -> impl IntoView {
    let busy = Memo::new(move |_| state.busy());
    // The model picker over the composer. It opens from the pill when a project is chosen, and gives the focus back to
    // the pill when it closes.
    let picking = RwSignal::new(false);
    let close_picker = move || {
        picking.set(false);
        dom::focus(PILL_ID);
    };
    view! {
        <div class="composer-wrap">
            {move || {
                picking.get().then(|| {
                    view! {
                        <div class="popover-backdrop" aria-hidden="true" on:click=move |_| close_picker()></div>
                        <div class="popover" role="dialog" aria-label="Escolher o modelo" data-testid="composer-picker">
                            {model_picker::picker(state.shared, "composer-models", move |_| close_picker())}
                        </div>
                    }
                })
            }}
            {move || need_view(state)}
            <div class="composer">
                <label for="chat-question" class="sr-only">"Pergunte sobre o projeto"</label>
                <div class="grow" data-value=move || state.question.get()>
                <textarea
                    id="chat-question"
                    rows="1"
                    maxlength=MAXIMUM_QUESTION_CHARACTERS
                    placeholder=move || match sidebar::current_name(state.shared) {
                        Some(name) => format!("Pergunte sobre {name}…"),
                        None => "Adicione um projeto para perguntar".to_string(),
                    }
                    prop:value=move || state.question.get()
                    prop:disabled=move || state.shared.current.with(Option::is_none)
                    on:input=move |event| state.question.set(event_target_value(&event))
                    on:keydown=move |event: KeyboardEvent| {
                        if event.key() == "Enter" && !event.shift_key() && !event.is_composing() {
                            event.prevent_default();
                            if state.busy() {
                                return;
                            }
                            state.ask();
                        }
                    }
                ></textarea>
                </div>
                <div class="composer__bar">
                    <button
                        class="pill"
                        id=PILL_ID
                        type="button"
                        data-testid="composer-model"
                        aria-haspopup="dialog"
                        aria-expanded=move || picking.get().to_string()
                        on:click=move |_| {
                            if state.shared.current.with_untracked(Option::is_some) {
                                picking.update(|open| *open = !*open);
                            } else {
                                state.shared.open_settings();
                            }
                        }
                    >
                        <span
                            class=move || {
                                if state.model_gate() == Gate::Ready && state.key_gate() == Gate::Ready {
                                    "dot"
                                } else {
                                    "dot dot--off"
                                }
                            }
                            aria-hidden="true"
                        ></span>
                        <span class="pill__name mono">{move || state.model_label()}</span>
                    </button>
                    <span class="composer__hint">"Enter envia · Shift+Enter quebra linha"</span>
                    {move || {
                        if busy.get() {
                            view! {
                                <button
                                    class="send send--stop"
                                    type="button"
                                    aria-label="Parar"
                                    data-testid="chat-cancel"
                                    on:click=move |_| state.cancel()
                                >
                                    {stop_icon()}
                                </button>
                            }
                            .into_any()
                        } else {
                            view! {
                                <button
                                    class="send"
                                    type="button"
                                    aria-label="Enviar"
                                    data-testid="chat-send"
                                    prop:disabled=move || !state.can_send()
                                    on:click=move |_| state.ask()
                                >
                                    {send_icon()}
                                </button>
                            }
                            .into_any()
                        }
                    }}
                </div>
            </div>
        </div>
    }
}

fn copy_icon() -> impl IntoView {
    view! {
        <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
            <rect x="9" y="9" width="12" height="12" rx="2" />
            <path d="M5 15H4a1 1 0 0 1-1-1V4a1 1 0 0 1 1-1h10a1 1 0 0 1 1 1v1" />
        </svg>
    }
}

fn bookmark_icon() -> impl IntoView {
    view! {
        <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
            <path d="M6 3h12v18l-6-4-6 4z" />
        </svg>
    }
}

fn send_icon() -> impl IntoView {
    view! {
        <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.4" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
            <path d="M12 19V5M5 12l7-7 7 7" />
        </svg>
    }
}

fn stop_icon() -> impl IntoView {
    view! {
        <svg width="12" height="12" viewBox="0 0 24 24" aria-hidden="true">
            <rect x="4" y="4" width="16" height="16" rx="3" fill="currentColor" />
        </svg>
    }
}

/// What the chat knows of a precondition of a send: ready, missing, still being read, or not readable.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Gate {
    Ready,
    Missing,
    Loading,
    Failed,
}

/// The permission card, in the conversation. It appears when a question is sent to a project without the permission. The
/// question stays in the composer, and nothing is sent until the person allows the project here.
pub fn consent_card(state: ChatState) -> impl IntoView {
    move || {
        let (Some(path), Some(asked)) = (state.shared.current.get(), state.awaiting.get()) else {
            return ().into_any();
        };
        if path != asked {
            return ().into_any();
        }
        let granted = state.permission_of(&path) == Gate::Ready;
        let busy = state.consent_busy.get();
        let error = state.consent_error.get();
        let heading = match sidebar::current_name(state.shared) {
            Some(name) => format!("Enviar arquivos de {name} ao OpenRouter?"),
            None => "Enviar arquivos do projeto ao OpenRouter?".to_string(),
        };
        let allow_path = path.clone();
        view! {
            <section class="card consent" data-testid="consent-card" aria-labelledby="consent-title">
                <div class="consent__icon" aria-hidden="true">{shield_icon()}</div>
                <div>
                    <h3 id="consent-title">{heading}</h3>
                    {if granted {
                        view! {
                            <p class="body" role="status" data-testid="consent-granted">
                                "Permissão concedida neste projeto. Envie a pergunta quando quiser."
                            </p>
                        }
                        .into_any()
                    } else {
                        view! {
                            <p class="body">"Para responder a esta pergunta, o NOCT envia:"</p>
                            <ul>
                                <li>"Sua pergunta. As respostas anteriores desta janela não são enviadas."</li>
                                <li>"Até 128 KiB de texto dos arquivos do topo do projeto, sem arquivos sensíveis, como .env, chaves e certificados."</li>
                            </ul>
                            <div class="actions">
                                <button
                                    class="btn btn--primary"
                                    type="button"
                                    data-testid="consent-allow"
                                    prop:disabled=busy
                                    on:click=move |_| state.grant(allow_path.clone())
                                >
                                    "Permitir neste projeto"
                                </button>
                                <button
                                    class="btn btn--ghost"
                                    type="button"
                                    data-testid="consent-later"
                                    prop:disabled=busy
                                    on:click=move |_| state.awaiting.set(None)
                                >
                                    "Agora não"
                                </button>
                            </div>
                        }
                        .into_any()
                    }}
                    {error.map(|message| {
                        view! { <p class="note" role="alert" data-testid="consent-error">{message}</p> }
                    })}
                </div>
            </section>
        }
        .into_any()
    }
}

/// What the chat still needs before it can send, first missing first, with the action that resolves it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Need {
    Project,
    ChatOff,
    Key,
    Model,
    Permission,
    Checking,
    Unreadable,
}

fn need_of(state: ChatState) -> Option<Need> {
    let path = state.shared.current.get()?;
    let gates = [
        (state.chat_gate(), Need::ChatOff),
        (state.key_gate(), Need::Key),
        (state.model_gate(), Need::Model),
        (state.permission_of(&path), Need::Permission),
    ];
    for (gate, need) in gates {
        match gate {
            Gate::Ready => {}
            Gate::Missing => return Some(need),
            Gate::Loading => return Some(Need::Checking),
            Gate::Failed => return Some(Need::Unreadable),
        }
    }
    None
}

/// The button of the line over the composer: its label and what it does.
type NeedAction = Option<(&'static str, Box<dyn Fn() + 'static>)>;

/// The line over the composer: what is missing, and a button that resolves it. Nothing is shown when the chat can send.
fn need_view(state: ChatState) -> AnyView {
    let need = if state.shared.current.with(Option::is_none) {
        Some(Need::Project)
    } else {
        need_of(state)
    };
    let Some(need) = need else {
        return ().into_any();
    };
    // Before the first question, the empty conversation already asks for a project or a key, with its own button.
    if matches!(need, Need::Project | Need::Key) && state.turns.with(Vec::is_empty) {
        return ().into_any();
    }
    let shared = state.shared;
    let (words, action): (&str, NeedAction) = match need {
        Need::Project => (
            "Adicione um projeto para perguntar.",
            Some(("Adicionar projeto", Box::new(move || shared.add_project()))),
        ),
        Need::ChatOff => (
            "O chat está desligado. Nada é enviado.",
            Some(("Ligar", Box::new(move || state.turn_on()))),
        ),
        Need::Key => (
            "Falta a chave do OpenRouter.",
            Some(("Adicionar chave", Box::new(move || shared.open_settings()))),
        ),
        Need::Model => (
            "Falta o modelo deste projeto.",
            Some(("Escolher modelo", Box::new(move || shared.open_settings()))),
        ),
        Need::Permission => (
            "Este projeto ainda não pode enviar arquivos.",
            Some((
                "Ver permissão",
                Box::new(move || state.awaiting.set(state.shared.current.get_untracked())),
            )),
        ),
        Need::Checking => ("Verificando…", None),
        Need::Unreadable => (
            "Não foi possível ler o estado do chat.",
            Some(("Abrir Ajustes", Box::new(move || shared.open_settings()))),
        ),
    };
    view! {
        <div class="need" role="status" data-testid="chat-need">
            <span class="need__text">{words}</span>
            {action.map(|(label, run)| {
                view! {
                    <button class="btn btn--sm" type="button" data-testid="chat-need-action" on:click=move |_| run()>
                        {label}
                    </button>
                }
            })}
        </div>
    }
    .into_any()
}

fn shield_icon() -> impl IntoView {
    view! {
        <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
            <path d="M12 2 4 5v6c0 5 3.4 9.4 8 11 4.6-1.6 8-6 8-11V5z" />
        </svg>
    }
}

/// What the conversation shows before its first question: the step that is missing first, or the project's own start.
fn empty_state(state: ChatState) -> AnyView {
    // The permission card takes the place of the start, so the card is in view with the composer under it.
    if state.turns.with(|turns| !turns.is_empty()) || state.awaiting.with(Option::is_some) {
        return ().into_any();
    }
    if state.shared.current.with(Option::is_none) {
        // Until the list has arrived, the window does not know yet whether a project exists.
        return match state.shared.projects.get() {
            WorkspacesView::Loading => view! {
                <section class="empty" data-testid="empty-loading">
                    <p class="empty__lead" role="status">"Carregando projetos…"</p>
                </section>
            }
            .into_any(),
            WorkspacesView::Failed(_) => view! {
                <section class="empty" data-testid="empty-projects-error">
                    <p class="empty__lead" role="status">"A lista de projetos não pôde ser lida. Tente de novo na barra lateral."</p>
                </section>
            }
            .into_any(),
            WorkspacesView::Ready(_) => no_project_view(state.shared).into_any(),
        };
    }
    match state.key_gate() {
        Gate::Missing => no_key_view(state.shared).into_any(),
        _ => suggestions_view(state).into_any(),
    }
}

/// The first question of a project, with the four suggestions. A suggestion fills the composer, and sends nothing.
fn suggestions_view(state: ChatState) -> impl IntoView {
    const SUGGESTIONS: [(&str, &str); 4] = [
        ("Resumo do projeto", "O que é e como está organizado"),
        (
            "Onde está a lógica principal?",
            "Encontrar arquivos e funções",
        ),
        ("Riscos de segurança", "Pontos para revisar primeiro"),
        (
            "O que mudou recentemente",
            "Pelos arquivos do topo do projeto",
        ),
    ];
    let title = match sidebar::current_name(state.shared) {
        Some(name) => format!("O que você quer entender em {name}?"),
        None => "O que você quer entender neste projeto?".to_string(),
    };
    view! {
        <section class="empty" data-testid="empty-project">
            <div class="empty__icon" aria-hidden="true">{search_icon()}</div>
            <h1 class="empty__title">{title}</h1>
            <p class="empty__lead">"Somente leitura. Nada é executado nem alterado."</p>
            <div class="suggest">
                {SUGGESTIONS
                    .iter()
                    .map(|(text, note)| {
                        let text = *text;
                        view! {
                            <button type="button" on:click=move |_| state.question.set(text.to_string())>
                                {text}
                                <small>{*note}</small>
                            </button>
                        }
                    })
                    .collect_view()}
            </div>
        </section>
    }
}

/// The step before any question: a key is needed, and Ajustes is where it is added.
fn no_key_view(shared: Shared) -> impl IntoView {
    view! {
        <section class="empty" data-testid="empty-no-key">
            <div class="empty__icon" aria-hidden="true">{key_icon()}</div>
            <h1 class="empty__title">"Conecte um modelo para começar"</h1>
            <p class="empty__lead">"Cole a sua chave do OpenRouter. Ela fica no cofre do sistema e nunca volta para a tela."</p>
            <div class="actions">
                <button class="btn btn--primary" type="button" on:click=move |_| shared.open_settings()>
                    "Adicionar chave"
                </button>
            </div>
        </section>
    }
}

/// The step before any conversation: a project has to be added, with the system folder picker.
fn no_project_view(shared: Shared) -> impl IntoView {
    view! {
        <section class="empty" data-testid="empty-no-project">
            <div class="empty__icon" aria-hidden="true">{folder_icon()}</div>
            <h1 class="empty__title">"Adicione um projeto para começar"</h1>
            <p class="empty__lead">"Escolha a pasta de um projeto. O NOCT só lê os arquivos; nada é alterado."</p>
            <div class="actions">
                <button
                    class="btn btn--primary"
                    type="button"
                    prop:disabled=move || shared.adding.get()
                    on:click=move |_| shared.add_project()
                >
                    "Adicionar projeto"
                </button>
            </div>
        </section>
    }
}

fn search_icon() -> impl IntoView {
    view! {
        <svg width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">
            <circle cx="11" cy="11" r="7" />
            <path d="m20 20-3.5-3.5" />
        </svg>
    }
}

fn key_icon() -> impl IntoView {
    view! {
        <svg width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">
            <circle cx="7.5" cy="15.5" r="4.5" />
            <path d="m10.7 12.3 9.3-9.3M17 6l3 3M14 9l3 3" />
        </svg>
    }
}

fn folder_icon() -> impl IntoView {
    view! {
        <svg width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">
            <path d="M3 7a2 2 0 0 1 2-2h4l2 2h8a2 2 0 0 1 2 2v8a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z" />
        </svg>
    }
}
