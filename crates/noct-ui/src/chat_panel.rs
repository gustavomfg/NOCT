//! The chat of the review panel: the switch, the project, the question, the streamed answer, and the save of the answer
//! into a conversation, which the person asks for. The logic is in `chat.rs`; this module draws it and calls the shell's
//! commands.
//!
//! The state is created once, by the app, so an answer that is still arriving keeps arriving when the person moves to
//! another mode. Deltas update only the answer text: the controls read memos, which change only when their value does.

use leptos::prelude::*;
use noct_core::limits::provider_chat::MAXIMUM_QUESTION_CHARACTERS;

use crate::chat::{self, ChatStatus, CreatedConversation, Phase, Session};
use crate::ipc;
use crate::provider::{self, ProviderView};
use crate::workspaces::WorkspacesView;

/// The chat's state. Every field is a signal, so the value is copied into each closure.
#[derive(Clone, Copy)]
pub struct ChatState {
    status: RwSignal<ProviderView<ChatStatus>>,
    selected: RwSignal<String>,
    question: RwSignal<String>,
    session: RwSignal<Session>,
    /// The conversation of the last save, with the project it belongs to. A save for that project reuses it.
    conversation: RwSignal<Option<(String, String)>>,
    save_busy: RwSignal<bool>,
    save_error: RwSignal<Option<String>>,
}

