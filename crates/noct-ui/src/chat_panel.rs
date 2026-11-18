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

use crate::chat::{self, ChatStatus, CreatedConversation, Phase, Session};
use crate::dom;
use crate::ipc;
use crate::provider::{self, ProviderView};
use crate::sidebar::Shared;

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
    status: RwSignal<ProviderView<ChatStatus>>,
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
}

impl ChatState {
    pub fn new(shared: Shared) -> Self {
        Self {
            shared,
            status: RwSignal::new(ProviderView::Loading),
            question: RwSignal::new(String::new()),
            turns: RwSignal::new(Vec::new()),
            next_id: RwSignal::new(0),
            conversation: RwSignal::new(None),
            save_busy: RwSignal::new(false),
            save_error: RwSignal::new(None),
            owner: StoredValue::new_local(Owner::current()),
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

    /// Reads the switch. The shell answers with it, and the chat stays closed until it says otherwise.
    pub fn read_status(self) {
        ipc::call(chat::STATUS, move |outcome| {
            self.status.set(provider::settle(outcome));
        });
    }

    fn enabled(self) -> bool {
        self.status
            .with(|view| matches!(view, ProviderView::Ready(status) if status.enabled))
    }

    /// Whether the last question is still running.
    fn busy(self) -> bool {
        self.turns.with(|turns| {
            turns
                .last()
                .is_some_and(|turn| turn.session.with(Session::busy))
        })
    }

    /// Whether the composer may send: the chat is on, a project is chosen, the question has text, and no answer is running.
    fn can_send(self) -> bool {
        self.enabled()
            && self.shared.current.with(Option::is_some)
            && chat::question_is_sendable(&self.question.get())
            && !self.busy()
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

        let Some(channel) = ipc::channel(move |text: String| {
            if let Some(frame) = chat::parse_frame(&text) {
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
                .unwrap_or_else(|| "Modelo".to_string()),
            _ => "Modelo".to_string(),
        })
    }
}

/// The conversation: the switch, then each question and its answer, in order.
pub fn thread(state: ChatState) -> impl IntoView {
    view! {
        <section class="thread" aria-label="Conversa" data-testid="chat">
            {move || state.status.with(switch_view)}
            <For
                each=move || state.turns.get()
                key=|turn| turn.id
                children=move |turn| turn_view(state, turn)
            />
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
                    Phase::Answering => " · respondendo",
                    _ => "",
                }}
            </div>
            {move || {
                let text = answer();
                (!text.is_empty()).then(|| {
                    view! {
                        <div class="msg-ai__body chat-answer" data-testid="chat-answer">
                            {text}
                            {move || {
                                matches!(phase(), Phase::Answering)
                                    .then(|| view! { <span class="caret" aria-hidden="true"></span> })
                            }}
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

/// The composer under the conversation: the question box, the model, and the button that sends or stops the answer.
pub fn composer(state: ChatState) -> impl IntoView {
    let busy = Memo::new(move |_| state.busy());
    view! {
        <div class="composer-wrap">
            <div class="composer">
                <label for="chat-question" class="sr-only">"Pergunte sobre o projeto"</label>
                <textarea
                    id="chat-question"
                    rows="1"
                    maxlength=MAXIMUM_QUESTION_CHARACTERS
                    placeholder="Pergunte sobre o projeto…"
                    prop:value=move || state.question.get()
                    prop:disabled=move || !state.enabled()
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
                <div class="composer__bar">
                    <button
                        class="pill"
                        type="button"
                        data-testid="composer-model"
                        on:click=move |_| state.shared.open_settings()
                    >
                        <span class="dot dot--off" aria-hidden="true"></span>
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

/// The switch, as the shell reports it. A read that is still running, or that failed, is said so.
fn switch_view(view: &ProviderView<ChatStatus>) -> AnyView {
    match view {
        ProviderView::Loading => view! {
            <p class="note" role="status" data-testid="chat-switch-loading">"Verificando o chat…"</p>
        }
        .into_any(),
        ProviderView::Ready(status) => view! {
            <section class="card">
                <p class="note" role="status" data-testid="chat-switch">{chat::switch_words(status.enabled)}</p>
            </section>
        }
        .into_any(),
        ProviderView::Failed(failure) => view! {
            <p class="note" role="alert" data-testid="chat-switch-error">{failure.message.clone()}</p>
        }
        .into_any(),
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
