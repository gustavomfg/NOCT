//! The model picker: OpenRouter's list, searched by name or id, with a "Só gratuitos" filter that starts on. Choosing a
//! model saves it as the default of the current project with `models_set_default_model`. Ajustes shows the picker in the
//! group Modelo, and the composer's model pill opens it over the composer.
//!
//! The list is read once per window with `models_list` (the shell keeps it an hour) and lives in `Shared`, so the two
//! places show the same list. A list that could not be read says why, and the caller offers the field to type an id.
//!
//! Keyboard: the search box holds the focus; the arrows move through the list, Enter chooses, Esc closes.

use leptos::prelude::*;
use leptos::web_sys::KeyboardEvent;
use noct_core::ai::openrouter::validated_model_id;
use serde::Deserialize;

use crate::dom;
use crate::ipc::{self, Failure, Outcome};
use crate::provider::{self, ModelBindingsView, ProviderView};
use crate::sidebar::Shared;

/// The command that reads the list.
pub const LIST: &str = "models_list";
/// The most models drawn at once. A longer match says how many there are, and asks to refine the search.
const SHOWN: usize = 100;

/// One model, as the shell sends it.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct CatalogItem {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub context_length: Option<u64>,
    pub free: bool,
}

/// What the picker knows of the list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CatalogView {
    Idle,
    Loading,
    Ready(Vec<CatalogItem>),
    Failed(Failure),
}

/// Reads a reply of `models_list`. An entry whose id is not a model id is dropped here too.
pub fn settle(outcome: Outcome) -> CatalogView {
    match ipc::reply(outcome) {
        Ok(text) => match serde_json::from_str::<Vec<CatalogItem>>(&text) {
            Ok(items) => CatalogView::Ready(
                items
                    .into_iter()
                    .filter(|item| validated_model_id(&item.id).is_ok())
                    .collect(),
            ),
            Err(_) => CatalogView::Failed(ipc::invalid_response()),
        },
        Err(failure) => CatalogView::Failed(failure),
    }
}

/// Reads the list, unless it is read or being read.
pub fn load(shared: Shared) {
    if matches!(
        shared.catalog.get_untracked(),
        CatalogView::Loading | CatalogView::Ready(_)
    ) {
        return;
    }
    shared.catalog.set(CatalogView::Loading);
    ipc::call(LIST, move |outcome| shared.catalog.set(settle(outcome)));
}

/// Reads the list again, after a failure.
pub fn reload(shared: Shared) {
    if matches!(shared.catalog.get_untracked(), CatalogView::Failed(_)) {
        shared.catalog.set(CatalogView::Idle);
    }
    load(shared);
}

/// The context window in words, such as "128k" or "1M".
pub fn context_words(tokens: u64) -> String {
    if tokens >= 1_000_000 {
        let millions = tokens as f64 / 1_000_000.0;
        let text = format!("{millions:.1}");
        format!("{}M", text.trim_end_matches(".0"))
    } else if tokens >= 1_000 {
        format!("{}k", (tokens + 500) / 1_000)
    } else {
        tokens.to_string()
    }
}

/// Whether a model is shown for the search and the filter. The search is the lowercase text typed.
pub fn matches(item: &CatalogItem, search: &str, free_only: bool) -> bool {
    (!free_only || item.free)
        && (search.is_empty()
            || item.name.to_lowercase().contains(search)
            || item.id.to_lowercase().contains(search))
}

/// Saves the default model of the current project, or clears it with `None`. The reply updates the model the sidebar and
/// the composer show. `done` is told the shell's message when the save is refused.
pub fn save_default_model(
    shared: Shared,
    model: Option<String>,
    done: impl FnOnce(Result<(), String>) + 'static,
) {
    let Some(path) = shared.current.get_untracked() else {
        done(Err(
            "Adicione um projeto antes de escolher o modelo.".to_string()
        ));
        return;
    };
    ipc::call_with(
        provider::MODEL_SET,
        &provider::model_set_payload(&path, model.as_deref()),
        move |outcome| match provider::settle::<ModelBindingsView>(outcome) {
            ProviderView::Ready(bindings) => {
                if shared.current.get_untracked().as_deref() == Some(path.as_str()) {
                    shared
                        .bindings
                        .set(Some(ProviderView::Ready(Some(bindings))));
                }
                done(Ok(()));
            }
            ProviderView::Failed(failure) => done(Err(failure.message)),
            ProviderView::Loading => {}
        },
    );
}

