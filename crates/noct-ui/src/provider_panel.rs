//! The provider groups of the settings area: the OpenRouter key and the default model of the current project (the group
//! Modelo), and the projects whose permission to send was granted (the group Projetos com permissão de envio). The logic
//! is in `provider.rs`; this module draws it and calls the shell's commands.
//!
//! The key field is write-only. It holds the key only until a save is sent, and the draft is emptied before the call, so
//! the field is empty whatever the reply is. The area never shows a key, only whether one is configured.

use std::rc::Rc;

use leptos::prelude::*;

use crate::ipc;
use crate::provider::{self, ConsentStatus, KeyStatus, ModelBindingsView, ProviderView};
use crate::sidebar::{self, Shared};
use crate::workspaces::WorkspacesView;

/// The identifier of the saved default model, or empty when none is saved.
fn saved_model_id(bindings: Option<&ModelBindingsView>) -> String {
    bindings
        .and_then(|bindings| bindings.default_binding.as_ref())
        .map(|model| model.model_id.clone())
        .unwrap_or_default()
}

/// Whether the current project has a default model saved, as the shell last reported it.
fn has_saved_model(shared: Shared) -> bool {
    shared.bindings.with(|view| {
        matches!(
            view,
            Some(ProviderView::Ready(Some(bindings))) if bindings.default_binding.is_some()
        )
    })
}

/// The badge of the key: whether one is configured, or the state of the read.
fn key_badge(view: ProviderView<KeyStatus>) -> AnyView {
    match view {
        ProviderView::Loading => view! {
            <span class="badge" data-testid="key-state">"Verificando…"</span>
        }
        .into_any(),
        ProviderView::Failed(_) => view! {
            <span class="badge" data-testid="key-state">"Não foi possível ler"</span>
        }
        .into_any(),
        ProviderView::Ready(status) if status.configured => view! {
            <span class="badge" data-testid="key-state"><span class="dot" aria-hidden="true"></span>"Configurada"</span>
        }
        .into_any(),
        ProviderView::Ready(_) => view! {
            <span class="badge" data-testid="key-state"><span class="dot dot--off" aria-hidden="true"></span>"Sem chave"</span>
        }
        .into_any(),
    }
}

fn key_configured(view: &ProviderView<KeyStatus>) -> bool {
    matches!(view, ProviderView::Ready(status) if status.configured)
}

