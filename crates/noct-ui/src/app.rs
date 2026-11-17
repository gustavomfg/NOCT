//! The window: the sidebar, the three modes, the settings (Ajustes) and the fixed question box, laid out as in
//! docs/New_Design/mockup-vidro-painel.html. The sidebar's projects come from `workspaces_list`
//! (`workspaces.rs`) and the settings from `settings_get` (`settings.rs`). The findings, the model and the vault
//! are EXAMPLE data (`examples.rs`). The build and document modes are not available yet: they say so and offer no
//! control for the work, so nothing in this window runs a command or writes a file.

use leptos::prelude::*;
use noct_core::contracts::Workspace;

use crate::dom;
use crate::examples::{EXAMPLE_FINDINGS, EXAMPLE_MODEL, EXAMPLE_VAULT};
use crate::ipc;
use crate::settings::{self, SettingsView};
use crate::theme::Theme;
use crate::workspaces::{self, WorkspacesView};

/// The three modes of the window.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mode {
    Review,
    Build,
    Docs,
}

#[component]
pub fn App() -> impl IntoView {
    // The theme starts as the system prefers, and takes the saved setting when the settings arrive. A theme the
    // person changed in this session is kept over the saved one.
    let system_dark = dom::system_prefers_dark();
    let (theme, set_theme) = signal(Theme::from_setting(None, system_dark));
    let (theme_touched, set_theme_touched) = signal(false);
    // The shell's message for a theme that was not saved. It is shown until the next press of the button.
    let (theme_error, set_theme_error) = signal(None::<String>);

    let (mode, set_mode) = signal(Mode::Review);
    let (settings_open, set_settings_open) = signal(false);
    let (asked, set_asked) = signal(false);

    let (settings_view, set_settings_view) = signal(SettingsView::Loading);
    let read_settings = move || {
        set_settings_view.set(SettingsView::Loading);
        ipc::call(settings::COMMAND, move |outcome| {
            let view = settings::settle(outcome);
            if let SettingsView::Ready(saved) = &view
                && !theme_touched.get_untracked()
            {
                set_theme.set(Theme::from_setting(saved.theme, system_dark));
            }
            set_settings_view.set(view);
        });
    };
    read_settings();

    let (workspaces_view, set_workspaces_view) = signal(WorkspacesView::Loading);
    let read_workspaces = move || {
        set_workspaces_view.set(WorkspacesView::Loading);
        ipc::call_with(
            workspaces::COMMAND,
            &workspaces::first_page_payload(),
            move |outcome| {
                set_workspaces_view.set(workspaces::settle(outcome));
            },
        );
    };
    read_workspaces();

    Effect::new(move |_| dom::set_root_attribute("data-theme", theme.get().attribute()));

    // The theme changes at once and is saved with `settings_set`. A saved reply is the settings the shell holds, so
    // the settings area shows the theme that was saved; a refused save keeps the theme on screen and says so.
    let toggle_theme = move |_| {
        let next = theme.get_untracked().toggled();
        set_theme_touched.set(true);
        set_theme.set(next);
        set_theme_error.set(None);
        ipc::call_with(
            settings::SET_COMMAND,
            &settings::theme_request(next.setting()),
            move |outcome| match settings::saved(outcome) {
                Ok(view) => set_settings_view.set(view),
                Err(failure) => set_theme_error.set(Some(failure.message)),
            },
        );
    };
    let theme_button_label = move || match theme.get() {
        Theme::Light => "Usar tema escuro",
        Theme::Dark => "Usar tema claro",
    };

    let pressed = move |target: Mode| (!settings_open.get() && mode.get() == target).to_string();
    let open_mode = move |chosen: Mode| {
        set_mode.set(chosen);
        set_settings_open.set(false);
    };

    view! {
        <div class="app">
            <aside class="sidebar">
                <div class="sidebar__top">
                    <span class="brand">"NOCT"</span>
                    <button
                        class="icon-btn row"
                        type="button"
                        aria-label=theme_button_label
                        on:click=toggle_theme
                    >
                        {move || {
                            if theme.get() == Theme::Light {
                                moon_icon().into_any()
                            } else {
                                sun_icon().into_any()
                            }
                        }}
                    </button>
                </div>
                {move || {
                    theme_error.get().map(|message| {
                        view! {
                            <p class="note" role="alert" data-testid="theme-error">{message}</p>
                        }
                    })
                }}
                <nav class="nav" aria-label="Projetos">
                    <div class="nav__label">"Projetos"</div>
                    {move || projects_view(workspaces_view.get(), read_workspaces)}
                </nav>
                <div class="spacer"></div>
                <div class="sidebar__bottom">
                    <div class="status">
                        <div class="status__label">"Modelo"</div>
                        <div class="status__value">{EXAMPLE_MODEL}</div>
                        <div class="status__note">"Exemplo"</div>
                    </div>
                    <div class="status">
                        <div class="status__label">"Cofre de chaves"</div>
                        <div class="status__value">{EXAMPLE_VAULT}</div>
                        <div class="status__note">"Exemplo: ainda não lido do backend"</div>
                    </div>
                    <button
                        type="button"
                        class="nav__item nav__item--muted row"
                        aria-current=move || settings_open.get().then_some("page")
                        on:click=move |_| set_settings_open.set(true)
                    >
                        "Ajustes"
                    </button>
                </div>
            </aside>

            <div class="content">
                <header class="modes">
                    <div class="segmented" role="group" aria-label="Modo de trabalho">
                        <button
                            type="button"
                            class="seg"
                            aria-pressed=move || pressed(Mode::Review)
                            on:click=move |_| open_mode(Mode::Review)
                        >
                            "Revisar"
                        </button>
                        <button
                            type="button"
                            class="seg"
                            aria-pressed=move || pressed(Mode::Build)
                            on:click=move |_| open_mode(Mode::Build)
                        >
                            "Construir"
                        </button>
                        <button
                            type="button"
                            class="seg"
                            aria-pressed=move || pressed(Mode::Docs)
                            on:click=move |_| open_mode(Mode::Docs)
                        >
                            "Documentar"
                        </button>
                    </div>
                </header>

                <main class="main" id="main">
                    <div class="stack">
                        {move || {
                            if settings_open.get() {
                                settings_panel(settings_view, read_settings).into_any()
                            } else {
                                match mode.get() {
                                    Mode::Review => review_panel().into_any(),
                                    Mode::Build => unavailable_panel(
                                        "Construir",
                                        "Cada comando espera a sua aprovação. Toda alteração pode ser revertida.",
                                    )
                                    .into_any(),
                                    Mode::Docs => unavailable_panel(
                                        "Documentar",
                                        "Atualizações incrementais em Markdown, só depois da sua confirmação.",
                                    )
                                    .into_any(),
                                }
                            }
                        }}
                    </div>
                </main>

                <div class="composer-wrap">
                    <form
                        class="composer"
                        on:submit=move |event| {
                            event.prevent_default();
                            set_asked.set(true);
                        }
                    >
                        <label for="ask" class="sr-only">"Pergunte sobre o projeto"</label>
                        <input
                            id="ask"
                            class="composer__input"
                            type="text"
                            autocomplete="off"
                            placeholder="Pergunte sobre o projeto"
                        />
                        <button class="composer__send" type="submit" aria-label="Enviar">
                            {arrow_icon()}
                        </button>
                    </form>
                    <p class="note" aria-live="polite" data-testid="ask-note">
                        {move || {
                            if asked.get() {
                                "Perguntas ainda não são respondidas nesta fatia."
                            } else {
                                ""
                            }
                        }}
                    </p>
                </div>
            </div>
        </div>
    }
}

