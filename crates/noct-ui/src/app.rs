//! The window: the sidebar (`sidebar.rs`), the three modes, the settings (Ajustes) and, in Revisar, the conversation with
//! the chat (`chat_panel.rs`), laid out as in docs/New_Design/mockup-v2.html. The sidebar's projects come from
//! `workspaces_list` (`workspaces.rs`) and the settings from `settings_get` (`settings.rs`). The build and document modes
//! are not available yet: they say so and offer no control for the work, so nothing in this window runs a command or
//! writes a file.

use leptos::prelude::*;

use crate::chat_panel;
use crate::dom;
use crate::ipc;
use crate::provider::ProviderView;
use crate::provider_panel;
use crate::settings::{self, SettingsView};
use crate::sidebar::{self, Shared};
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
    let shared = Shared {
        current: RwSignal::new(None),
        projects: RwSignal::new(WorkspacesView::Loading),
        settings_open: RwSignal::new(false),
        key: RwSignal::new(ProviderView::Loading),
        bindings: RwSignal::new(None),
    };
    let settings_open = shared.settings_open;

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

    // The chat switch is read once. Until the shell answers, the chat section says it is loading.
    let chat = chat_panel::ChatState::new(shared);
    chat.read_status();

    // A change in Ajustes is saved at once, with the one setting that changed, and the area then shows the settings the
    // shell replied with. A refused save says so, and its controls go back to the settings the shell holds.
    let (saving, set_saving) = signal(false);
    let (save_error, set_save_error) = signal(None::<String>);
    let save_setting = move |payload: String| {
        set_saving.set(true);
        set_save_error.set(None);
        ipc::call_with(settings::SET_COMMAND, &payload, move |outcome| {
            set_saving.set(false);
            match settings::saved(outcome) {
                Ok(view) => set_settings_view.set(view),
                Err(failure) => {
                    set_save_error.set(Some(failure.message));
                    set_settings_view.update(|_| {});
                }
            }
        });
    };

    let read_workspaces = move || {
        shared.projects.set(WorkspacesView::Loading);
        ipc::call_with(
            workspaces::COMMAND,
            &workspaces::first_page_payload(),
            move |outcome| {
                shared.projects.set(workspaces::settle(outcome));
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
        shared.close_settings();
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
                {sidebar::sidebar(shared, read_workspaces)}
            </aside>

            <div class="content">
                <header class="topbar">
                    <div class="crumb" data-testid="crumb">
                        {move || crumb_view(shared, mode.get(), settings_open.get())}
                    </div>
                    <div class="modes">
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
                    </div>
                    <div class=move || {
                        if settings_open.get() || mode.get() != Mode::Review {
                            "topbar__end is-hidden"
                        } else {
                            "topbar__end"
                        }
                    }>
                        <button
                            class="icon-btn row"
                            type="button"
                            aria-label="Nova conversa"
                            title="Nova conversa"
                            on:click=move |_| chat.new_conversation()
                        >
                            {pencil_icon()}
                        </button>
                    </div>
                </header>

                <main class="main" id="main">
                    <div class="stack">
                        {move || {
                            if settings_open.get() {
                                settings_panel(settings_view, saving, save_error, save_setting, read_settings, shared).into_any()
                            } else {
                                match mode.get() {
                                    Mode::Review => chat_panel::thread(chat).into_any(),
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
                {move || {
                    (mode.get() == Mode::Review && !settings_open.get())
                        .then(|| chat_panel::composer(chat))
                }}
            </div>
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
    saving: ReadSignal<bool>,
    save_error: ReadSignal<Option<String>>,
    save: impl Fn(String) + Copy + Send + Sync + 'static,
    retry: impl Fn() + Copy + Send + Sync + 'static,
    shared: Shared,
) -> impl IntoView {
    let ready = move || matches!(settings_view.get(), SettingsView::Ready(_));
    let saved = move || match settings_view.get() {
        SettingsView::Ready(settings) => Some(settings),
        SettingsView::Loading | SettingsView::Failed(_) => None,
    };
    view! {
        <div class="stack" data-testid="settings">
            <div>
                <h1 class="h1">"Ajustes"</h1>
                <p class="lead">
                    "Preferências salvas. Cada mudança é salva ao acontecer, e o NOCT lê estes valores ao abrir a janela."
                </p>
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
                    SettingsView::Ready(_) => ().into_any(),
                }}
                <dl class="settings-facts" data-testid="settings-values" hidden=move || !ready()>
                    <dt>"Tema"</dt>
                    <dd>{move || settings::theme_label(saved().and_then(|settings| settings.theme))}</dd>
                    <dt><label for="setting-language">"Idioma"</label></dt>
                    <dd>
                        <select
                            id="setting-language"
                            class="select"
                            disabled=move || !ready()
                            prop:value=move || {
                                saved()
                                    .and_then(|settings| settings.language)
                                    .map_or("", |language| {
                                        settings::choice_value(&settings::LANGUAGE_CHOICES, language)
                                    })
                            }
                            on:change=move |event| {
                                let choice = settings::choice_of(
                                    &settings::LANGUAGE_CHOICES,
                                    &event_target_value(&event),
                                );
                                if let Some(language) = choice {
                                    save(settings::language_request(language));
                                }
                            }
                        >
                            <option value="" disabled>{settings::NOT_INFORMED}</option>
                            {settings::LANGUAGE_CHOICES
                                .iter()
                                .map(|(_, value, label)| view! { <option value=*value>{*label}</option> })
                                .collect_view()}
                        </select>
                    </dd>
                    <dt><label for="setting-approval">"Segurança do agente"</label></dt>
                    <dd>
                        <select
                            id="setting-approval"
                            class="select"
                            disabled=move || !ready()
                            prop:value=move || {
                                saved().map_or("", |settings| {
                                    settings::choice_value(&settings::APPROVAL_CHOICES, settings.approval_policy)
                                })
                            }
                            on:change=move |event| {
                                let choice = settings::choice_of(
                                    &settings::APPROVAL_CHOICES,
                                    &event_target_value(&event),
                                );
                                if let Some(policy) = choice {
                                    save(settings::approval_request(policy));
                                }
                            }
                        >
                            {settings::APPROVAL_CHOICES
                                .iter()
                                .map(|(_, value, label)| view! { <option value=*value>{*label}</option> })
                                .collect_view()}
                        </select>
                    </dd>
                    <dt><label for="setting-diagnostic">"Diagnóstico"</label></dt>
                    <dd>
                        <select
                            id="setting-diagnostic"
                            class="select"
                            disabled=move || !ready()
                            prop:value=move || {
                                saved()
                                    .and_then(|settings| settings.diagnostic_mode)
                                    .map_or("", |enabled| {
                                        settings::choice_value(&settings::DIAGNOSTIC_CHOICES, enabled)
                                    })
                            }
                            on:change=move |event| {
                                let choice = settings::choice_of(
                                    &settings::DIAGNOSTIC_CHOICES,
                                    &event_target_value(&event),
                                );
                                if let Some(enabled) = choice {
                                    save(settings::diagnostic_request(enabled));
                                }
                            }
                        >
                            <option value="" disabled>{settings::NOT_INFORMED}</option>
                            {settings::DIAGNOSTIC_CHOICES
                                .iter()
                                .map(|(_, value, label)| view! { <option value=*value>{*label}</option> })
                                .collect_view()}
                        </select>
                    </dd>
                    <dt>"Pandoc"</dt>
                    <dd>
                        {move || {
                            saved().map_or(settings::NOT_INFORMED.to_string(), |settings| {
                                settings::pandoc_label(&settings)
                            })
                        }}
                    </dd>
                </dl>
                {move || {
                    saving.get().then(|| {
                        view! {
                            <p class="note" role="status" data-testid="settings-saving">"Salvando…"</p>
                        }
                    })
                }}
                {move || {
                    save_error.get().map(|message| {
                        view! {
                            <p class="note" role="alert" data-testid="settings-save-error">{message}</p>
                        }
                    })
                }}
            </section>
            <p class="note">"O tema muda pelo botão da barra lateral, que o troca e o salva."</p>
            {provider_panel::provider_section(shared, shared.projects.read_only())}
        </div>
    }
}

/// The trail of the top bar: the current project and the mode, or only Ajustes while they are open.
fn crumb_view(shared: Shared, mode: Mode, settings_open: bool) -> AnyView {
    if settings_open {
        return view! { <b>"Ajustes"</b> }.into_any();
    }
    let project = current_name(shared).unwrap_or_else(|| "Nenhum projeto".to_string());
    let mode_label = match mode {
        Mode::Review => "Revisar",
        Mode::Build => "Construir",
        Mode::Docs => "Documentar",
    };
    view! { <b>{project}</b> " · " {mode_label} }.into_any()
}

/// The name of the current project, once the list has arrived.
fn current_name(shared: Shared) -> Option<String> {
    let path = shared.current.get()?;
    match shared.projects.get() {
        WorkspacesView::Ready(page) => page
            .items
            .iter()
            .find(|workspace| workspace.path == path)
            .map(|workspace| workspace.name.clone()),
        WorkspacesView::Loading | WorkspacesView::Failed(_) => None,
    }
}

fn pencil_icon() -> impl IntoView {
    view! {
        <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
            <path d="M12 20h9" />
            <path d="M16.5 3.5a2.1 2.1 0 0 1 3 3L7 19l-4 1 1-4z" />
        </svg>
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
