//! Spike screen: the topbar and composer shell, rendered with the existing class names
//! so the Leptos build can be judged against the React design. Phase 7 replaces it.

use leptos::prelude::*;

#[component]
pub fn App() -> impl IntoView {
    let (presses, set_presses) = signal(0_u32);
    let (draft, set_draft) = signal(String::new());
    let characters = move || draft.with(|text| text.chars().count());

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
    }
}
