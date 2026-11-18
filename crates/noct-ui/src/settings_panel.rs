//! The settings area (Ajustes), in groups: the model and the projects with permission, the appearance, and the advanced
//! settings. Each change of a setting is saved at once through `settings_set`, and the shell's reply is what is shown. The
//! model and the permissions are in `provider_panel.rs`.

use leptos::prelude::*;

use crate::dom;
use crate::provider_panel;
use crate::settings::{self, SettingsView};
use crate::sidebar::Shared;
use crate::theme::Theme;

/// The theme for an option of the appearance select.
fn theme_choice(value: &str) -> Option<Theme> {
    match value {
        "dark" => Some(Theme::Dark),
        "light" => Some(Theme::Light),
        _ => None,
    }
}

/// Turns the glass layer on or off for this viewer, and remembers the choice.
fn toggle_glass(glass: RwSignal<bool>) {
    let next = !glass.get_untracked();
    glass.set(next);
    dom::store_glass(next);
}

#[allow(clippy::too_many_arguments)]
pub fn settings_panel(
    settings_view: ReadSignal<SettingsView>,
    saving: ReadSignal<bool>,
    save_error: ReadSignal<Option<String>>,
    save: impl Fn(String) + Copy + Send + Sync + 'static,
    retry: impl Fn() + Copy + Send + Sync + 'static,
    shared: Shared,
    theme: ReadSignal<Theme>,
    choose_theme: impl Fn(Theme) + Copy + Send + Sync + 'static,
    glass: RwSignal<bool>,
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
            </div>
            {move || match settings_view.get() {
                SettingsView::Loading => view! {
                    <p class="lead" role="status" data-testid="settings-loading">"Carregando configurações…"</p>
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

            {provider_panel::model_group(shared)}
            {provider_panel::permissions_group(shared)}

            <div class="group">
                <div class="group__title">"Aparência"</div>
                <div class="list">
                    <div class="item">
                        <div class="item__txt">
                            <label class="item__name" for="setting-theme">"Tema"</label>
                        </div>
                        <select
                            id="setting-theme"
                            class="select"
                            prop:value=move || theme.get().attribute()
                            on:change=move |event| {
                                if let Some(next) = theme_choice(&event_target_value(&event)) {
                                    choose_theme(next);
                                }
                            }
                        >
                            <option value="dark">"Escuro"</option>
                            <option value="light">"Claro"</option>
                        </select>
                    </div>
                    <div class="item">
                        <div class="item__txt">
                            <div class="item__name">"Efeito de vidro"</div>
                            <div class="item__desc">"Desligue se a janela ficar lenta."</div>
                        </div>
                        <button
                            class="switch"
                            type="button"
                            role="switch"
                            aria-label="Efeito de vidro"
                            aria-checked=move || glass.get().to_string()
                            on:click=move |_| toggle_glass(glass)
                        ></button>
                    </div>
                </div>
            </div>

            <div class="group">
                <div class="group__title">"Avançado"</div>
                <div class="list" data-testid="settings-values" hidden=move || !ready()>
                    <div class="item">
                        <div class="item__txt">
                            <label class="item__name" for="setting-language">"Idioma"</label>
                        </div>
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
                    </div>
                    <div class="item">
                        <div class="item__txt">
                            <label class="item__name" for="setting-approval">"Segurança do agente"</label>
                        </div>
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
                    </div>
                    <div class="item">
                        <div class="item__txt">
                            <label class="item__name" for="setting-diagnostic">"Diagnóstico"</label>
                        </div>
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
                    </div>
                    <div class="item">
                        <div class="item__txt">
                            <div class="item__name">"Pandoc"</div>
                        </div>
                        <span class="body">
                            {move || {
                                saved().map_or(settings::NOT_INFORMED.to_string(), |settings| {
                                    settings::pandoc_label(&settings)
                                })
                            }}
                        </span>
                    </div>
                </div>
                {move || {
                    saving.get().then(|| {
                        view! { <p class="note" role="status" data-testid="settings-saving">"Salvando…"</p> }
                    })
                }}
                {move || {
                    save_error.get().map(|message| {
                        view! { <p class="note" role="alert" data-testid="settings-save-error">{message}</p> }
                    })
                }}
            </div>
        </div>
    }
}