/// The sidebar's project list, or the state of its read: loading, nothing registered yet, the shell's error with
/// a way to try again, or the list itself.
fn projects_view(view: WorkspacesView, retry: impl Fn() + Copy + Send + Sync + 'static) -> AnyView {
    match view {
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
                    {page.items.iter().map(project_item).collect_view()}
                </ul>
                {more}
            }
            .into_any()
        }
    }
}

/// One project: its name, a tag when it is a favourite, a note when it cannot be used as it is, and its folder.
fn project_item(workspace: &Workspace) -> impl IntoView {
    let favorite = workspace.favorite.then_some("Favorito");
    let note = workspaces::note(workspace);
    view! {
        <li class="project row">
            <span class="project__name">{workspace.name.clone()}</span>
            {favorite.map(|label| view! { <span class="project__tag">{label}</span> })}
            {note.map(|text| view! { <span class="project__note">{text}</span> })}
            <span class="path project__path">{workspace.path.clone()}</span>
        </li>
    }
}

/// The review mode: read-only findings, each with its severity, its file and its description. EXAMPLE.
fn review_panel() -> impl IntoView {
    view! {
        <div class="stack">
            <p class="example-banner">"Dados de exemplo: nada é executado nem gravado nesta tela."</p>
            <div>
                <h1 class="h1">"Revisão"</h1>
                <p class="lead">"Análise somente leitura, com as evidências ao lado de cada achado."</p>
            </div>
            {EXAMPLE_FINDINGS
                .iter()
                .map(|finding| {
                    view! {
                        <article class="card">
                            <div class="finding__head">
                                <span class=finding.severity.class()>{finding.severity.label()}</span>
                                <h2 class="h2">{finding.title}</h2>
                            </div>
                            <code class="path">{finding.path}</code>
                            <p class="body">{finding.body}</p>
                        </article>
                    }
                })
                .collect_view()}
        </div>
    }
}