/// The id of the model saved for the current project, or empty.
fn saved_id(shared: Shared) -> String {
    shared.bindings.with(|view| match view {
        Some(ProviderView::Ready(Some(bindings))) => bindings
            .default_binding
            .as_ref()
            .map(|model| model.model_id.clone())
            .unwrap_or_default(),
        _ => String::new(),
    })
}

/// The picker. `id` names its elements, so two pickers never share an id. `on_done` is called with `true` after a model
/// was saved, and with `false` when the person pressed Esc.
pub fn picker(
    shared: Shared,
    id: &'static str,
    on_done: impl Fn(bool) + Copy + Send + Sync + 'static,
) -> impl IntoView {
    load(shared);
    let search = RwSignal::new(String::new());
    let free_only = RwSignal::new(true);
    let active = RwSignal::new(0usize);
    let busy = RwSignal::new(false);
    let error = RwSignal::new(None::<String>);
    let list_id = format!("{id}-list");
    let option_id = move |index: usize| format!("{id}-option-{index}");

    // The models the search and the filter leave, and how many there are in all.
    let shown = Memo::new(move |_| {
        let wanted = search.get().trim().to_lowercase();
        let free = free_only.get();
        shared.catalog.with(|view| match view {
            CatalogView::Ready(items) => {
                let all: Vec<&CatalogItem> = items
                    .iter()
                    .filter(|item| matches(item, &wanted, free))
                    .collect();
                let total = all.len();
                (
                    all.into_iter().take(SHOWN).cloned().collect::<Vec<_>>(),
                    total,
                )
            }
            _ => (Vec::new(), 0),
        })
    });
    // A new search or filter starts at the first model.
    Effect::new(move |_| {
        let _ = search.get();
        let _ = free_only.get();
        active.set(0);
    });

    let choose = move |model: String| {
        if busy.get_untracked() {
            return;
        }
        busy.set(true);
        error.set(None);
        save_default_model(shared, Some(model), move |result| {
            busy.set(false);
            match result {
                Ok(()) => on_done(true),
                Err(message) => error.set(Some(message)),
            }
        });
    };

    let list_for_keys = list_id.clone();
    let on_key = move |event: KeyboardEvent| {
        let count = shown.with_untracked(|(items, _)| items.len());
        match event.key().as_str() {
            "ArrowDown" if count > 0 => {
                event.prevent_default();
                active.update(|index| *index = (*index + 1).min(count - 1));
                dom::keep_in_view(&list_for_keys, &option_id(active.get_untracked()));
            }
            "ArrowUp" if count > 0 => {
                event.prevent_default();
                active.update(|index| *index = index.saturating_sub(1));
                dom::keep_in_view(&list_for_keys, &option_id(active.get_untracked()));
            }
            "Enter" => {
                event.prevent_default();
                let chosen = shown.with_untracked(|(items, _)| {
                    items
                        .get(active.get_untracked())
                        .map(|item| item.id.clone())
                });
                if let Some(model) = chosen {
                    choose(model);
                }
            }
            "Escape" => {
                event.prevent_default();
                on_done(false);
            }
            _ => {}
        }
    };

    let list_attr = list_id.clone();
    view! {
        <div class="picker" data-testid="model-picker">
            <div class="picker__bar">
                <input
                    class="field picker__search"
                    type="search"
                    role="combobox"
                    aria-label="Buscar modelo por nome ou id"
                    aria-autocomplete="list"
                    aria-expanded="true"
                    aria-controls=list_attr.clone()
                    aria-activedescendant=move || {
                        (shown.with(|(items, _)| !items.is_empty())).then(|| option_id(active.get()))
                    }
                    autocomplete="off"
                    spellcheck="false"
                    placeholder="Buscar por nome ou id"
                    autofocus=true
                    prop:value=move || search.get()
                    on:input=move |event| search.set(event_target_value(&event))
                    on:keydown=on_key
                />
                <button
                    class="toggle"
                    type="button"
                    data-testid="free-only"
                    aria-pressed=move || free_only.get().to_string()
                    on:click=move |_| free_only.update(|on| *on = !*on)
                >
                    "Só gratuitos"
                </button>
            </div>
            {move || match shared.catalog.get() {
                CatalogView::Idle | CatalogView::Loading => view! {
                    <p class="note picker__state" role="status" data-testid="models-loading">"Carregando modelos do OpenRouter…"</p>
                }
                .into_any(),
                CatalogView::Failed(failure) => view! {
                    <p class="note note--error picker__state" role="alert" data-testid="models-error">{failure.message}</p>
                }
                .into_any(),
                CatalogView::Ready(_) if shown.with(|(_, total)| *total == 0) => view! {
                    <p class="note picker__state" role="status" data-testid="models-empty">
                        {move || if free_only.get() {
                            "Nenhum modelo gratuito encontrado. Desligue \"Só gratuitos\" para ver todos."
                        } else {
                            "Nenhum modelo encontrado."
                        }}
                    </p>
                }
                .into_any(),
                CatalogView::Ready(_) => ().into_any(),
            }}
            <ul class="picker__list" id=list_attr role="listbox" aria-label="Modelos do OpenRouter">
                {move || {
                    let saved = saved_id(shared);
                    shown
                        .get()
                        .0
                        .into_iter()
                        .enumerate()
                        .map(|(index, item)| {
                            let chosen = item.id.clone();
                            let context = item.context_length.map(context_words);
                            view! {
                                <li
                                    id=option_id(index)
                                    class="picker__option"
                                    role="option"
                                    aria-selected=(item.id == saved).to_string()
                                    data-active=move || (active.get() == index).to_string()
                                    on:mousemove=move |_| {
                                        if active.get_untracked() != index {
                                            active.set(index);
                                        }
                                    }
                                    on:click=move |_| choose(chosen.clone())
                                >
                                    <span class="picker__head">
                                        <span class="picker__name">{item.name.clone()}</span>
                                        {item.free.then(|| view! { <span class="tag">"Grátis"</span> })}
                                    </span>
                                    <span class="picker__meta">
                                        <span class="mono">{item.id.clone()}</span>
                                        {context.map(|words| view! { <span>{format!("· {words}")}</span> })}
                                    </span>
                                </li>
                            }
                        })
                        .collect_view()
                }}
            </ul>
            {move || {
                let (shown_count, total) = shown.with(|(items, total)| (items.len(), *total));
                (total > shown_count).then(|| {
                    view! {
                        <p class="note picker__state">
                            {format!("Mostrando {shown_count} de {total}. Refine a busca.")}
                        </p>
                    }
                })
            }}
            {move || {
                error.get().map(|message| {
                    view! { <p class="note note--error picker__state" role="alert" data-testid="picker-error">{message}</p> }
                })
            }}
        </div>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(id: &str, name: &str, free: bool) -> CatalogItem {
        CatalogItem {
            id: id.into(),
            name: name.into(),
            context_length: None,
            free,
        }
    }

    #[test]
    fn the_context_reads_as_thousands_or_millions() {
        assert_eq!(context_words(128_000), "128k");
        assert_eq!(context_words(131_072), "131k");
        assert_eq!(context_words(1_000_000), "1M");
        assert_eq!(context_words(1_048_576), "1M");
        assert_eq!(context_words(2_500_000), "2.5M");
        assert_eq!(context_words(512), "512");
    }

    #[test]
    fn the_search_reads_the_name_and_the_id_and_the_filter_keeps_free_models() {
        let free = item("deepseek/deepseek-chat-v3:free", "DeepSeek V3", true);
        let paid = item("openai/gpt-4o", "GPT-4o", false);
        assert!(matches(&free, "deepseek", true));
        assert!(matches(&free, "v3", true));
        assert!(
            !matches(&paid, "gpt", true),
            "a paid model is hidden while the filter is on"
        );
        assert!(matches(&paid, "gpt", false));
        assert!(matches(&paid, "openai/", false));
        assert!(matches(&paid, "", false));
    }

    #[test]
    fn a_reply_with_an_unknown_field_or_a_bad_id_shows_no_bad_model() {
        let ok = settle(Outcome::Resolved(
            r#"[{"id":"v/a","name":"A","free":true},{"id":"<b>","name":"x","free":true}]"#.into(),
        ));
        assert_eq!(ok, CatalogView::Ready(vec![item("v/a", "A", true)]));
        let extra = settle(Outcome::Resolved(
            r#"[{"id":"v/a","name":"A","free":true,"html":"<b>"}]"#.into(),
        ));
        assert!(matches!(extra, CatalogView::Failed(_)));
    }
}
