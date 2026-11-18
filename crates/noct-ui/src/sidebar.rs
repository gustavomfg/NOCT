//! The sidebar of the window: the projects, the choice of the current one, and the card of its model. The card shows what
//! the shell holds: the default model saved for the current project, and whether an OpenRouter key is configured. Nothing
//! here sends a question or changes a setting; the card opens Ajustes, where the settings are changed.

use leptos::prelude::*;
use noct_core::contracts::Workspace;

use crate::ipc;
use crate::provider::{self, KeyStatus, ModelBindingsView, ProviderView};
use crate::workspaces::{self, WorkspacesView};

/// The state the sidebar shares with the window. The card shows what the shell reports through these signals.
#[derive(Clone, Copy)]
pub struct Shared {
    pub current: RwSignal<Option<String>>,
    pub projects: RwSignal<WorkspacesView>,
    pub settings_open: RwSignal<bool>,
    /// The key status, as the shell last reported it. The card and the settings both write it.
    pub key: RwSignal<ProviderView<KeyStatus>>,
    /// The default model of the current project, as the shell last reported it. `None` when no project is chosen.
    pub bindings: RwSignal<Option<ProviderView<Option<ModelBindingsView>>>>,
}

impl Shared {
    /// Opens Ajustes. A value that is already open is not written again, so the panel is not rebuilt, and the shell is not
    /// asked for the key a second time.
    pub fn open_settings(self) {
        if !self.settings_open.get_untracked() {
            self.settings_open.set(true);
        }
    }

    /// Leaves Ajustes, if they are open.
    pub fn close_settings(self) {
        if self.settings_open.get_untracked() {
            self.settings_open.set(false);
        }
    }
}

/// The sidebar. `retry` reads the projects again, for the error card of the list.
pub fn sidebar(shared: Shared, retry: impl Fn() + Copy + Send + Sync + 'static) -> impl IntoView {
    // The key is read once, when the window opens. Ajustes reports each change of it through `shared.key`.
    ipc::call(provider::KEY_STATUS, move |outcome| {
        shared.key.set(provider::settle(outcome));
    });

    // The model of the current project is read when the project changes. `None` means no project is chosen.
    Effect::new(move |_| {
        let path = shared.current.get();
        match path {
            None => shared.bindings.set(None),
            Some(path) => {
                shared.bindings.set(Some(ProviderView::Loading));
                let asked = path.clone();
                ipc::call_with(
                    provider::MODEL_READ,
                    &provider::consent_payload(&path),
                    move |outcome| {
                        if shared.current.get_untracked().as_deref() == Some(asked.as_str()) {
                            shared.bindings.set(Some(provider::settle(outcome)));
                        }
                    },
                );
            }
        }
    });

    // Pick the first project once the list arrives, so the window opens on a project and the model card has one.
    Effect::new(move |_| {
        if let WorkspacesView::Ready(page) = shared.projects.get()
            && shared.current.get_untracked().is_none()
            && let Some(first) = page.items.first()
        {
            shared.current.set(Some(first.path.clone()));
        }
    });

    view! {
        <div class="label-row">
            <span>"Projetos"</span>
            <button
                class="icon-btn"
                type="button"
                disabled=true
                aria-label="Adicionar projeto"
                title="Adicionar projeto: ainda não disponível nesta versão"
            >
                {plus_icon()}
            </button>
        </div>
        <nav class="nav" aria-label="Projetos">
            {move || projects_view(shared, retry)}
        </nav>
        <div class="spacer"></div>
        <div class="sidebar__bottom">
            <button
                type="button"
                class="model-card"
                data-testid="model-card"
                on:click=move |_| shared.open_settings()
            >
                <span class=move || dot_class(&shared.key.get(), &shared.bindings.get())></span>
                <span class="model-card__txt">
                    <span class="model-card__name" data-testid="model-card-name">
                        {move || model_name(&shared.bindings.get(), shared.current.get().is_some())}
                    </span>
                    <span class="model-card__sub" data-testid="model-card-sub">
                        {move || key_words(&shared.key.get())}
                    </span>
                </span>
            </button>
            <button
                type="button"
                class="nav__item"
                aria-current=move || shared.settings_open.get().then_some("page")
                on:click=move |_| shared.open_settings()
            >
                "Ajustes"
            </button>
        </div>
    }
}

