//! Spike screen: the topbar and composer shell, rendered with the existing class names
//! so the Leptos build can be judged against the React design. Phase 7 replaces it.
//!
//! The settings area is the first part that reads the shell: it calls `settings_get` when the page
//! opens, and shows a loading state, the settings, or the error with a way to try again.

use leptos::prelude::*;

use crate::ipc;
use crate::settings::{self, SettingsView};

#[component]
pub fn App() -> impl IntoView {
    let (presses, set_presses) = signal(0_u32);
    let (draft, set_draft) = signal(String::new());
    let characters = move || draft.with(|text| text.chars().count());

    let (settings_view, set_settings_view) = signal(SettingsView::Loading);
    let read_settings = move || {
        set_settings_view.set(SettingsView::Loading);
        ipc::call(settings::COMMAND, move |outcome| {
            set_settings_view.set(settings::settle(outcome));
        });
    };
    read_settings();

    view! {
        <header class="topbar">
            <div class="title-block"><h1>"NOCT"</h1></div>
            <div class="top-actions">
                <span class="branch-pill" data-testid="presses">
                    {move || format!("{} toques", presses.get())}
                </span>
            </div>
        </header>
        <main class="composer-wrap" id="leptos-spike">
            <button
                class="icon-button"
                type="button"
                on:click=move |_| set_presses.update(|count| *count += 1)
            >
                "Tocar"
            </button>
            <textarea
                aria-label="Mensagem"
                prop:value=move || draft.get()
                on:input=move |event| set_draft.set(event_target_value(&event))
            ></textarea>
            <p class="mode-description" data-testid="characters">
                {move || format!("{} caracteres", characters())}
            </p>
        </main>
        <section class="settings-section" aria-labelledby="settings-heading" data-testid="settings">
            <h3 id="settings-heading">"Configurações"</h3>
            {move || match settings_view.get() {
                SettingsView::Loading => view! {
                    <p class="mode-description" role="status" data-testid="settings-loading">
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
                        <span>
                            <button
                                class="icon-button"
                                type="button"
                                on:click=move |_| read_settings()
                            >
                                "Tentar novamente"
                            </button>
                        </span>
                    </div>
                }
                .into_any(),
                SettingsView::Ready(settings) => view! {
                    <dl class="settings-facts" data-testid="settings-values">
                        {settings::rows(&settings)
                            .into_iter()
                            .map(|(label, value)| view! { <dt>{label}</dt><dd>{value}</dd> })
                            .collect_view()}
                    </dl>
                }
                .into_any(),
            }}
        </section>
    }
}