/// The group Modelo: the key, with its state and a field that opens on "Trocar", and the default model of the current
/// project. The key and the model are the ones the sidebar shows, and every save updates them.
pub fn model_group(shared: Shared) -> impl IntoView {
    let reveal = RwSignal::new(false);
    let key_draft = RwSignal::new(String::new());
    let key_busy = RwSignal::new(false);
    let key_error = RwSignal::new(None::<String>);
    let model_draft = RwSignal::new(String::new());
    let model_busy = RwSignal::new(false);
    let model_error = RwSignal::new(None::<String>);

    // The draft of the default model follows the saved one whenever the shell reports it, so a change of project shows
    // the model of that project.
    Effect::new(move |_| {
        let saved = shared.bindings.with(|view| match view {
            Some(ProviderView::Ready(bindings)) => Some(saved_model_id(bindings.as_ref())),
            Some(ProviderView::Loading | ProviderView::Failed(_)) | None => None,
        });
        if let Some(saved) = saved {
            model_draft.set(saved);
        }
    });

    // Reads the key again, when the first read failed. The shell's message is shown in the group.
    let read_key = move || {
        shared.key.set(ProviderView::Loading);
        ipc::call(provider::KEY_STATUS, move |outcome| {
            shared.key.set(provider::settle(outcome));
        });
    };

    let save_key = move || {
        let key = key_draft.get_untracked();
        key_draft.set(String::new());
        key_busy.set(true);
        key_error.set(None);
        ipc::call_with(
            provider::KEY_SAVE,
            &provider::key_save_payload(&key),
            move |outcome| {
                key_busy.set(false);
                match provider::settle::<KeyStatus>(outcome) {
                    ProviderView::Ready(status) => {
                        shared.key.set(ProviderView::Ready(status));
                        reveal.set(false);
                    }
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
                ProviderView::Ready(status) => shared.key.set(ProviderView::Ready(status)),
                ProviderView::Failed(failure) => key_error.set(Some(failure.message)),
                ProviderView::Loading => {}
            }
        });
    };

    // Saves the default model of the current project, or clears it with `None`. The reply is what the sidebar shows.
    let write_model = move |model: Option<String>| {
        let Some(path) = shared.current.get_untracked() else {
            return;
        };
        model_busy.set(true);
        model_error.set(None);
        ipc::call_with(
            provider::MODEL_SET,
            &provider::model_set_payload(&path, model.as_deref()),
            move |outcome| {
                model_busy.set(false);
                match provider::settle::<ModelBindingsView>(outcome) {
                    ProviderView::Ready(bindings) => {
                        if shared.current.get_untracked().as_deref() == Some(path.as_str()) {
                            shared
                                .bindings
                                .set(Some(ProviderView::Ready(Some(bindings))));
                        }
                    }
                    ProviderView::Failed(failure) => model_error.set(Some(failure.message)),
                    ProviderView::Loading => {}
                }
            },
        );
    };

    view! {
        <div class="group" data-testid="provider">
            <div class="group__title">"Modelo"</div>
            <div class="list">
                <div class="item">
                    <div class="item__txt">
                        <div class="item__name">"Chave do OpenRouter"</div>
                        <div class="item__desc">"Guardada no cofre do sistema. Nunca volta para a tela."</div>
                    </div>
                    {move || key_badge(shared.key.get())}
                    <button
                        class="btn"
                        type="button"
                        data-testid="key-change"
                        aria-expanded=move || reveal.get().to_string()
                        disabled=move || key_busy.get()
                        on:click=move |_| reveal.update(|open| *open = !*open)
                    >
                        "Trocar"
                    </button>
                    {move || {
                        key_configured(&shared.key.get()).then(|| {
                            view! {
                                <button
                                    class="btn btn--ghost"
                                    type="button"
                                    data-testid="key-remove"
                                    disabled=move || key_busy.get()
                                    on:click=move |_| remove_key()
                                >
                                    "Remover"
                                </button>
                            }
                        })
                    }}
                </div>
                {move || match shared.key.get() {
                    ProviderView::Failed(failure) => view! {
                        <div class="error-card" role="alert" data-testid="key-error">
                            <div>
                                <strong>"Não foi possível ler o estado da chave."</strong>
                                <p>{failure.message}</p>
                            </div>
                            <button class="btn" type="button" data-testid="key-retry" on:click=move |_| read_key()>
                                "Tentar novamente"
                            </button>
                        </div>
                    }
                    .into_any(),
                    ProviderView::Loading | ProviderView::Ready(_) => ().into_any(),
                }}
                {move || {
                    reveal.get().then(|| {
                        view! {
                            <div class="item">
                                <label class="sr-only" for="provider-key">"Nova chave do OpenRouter"</label>
                                <input
                                    id="provider-key"
                                    class="field field--wide"
                                    type="password"
                                    autocomplete="off"
                                    autocapitalize="off"
                                    spellcheck="false"
                                    placeholder="sk-or-v1-…"
                                    disabled=move || key_busy.get()
                                    prop:value=move || key_draft.get()
                                    on:input=move |event| key_draft.set(event_target_value(&event))
                                />
                                <button
                                    class="btn btn--primary"
                                    type="button"
                                    data-testid="key-save"
                                    disabled=move || key_busy.get() || key_draft.get().is_empty()
                                    on:click=move |_| save_key()
                                >
                                    "Salvar"
                                </button>
                                <button
                                    class="btn btn--ghost"
                                    type="button"
                                    disabled=move || key_busy.get()
                                    on:click=move |_| {
                                        key_draft.set(String::new());
                                        reveal.set(false);
                                    }
                                >
                                    "Cancelar"
                                </button>
                            </div>
                        }
                    })
                }}
                {move || {
                    key_error.get().map(|message| {
                        view! { <p class="note" role="alert" data-testid="key-save-error">{message}</p> }
                    })
                }}
                <div class="item">
                    <div class="item__txt">
                        <div class="item__name">"Modelo padrão"</div>
                        <div class="item__desc">
                            {move || match sidebar::current_name(shared) {
                                Some(name) => format!("Do projeto {name}. Termine com :free para a versão gratuita."),
                                None => "Escolha um projeto na barra lateral.".to_string(),
                            }}
                        </div>
                    </div>
                    <label class="sr-only" for="provider-model">"Id do modelo padrão"</label>
                    <input
                        id="provider-model"
                        class="field field--wide mono"
                        type="text"
                        autocomplete="off"
                        autocapitalize="off"
                        spellcheck="false"
                        placeholder="fornecedor/modelo"
                        disabled=move || model_busy.get() || shared.current.get().is_none()
                        prop:value=move || model_draft.get()
                        on:input=move |event| model_draft.set(event_target_value(&event))
                    />
                    <button
                        class="btn btn--primary"
                        type="button"
                        data-testid="model-save"
                        disabled=move || model_busy.get() || model_draft.get().trim().is_empty()
                        on:click=move |_| write_model(Some(model_draft.get_untracked().trim().to_string()))
                    >
                        "Salvar"
                    </button>
                    {move || {
                        has_saved_model(shared).then(|| {
                            view! {
                                <button
                                    class="btn btn--ghost"
                                    type="button"
                                    data-testid="model-clear"
                                    disabled=move || model_busy.get()
                                    on:click=move |_| write_model(None)
                                >
                                    "Limpar"
                                </button>
                            }
                        })
                    }}
                </div>
                {move || {
                    model_error.get().map(|message| {
                        view! { <p class="note" role="alert" data-testid="model-error">{message}</p> }
                    })
                }}
            </div>
        </div>
    }
}