impl ChatState {
    pub fn new() -> Self {
        Self {
            status: RwSignal::new(ProviderView::Loading),
            selected: RwSignal::new(String::new()),
            question: RwSignal::new(String::new()),
            session: RwSignal::new(Session::default()),
            conversation: RwSignal::new(None),
            save_busy: RwSignal::new(false),
            save_error: RwSignal::new(None),
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

    /// Sends the question about the chosen project. The channel is registered before the call, so the first event has a
    /// place to go. A refusal of the shell is shown for this run only.
    fn ask(self) {
        let workspace = self.selected.get_untracked();
        let question = self.question.get_untracked();
        if !self.enabled()
            || workspace.is_empty()
            || !chat::question_is_sendable(&question)
            || self.session.with_untracked(Session::busy)
        {
            return;
        }
        let mut run = 0;
        self.session
            .update(|session| run = session.start(&workspace));
        let Some(channel) = ipc::channel(move |text: String| {
            if let Some(frame) = chat::parse_frame(&text) {
                self.session.update(|session| session.frame(run, frame));
            }
        }) else {
            self.session.update(|session| {
                session.refused(run, "O NOCT não conseguiu abrir a resposta.".to_string())
            });
            return;
        };
        let payload = chat::send_payload(&workspace, question.trim(), &channel);
        ipc::call_with(chat::SEND, &payload, move |outcome| {
            if let Err(failure) = ipc::reply(outcome) {
                self.session
                    .update(|session| session.refused(run, failure.message));
            }
        });
    }

    /// Stops the answer. What arrived stays on screen, and nothing more is shown after the stop.
    fn cancel(self) {
        self.session.update(|session| session.cancel());
        ipc::call(chat::CANCEL, |_| {});
    }

    /// Starts a new conversation: the answer on screen is cleared, and the next save creates a conversation of its own.
    /// A question still running is stopped first, so nothing arrives into the new conversation.
    pub fn new_conversation(self) {
        if self.session.with_untracked(Session::busy) {
            self.cancel();
        }
        self.session.set(Session::default());
        self.conversation.set(None);
        self.question.set(String::new());
        self.save_error.set(None);
    }

    /// Keeps the answer in a conversation of its project. The conversation is created the first time, and reused after.
    fn save(self) {
        let (run, workspace, content) = self.session.with_untracked(|session| {
            (
                session.run(),
                session.workspace().to_string(),
                session.answer().to_string(),
            )
        });
        if !self.session.with_untracked(Session::can_save) || self.save_busy.get_untracked() {
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
            Some(id) => self.write_answer(run, id, content),
            None => ipc::call_with(
                chat::CONVERSATION_CREATE,
                &chat::conversation_payload(&workspace),
                move |outcome| match ipc::reply(outcome) {
                    Ok(text) => match serde_json::from_str::<CreatedConversation>(&text) {
                        Ok(created) => {
                            self.conversation.set(Some((workspace, created.id.clone())));
                            self.write_answer(run, created.id, content);
                        }
                        Err(_) => self.fail_save(ipc::invalid_response().message),
                    },
                    Err(failure) => self.fail_save(failure.message),
                },
            ),
        }
    }

    fn write_answer(self, run: u64, conversation: String, content: String) {
        ipc::call_with(
            chat::SAVE_ASSISTANT,
            &chat::save_payload(&conversation, &content),
            move |outcome| {
                self.save_busy.set(false);
                match ipc::reply(outcome) {
                    Ok(_) => self.session.update(|session| session.mark_saved(run)),
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
}

/// The chat section of the review panel. `projects` is the list the sidebar shows, and the project is chosen from it.
pub fn chat_section(state: ChatState, projects: ReadSignal<WorkspacesView>) -> impl IntoView {
    let enabled = Memo::new(move |_| state.enabled());
    let busy = Memo::new(move |_| state.session.with(Session::busy));
    let phase = Memo::new(move |_| state.session.with(|session| session.phase().clone()));
    let has_answer = Memo::new(move |_| state.session.with(|session| !session.answer().is_empty()));
    let can_save =
        Memo::new(move |_| state.session.with(Session::can_save) && !state.save_busy.get());
    let can_ask = Memo::new(move |_| {
        enabled.get()
            && !busy.get()
            && !state.selected.with(String::is_empty)
            && state
                .question
                .with(|question| chat::question_is_sendable(question))
    });

    view! {
        <section class="stack" aria-labelledby="chat-title" data-testid="chat">
            <h2 class="h2" id="chat-title">"Perguntar sobre o projeto"</h2>
            <p class="lead">
                "Somente leitura: o NOCT envia ao OpenRouter só os arquivos de texto do topo do projeto que as regras permitem. Nada é executado nem gravado no projeto."
            </p>
            {move || state.status.with(switch_view)}
            <form
                class="stack"
                data-testid="chat-form"
                on:submit=move |event| {
                    event.prevent_default();
                    state.ask();
                }
            >
                <label for="chat-project">"Projeto"</label>
                <select
                    id="chat-project"
                    class="select"
                    prop:value=move || state.selected.get()
                    prop:disabled=move || !enabled.get() || busy.get()
                    on:change=move |event| state.selected.set(event_target_value(&event))
                >
                    <option value="">"Escolha um projeto"</option>
                    {move || projects.with(project_options)}
                </select>
                <label for="chat-question">"Pergunta"</label>
                <textarea
                    id="chat-question"
                    class="field"
                    rows="3"
                    maxlength=MAXIMUM_QUESTION_CHARACTERS
                    prop:value=move || state.question.get()
                    prop:disabled=move || !enabled.get() || busy.get()
                    on:input=move |event| state.question.set(event_target_value(&event))
                ></textarea>
                <div class="actions">
                    <button
                        class="btn btn--primary"
                        type="submit"
                        prop:disabled=move || !can_ask.get()
                    >
                        "Perguntar"
                    </button>
                    {move || {
                        busy.get()
                            .then(|| {
                                view! {
                                    <button
                                        class="btn btn--ghost"
                                        type="button"
                                        data-testid="chat-cancel"
                                        on:click=move |_| state.cancel()
                                    >
                                        "Cancelar"
                                    </button>
                                }
                            })
                    }}
                </div>
            </form>
            <p class="note" role="status" data-testid="chat-status">
                {move || chat::phase_words(&phase.get())}
            </p>
            {move || match phase.get() {
                Phase::Failed(message) => view! {
                    <p class="note" role="alert" data-testid="chat-error">{message}</p>
                }
                .into_any(),
                _ => ().into_any(),
            }}
            {move || {
                has_answer
                    .get()
                    .then(|| {
                        view! {
                            <div
                                class="card chat-answer"
                                role="region"
                                aria-label="Resposta do provedor"
                                data-testid="chat-answer"
                            >
                                {move || state.session.with(|session| session.answer().to_string())}
                            </div>
                        }
                    })
            }}
            <div class="actions">
                <button
                    class="btn btn--ghost"
                    type="button"
                    data-testid="chat-save"
                    prop:disabled=move || !can_save.get()
                    on:click=move |_| state.save()
                >
                    "Salvar resposta"
                </button>
            </div>
            {move || {
                state
                    .session
                    .with(Session::saved)
                    .then(|| {
                        view! {
                            <p class="note" role="status" data-testid="chat-saved">
                                "Resposta salva na conversa do projeto."
                            </p>
                        }
                    })
            }}
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

/// The switch, as the shell reports it. A read that is still running, or that failed, is said so.
fn switch_view(view: &ProviderView<ChatStatus>) -> AnyView {
    match view {
        ProviderView::Loading => view! {
            <p class="note" role="status" data-testid="chat-switch-loading">"Verificando o chat…"</p>
        }
        .into_any(),
        ProviderView::Ready(status) => view! {
            <p class="note" role="status" data-testid="chat-switch">{chat::switch_words(status.enabled)}</p>
        }
        .into_any(),
        ProviderView::Failed(failure) => view! {
            <p class="note" role="alert" data-testid="chat-switch-error">{failure.message.clone()}</p>
        }
        .into_any(),
    }
}

/// The projects the sidebar shows, as the choices of the select. Nothing is listed until the list has arrived.
fn project_options(view: &WorkspacesView) -> AnyView {
    match view {
        WorkspacesView::Ready(page) => page
            .items
            .iter()
            .map(|workspace| {
                view! { <option value=workspace.path.clone()>{workspace.name.clone()}</option> }
            })
            .collect_view()
            .into_any(),
        _ => ().into_any(),
    }
}
