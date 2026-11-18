//! The provider section of the settings area: the OpenRouter key and the consent of one project. The logic is in
//! `provider.rs`; this module draws it and calls the shell's commands.
//!
//! The key field is write-only. It holds the key only until a save is sent, and the draft is emptied before the call,
//! so the field is empty whatever the reply is. The area never shows a key, only whether one is configured.

use leptos::prelude::*;

use crate::ipc;
use crate::provider::{self, ConsentStatus, KeyStatus, ProviderView};
use crate::workspaces::WorkspacesView;

/// The words the section shows for the key, by the state the shell reports.
fn key_words(configured: bool) -> &'static str {
    if configured {
        "Chave configurada."
    } else {
        "Nenhuma chave configurada."
    }
}

/// The words the section shows for the consent of the chosen project.
fn consent_words(granted: bool) -> &'static str {
    if granted {
        "Consentimento concedido."
    } else {
        "Sem consentimento."
    }
}

/// What a person is asked to confirm. It says what may be sent, and that nothing is sent while the chat is off.
pub const CONSENT_TEXT: &str = "Ao confirmar, o NOCT pode enviar ao OpenRouter, por HTTPS, o contexto deste projeto: \
arquivos de texto e código, sem pastas ignoradas e sem arquivos sensíveis, até 128 KiB no total e 32 KiB por arquivo. \
Hoje o chat com o provedor fica desligado e nada é enviado. Você pode revogar o consentimento a qualquer momento.";