/// What the list of permissions says while it is read, or when it has nothing to show. `None` when the list is complete
/// and has projects in it.
fn permissions_status(
    projects: &WorkspacesView,
    progress: Option<(usize, usize)>,
    shown: usize,
) -> Option<String> {
    match projects {
        WorkspacesView::Loading => Some("Carregando projetos…".to_string()),
        WorkspacesView::Failed(failure) => Some(failure.message.clone()),
        WorkspacesView::Ready(page) if page.items.is_empty() => {
            Some("Nenhum projeto registrado ainda.".to_string())
        }
        WorkspacesView::Ready(_) => match progress {
            Some((done, total)) => Some(format!("Verificando {} de {}…", done + 1, total)),
            None if shown == 0 => Some("Nenhum projeto com permissão ainda.".to_string()),
            None => None,
        },
    }
}

/// Asks the shell for the permission of the project at `index`, then of the next one. The reads run one at a time, so a
/// long list does not send many calls at once.
fn read_grants(
    projects: Rc<Vec<(String, String)>>,
    index: usize,
    granted: RwSignal<Vec<(String, String)>>,
    progress: RwSignal<Option<(usize, usize)>>,
    error: RwSignal<Option<String>>,
) {
    if index >= projects.len() {
        progress.set(None);
        return;
    }
    progress.set(Some((index, projects.len())));
    let (name, path) = projects[index].clone();
    let next = Rc::clone(&projects);
    ipc::call_with(
        provider::CONSENT_STATUS,
        &provider::consent_payload(&path),
        move |outcome| {
            match provider::settle::<ConsentStatus>(outcome) {
                ProviderView::Ready(status) if status.granted => {
                    granted.update(|list| list.push((name, path)));
                }
                ProviderView::Ready(_) | ProviderView::Loading => {}
                ProviderView::Failed(failure) => error.set(Some(failure.message)),
            }
            read_grants(next, index + 1, granted, progress, error);
        },
    );
}

/// The group Projetos com permissão de envio: the projects of the sidebar that were granted permission. The permissions
/// are read when the group opens, one project after another; a granted project can be revoked from here.
pub fn permissions_group(shared: Shared) -> impl IntoView {
    let granted = RwSignal::new(Vec::<(String, String)>::new());
    let progress = RwSignal::new(None::<(usize, usize)>);
    let started = RwSignal::new(false);
    let error = RwSignal::new(None::<String>);
    let revoke_busy = RwSignal::new(false);

    // The reads start once the list of projects has arrived.
    Effect::new(move |_| {
        if started.get_untracked() {
            return;
        }
        if let WorkspacesView::Ready(page) = shared.projects.get() {
            started.set(true);
            let projects: Vec<(String, String)> = page
                .items
                .iter()
                .map(|workspace| (workspace.name.clone(), workspace.path.clone()))
                .collect();
            read_grants(Rc::new(projects), 0, granted, progress, error);
        }
    });

    let revoke = move |path: String| {
        revoke_busy.set(true);
        error.set(None);
        let asked = path.clone();
        ipc::call_with(
            provider::CONSENT_REVOKE,
            &provider::consent_payload(&path),
            move |outcome| {
                revoke_busy.set(false);
                match provider::settle::<ConsentStatus>(outcome) {
                    ProviderView::Ready(status) => {
                        if !status.granted {
                            granted.update(|list| list.retain(|(_, known)| *known != asked));
                        }
                    }
                    ProviderView::Failed(failure) => error.set(Some(failure.message)),
                    ProviderView::Loading => {}
                }
            },
        );
    };

    view! {
        <div class="group" data-testid="permissions">
            <div class="group__title">"Projetos com permissão de envio"</div>
            <div class="list">
                {move || {
                    granted
                        .get()
                        .into_iter()
                        .map(|(name, path)| {
                            let revoke_path = path.clone();
                            view! {
                                <div class="item">
                                    <div class="item__txt">
                                        <div class="item__name">{name}</div>
                                        <div class="item__desc path">{path}</div>
                                    </div>
                                    <button
                                        class="btn btn--ghost"
                                        type="button"
                                        disabled=move || revoke_busy.get()
                                        on:click=move |_| revoke(revoke_path.clone())
                                    >
                                        "Revogar"
                                    </button>
                                </div>
                            }
                        })
                        .collect_view()
                }}
            </div>
            {move || {
                permissions_status(
                    &shared.projects.get(),
                    progress.get(),
                    granted.with(Vec::len),
                )
                .map(|words| view! { <p class="note" role="status" data-testid="permissions-status">{words}</p> })
            }}
            {move || {
                error.get().map(|message| {
                    view! { <p class="note" role="alert" data-testid="permissions-error">{message}</p> }
                })
            }}
            <div class="foot-note">"A permissão é pedida na primeira pergunta de cada projeto."</div>
        </div>
    }
}