/// The dot of the card: green when the key is configured and the current project has a model, grey otherwise.
fn dot_class(
    key: &ProviderView<KeyStatus>,
    bindings: &Option<ProviderView<Option<ModelBindingsView>>>,
) -> &'static str {
    let configured = matches!(key, ProviderView::Ready(status) if status.configured);
    let has_model = matches!(
        bindings,
        Some(ProviderView::Ready(Some(bindings))) if bindings.default_binding.is_some()
    );
    if configured && has_model {
        "dot"
    } else {
        "dot dot--off"
    }
}

/// The model's name on the card, as the shell saved it for the current project.
fn model_name(
    bindings: &Option<ProviderView<Option<ModelBindingsView>>>,
    has_project: bool,
) -> String {
    if !has_project {
        return "Nenhum projeto escolhido".to_string();
    }
    match bindings {
        None | Some(ProviderView::Loading) => "Lendo o modelo…".to_string(),
        Some(ProviderView::Failed(_)) => "Modelo indisponível".to_string(),
        Some(ProviderView::Ready(None)) => "Nenhum modelo padrão".to_string(),
        Some(ProviderView::Ready(Some(bindings))) => match &bindings.default_binding {
            Some(model) => model.model_id.clone(),
            None => "Nenhum modelo padrão".to_string(),
        },
    }
}

/// The line under the model's name: whether the key is in the vault.
fn key_words(key: &ProviderView<KeyStatus>) -> String {
    match key {
        ProviderView::Loading => "Verificando a chave…".to_string(),
        ProviderView::Failed(_) => "Não foi possível verificar a chave".to_string(),
        ProviderView::Ready(status) if status.configured => {
            "OpenRouter · chave no cofre".to_string()
        }
        ProviderView::Ready(_) => "Adicione uma chave em Ajustes".to_string(),
    }
}

/// The sidebar's project list, or the state of its read: loading, nothing registered yet, the shell's error with a way
/// to try again, or the list itself. The current project is marked with `aria-current`.
fn projects_view(shared: Shared, retry: impl Fn() + Copy + Send + Sync + 'static) -> AnyView {
    match shared.projects.get() {
        WorkspacesView::Loading => view! {
            <p class="note project__state" role="status" data-testid="projects-loading">
                "Carregando projetos…"
            </p>
        }
        .into_any(),
        WorkspacesView::Failed(failure) => view! {
            <div class="status error-card" role="alert" data-testid="projects-error">
                <p>{failure.message}</p>
                <button class="btn" type="button" on:click=move |_| retry()>
                    "Tentar novamente"
                </button>
            </div>
        }
        .into_any(),
        WorkspacesView::Ready(page) if page.items.is_empty() => view! {
            <p class="note project__state" data-testid="projects-empty">
                "Nenhum projeto registrado ainda."
            </p>
        }
        .into_any(),
        WorkspacesView::Ready(page) => {
            let more = page.has_more.then(|| {
                view! {
                    <p class="note project__state" role="status" data-testid="projects-more">
                        {workspaces::MORE_NOTE}
                    </p>
                }
            });
            view! {
                <ul class="projects" data-testid="projects">
                    {page
                        .items
                        .iter()
                        .map(|workspace| project_item(shared, workspace))
                        .collect_view()}
                </ul>
                {more}
            }
            .into_any()
        }
    }
}

/// One project: its name, a tag when it is a favourite, a note when it cannot be used as it is, and its folder. A usable
/// project is a button that makes it the current one.
fn project_item(shared: Shared, workspace: &Workspace) -> impl IntoView {
    let favorite = workspace.favorite.then_some("Favorito");
    let note = workspaces::note(workspace);
    let path = workspace.path.clone();
    let current_path = path.clone();
    view! {
        <li class="project">
            <button
                type="button"
                class="proj"
                aria-current=move || {
                    (shared.current.get().as_deref() == Some(current_path.as_str())).then_some("true")
                }
                on:click=move |_| {
                    shared.current.set(Some(path.clone()));
                    shared.close_settings();
                }
            >
                <span class="project__name">{workspace.name.clone()}</span>
                {favorite.map(|label| view! { <span class="project__tag">{label}</span> })}
                {note.map(|text| view! { <span class="project__note">{text}</span> })}
                <span class="path project__path">{workspace.path.clone()}</span>
            </button>
        </li>
    }
}

fn plus_icon() -> impl IntoView {
    view! {
        <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" aria-hidden="true">
            <path d="M12 5v14M5 12h14" />
        </svg>
    }
}