/// The provider section. `projects` is the list the sidebar shows, and the consent is chosen from it.
pub fn provider_section(projects: ReadSignal<WorkspacesView>) -> impl IntoView {
    // The key. The draft is the only place the key is held, and it is emptied when a save is sent.
    let key_view = RwSignal::new(ProviderView::<KeyStatus>::Loading);
    let key_error = RwSignal::new(None::<String>);
    let key_busy = RwSignal::new(false);
    let draft = RwSignal::new(String::new());

    let read_key = move || {
        key_view.set(ProviderView::Loading);
        ipc::call(provider::KEY_STATUS, move |outcome| {
            key_view.set(provider::settle(outcome));
        });
    };
    read_key();

    let save_key = move || {
        let key = draft.get_untracked();
        draft.set(String::new());
        key_busy.set(true);
        key_error.set(None);
        ipc::call_with(
            provider::KEY_SAVE,
            &provider::key_save_payload(&key),
            move |outcome| {
                key_busy.set(false);
                match provider::settle::<KeyStatus>(outcome) {
                    ProviderView::Ready(status) => key_view.set(ProviderView::Ready(status)),
                    ProviderView::Failed(failure) => key_error.set(Some(failure.message)),
                    ProviderView::Loading => {}
                }
            },
        );
    };

    let remove_key = move || {
        key_busy.set(true);
        key_error.set(None);
        ipc::call(provider::KEY_REMOVE, move |outcome| {
            key_busy.set(false);
            match provider::settle::<KeyStatus>(outcome) {
                ProviderView::Ready(status) => key_view.set(ProviderView::Ready(status)),
                ProviderView::Failed(failure) => key_error.set(Some(failure.message)),
                ProviderView::Loading => {}
            }
        });
    };

    // The consent of one project. The chosen path is kept, and a reply for a path that is no longer chosen is ignored.
    let selected = RwSignal::new(String::new());
    let consent_view = RwSignal::new(None::<ProviderView<ConsentStatus>>);
    let confirming = RwSignal::new(false);
    let consent_busy = RwSignal::new(false);

    let choose = move |path: String| {
        confirming.set(false);
        selected.set(path.clone());
        if path.is_empty() {
            consent_view.set(None);
            return;
        }
        consent_view.set(Some(ProviderView::Loading));
        let asked = path.clone();
        ipc::call_with(
            provider::CONSENT_STATUS,
            &provider::consent_payload(&path),
            move |outcome| {
                if selected.get_untracked() == asked {
                    consent_view.set(Some(provider::settle(outcome)));
                }
            },
        );
    };

    let change_consent = move |command: &'static str| {
        let path = selected.get_untracked();
        consent_busy.set(true);
        ipc::call_with(command, &provider::consent_payload(&path), move |outcome| {
            consent_busy.set(false);
            confirming.set(false);
            if selected.get_untracked() == path {
                consent_view.set(Some(provider::settle(outcome)));
            }
        });
    };

    view! {
        <section class="card" aria-labelledby="provider-heading" data-testid="provider">
            <h2 class="h2" id="provider-heading">"Provedor OpenRouter"</h2>
            <p class="lead">
                "A chave fica no cofre do sistema operacional. Ela é enviada uma vez, não volta para esta tela e não vai para o banco de dados."
            </p>

            {move || match key_view.get() {
                ProviderView::Loading => view! {
                    <p class="note" role="status" data-testid="key-loading">"Verificando a chave…"</p>
                }
                .into_any(),
                ProviderView::Failed(failure) => view! {
                    <div class="error-card" role="alert" data-testid="key-error">
                        <div>
                            <strong>"Não foi possível ler o estado da chave."</strong>
                            <p>{failure.message}</p>
                        </div>
                        <button class="btn" type="button" on:click=move |_| read_key()>
                            "Tentar novamente"
                        </button>
                    </div>
                }
                .into_any(),
                ProviderView::Ready(status) => view! {
                    <p class="body" role="status" data-testid="key-status">{key_words(status.configured)}</p>
                }
                .into_any(),
            }}

            <div class="stack">
                <label for="provider-key">"Chave do OpenRouter"</label>
                <input
                    id="provider-key"
                    class="field"
                    type="password"
                    autocomplete="off"
                    autocapitalize="off"
                    spellcheck="false"
                    aria-describedby="provider-key-note"
                    disabled=move || key_busy.get()
                    prop:value=move || draft.get()
                    on:input=move |event| draft.set(event_target_value(&event))
                />
                <p class="note" id="provider-key-note">"O campo é esvaziado assim que a chave é enviada."</p>
                <div class="actions">
                    <button
                        class="btn btn--primary"
                        type="button"
                        disabled=move || key_busy.get() || draft.get().is_empty()
                        on:click=move |_| save_key()
                    >
                        "Salvar chave"
                    </button>
                    <button
                        class="btn btn--ghost"
                        type="button"
                        disabled=move || {
                            key_busy.get()
                                || !matches!(key_view.get(), ProviderView::Ready(status) if status.configured)
                        }
                        on:click=move |_| remove_key()
                    >
                        "Remover chave"
                    </button>
                </div>
                {move || key_error.get().map(|message| view! {
                    <p class="note" role="alert" data-testid="key-save-error">{message}</p>
                })}
            </div>

            <h2 class="h2">"Consentimento por projeto"</h2>
            <p class="lead">
                "Antes de qualquer envio ao OpenRouter, o projeto precisa de consentimento. Hoje o NOCT não envia nada."
            </p>
            <div class="stack">
                <label for="provider-project">"Projeto"</label>
                <select
                    id="provider-project"
                    class="select"
                    prop:value=move || selected.get()
                    on:change=move |event| choose(event_target_value(&event))
                >
                    <option value="">"Escolha um projeto"</option>
                    {move || match projects.get() {
                        WorkspacesView::Ready(page) => page
                            .items
                            .into_iter()
                            .map(|workspace| {
                                view! { <option value=workspace.path.clone()>{workspace.name.clone()}</option> }
                            })
                            .collect_view()
                            .into_any(),
                        WorkspacesView::Loading | WorkspacesView::Failed(_) => ().into_any(),
                    }}
                </select>

                {move || match consent_view.get() {
                    None => view! {
                        <p class="note" data-testid="consent-empty">"Escolha um projeto para ver o consentimento."</p>
                    }
                    .into_any(),
                    Some(ProviderView::Loading) => view! {
                        <p class="note" role="status" data-testid="consent-loading">"Verificando o consentimento…"</p>
                    }
                    .into_any(),
                    Some(ProviderView::Failed(failure)) => view! {
                        <div class="error-card" role="alert" data-testid="consent-error">
                            <div>
                                <strong>"Não foi possível ler o consentimento."</strong>
                                <p>{failure.message}</p>
                            </div>
                        </div>
                    }
                    .into_any(),
                    Some(ProviderView::Ready(status)) => {
                        let granted = status.granted;
                        view! {
                            <p class="body" role="status" data-testid="consent-status">{consent_words(granted)}</p>
                            {if granted {
                                view! {
                                    <div class="actions">
                                        <button
                                            class="btn btn--ghost"
                                            type="button"
                                            disabled=move || consent_busy.get()
                                            on:click=move |_| change_consent(provider::CONSENT_REVOKE)
                                        >
                                            "Revogar consentimento"
                                        </button>
                                    </div>
                                }
                                .into_any()
                            } else if confirming.get() {
                                view! {
                                    <div class="stack" data-testid="consent-confirm">
                                        <p class="body">{CONSENT_TEXT}</p>
                                        <div class="actions">
                                            <button
                                                class="btn btn--primary"
                                                type="button"
                                                disabled=move || consent_busy.get()
                                                on:click=move |_| change_consent(provider::CONSENT_GRANT)
                                            >
                                                "Confirmar consentimento"
                                            </button>
                                            <button
                                                class="btn btn--ghost"
                                                type="button"
                                                disabled=move || consent_busy.get()
                                                on:click=move |_| confirming.set(false)
                                            >
                                                "Cancelar"
                                            </button>
                                        </div>
                                    </div>
                                }
                                .into_any()
                            } else {
                                view! {
                                    <div class="actions">
                                        <button
                                            class="btn btn--primary"
                                            type="button"
                                            disabled=move || consent_busy.get()
                                            on:click=move |_| confirming.set(true)
                                        >
                                            "Pedir consentimento"
                                        </button>
                                    </div>
                                }
                                .into_any()
                            }}
                        }
                        .into_any()
                    }
                }}
            </div>
        </section>
    }
}