/// A mode the shell does not run yet. It says so, and offers no control for the work, so that nothing in it can be
/// approved, reverted or confirmed.
fn unavailable_panel(heading: &'static str, lead: &'static str) -> impl IntoView {
    view! {
        <div class="stack" data-testid="unavailable">
            <div>
                <h1 class="h1">{heading}</h1>
                <p class="lead">{lead}</p>
            </div>
            <section class="card">
                <p class="body" role="status">
                    "Ainda não disponível nesta versão. Nada é executado nem gravado nesta tela."
                </p>
            </section>
        </div>
    }
}

fn settings_panel(
    settings_view: ReadSignal<SettingsView>,
    retry: impl Fn() + Copy + Send + Sync + 'static,
) -> impl IntoView {
    view! {
        <div class="stack" data-testid="settings">
            <div>
                <h1 class="h1">"Ajustes"</h1>
                <p class="lead">"Preferências salvas. O NOCT lê estes valores ao abrir a janela."</p>
            </div>
            <section class="card" aria-label="Preferências salvas">
                {move || match settings_view.get() {
                    SettingsView::Loading => view! {
                        <p class="lead" role="status" data-testid="settings-loading">
                            "Carregando configurações…"
                        </p>
                    }
                    .into_any(),
                    SettingsView::Failed(failure) => view! {
                        <div class="error-card" role="alert" data-testid="settings-error">
                            <div>
                                <strong>"Não foi possível carregar as configurações."</strong>
                                <p>{failure.message}</p>
                            </div>
                            <button class="btn" type="button" on:click=move |_| retry()>
                                "Tentar novamente"
                            </button>
                        </div>
                    }
                    .into_any(),
                    SettingsView::Ready(saved) => view! {
                        <dl class="settings-facts" data-testid="settings-values">
                            {settings::rows(&saved)
                                .into_iter()
                                .map(|(label, value)| view! { <dt>{label}</dt><dd>{value}</dd> })
                                .collect_view()}
                        </dl>
                    }
                    .into_any(),
                }}
            </section>
            <p class="note">
                "O botão de tema da barra lateral muda só esta sessão. A linha Tema mostra a preferência salva."
            </p>
        </div>
    }
}

fn moon_icon() -> impl IntoView {
    view! {
        <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
            <path d="M21 12.8A9 9 0 1 1 11.2 3a7 7 0 0 0 9.8 9.8z" />
        </svg>
    }
}

fn sun_icon() -> impl IntoView {
    view! {
        <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
            <circle cx="12" cy="12" r="4" />
            <path d="M12 2v2M12 20v2M4.93 4.93l1.41 1.41M17.66 17.66l1.41 1.41M2 12h2M20 12h2M4.93 19.07l1.41-1.41M17.66 6.34l1.41-1.41" />
        </svg>
    }
}

fn arrow_icon() -> impl IntoView {
    view! {
        <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
            <path d="M12 19V5M5 12l7-7 7 7" />
        </svg>
    }
}
