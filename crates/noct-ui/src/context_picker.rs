//! What a provider call carries, and "Escolher arquivos".
//!
//! The shell sends the project tree (names and sizes, no content) and the text of the chosen files; the choice is kept by
//! the shell per project (`context_tree`, `context_selection_set`). The line over the composer and the head of Sugestões
//! say what goes before anything is sent. The dialog lists the tree with a search and boxes to mark; a sensitive or
//! ignored file is shown blocked, and a file that does not fit the caps (32 KiB each, 128 KiB in all) cannot be marked.

use leptos::ev::KeyboardEvent;
use leptos::prelude::*;
use noct_core::limits::provider_context::{MAXIMUM_CONTEXT_BYTES, MAXIMUM_FILE_BYTES};
use serde::Deserialize;
use serde_json::json;

use crate::dom;
use crate::ipc;
use crate::sidebar::Shared;

const TREE: &str = "context_tree";
const SET: &str = "context_selection_set";
/// The most rows the dialog draws at once; the search narrows the rest.
const SHOWN_ROWS: usize = 300;

const SEARCH_ID: &str = "ctx-search";
const SAVE_ID: &str = "ctx-save";

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Entry {
    pub path: String,
    pub bytes: u64,
    pub blocked: bool,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Tree {
    pub entries: Vec<Entry>,
    pub truncated: bool,
    pub selected: Vec<String>,
    pub custom: bool,
}

impl Tree {
    fn bytes_of(&self, paths: &[String]) -> u64 {
        self.entries
            .iter()
            .filter(|entry| paths.contains(&entry.path))
            .map(|entry| entry.bytes)
            .sum()
    }

    fn names(&self) -> usize {
        self.entries.iter().filter(|entry| !entry.blocked).count()
    }
}

#[derive(Debug, Clone, PartialEq)]
enum TreeView {
    None,
    Loading,
    Ready(Tree),
    Failed(String),
}

fn kib(bytes: u64) -> String {
    let value = bytes as f64 / 1024.0;
    if value < 10.0 && bytes > 0 {
        format!("{value:.1}").replace('.', ",")
    } else {
        format!("{}", value.round() as u64)
    }
}

/// The state of the window: the tree of the current project, and the dialog.
#[derive(Clone, Copy)]
pub struct ContextState {
    shared: Shared,
    tree: RwSignal<TreeView>,
    of: RwSignal<Option<String>>,
    open: RwSignal<bool>,
    draft: RwSignal<Vec<String>>,
    query: RwSignal<String>,
    busy: RwSignal<bool>,
    error: RwSignal<Option<String>>,
    /// The button that opened the dialog, which takes the focus back when it closes.
    trigger: RwSignal<&'static str>,
}

impl ContextState {
    pub fn new(shared: Shared) -> Self {
        Self {
            shared,
            tree: RwSignal::new(TreeView::None),
            of: RwSignal::new(None),
            open: RwSignal::new(false),
            draft: RwSignal::new(Vec::new()),
            query: RwSignal::new(String::new()),
            busy: RwSignal::new(false),
            error: RwSignal::new(None),
            trigger: RwSignal::new(""),
        }
    }

    /// Reads the tree of the current project whenever the project changes.
    pub fn watch(self) {
        Effect::new(move |_| {
            let current = self.shared.current.get();
            self.of.set(current.clone());
            self.open.set(false);
            let Some(workspace) = current else {
                self.tree.set(TreeView::None);
                return;
            };
            self.tree.set(TreeView::Loading);
            let payload = json!({ "input": { "workspace": workspace } }).to_string();
            ipc::call_with(TREE, &payload, move |outcome| {
                if self.of.get_untracked().as_deref() == Some(workspace.as_str()) {
                    self.tree.set(settle(outcome));
                }
            });
        });
    }

    fn show(self, trigger: &'static str) {
        let Some(selected) = self.tree.with_untracked(|tree| match tree {
            TreeView::Ready(tree) => Some(tree.selected.clone()),
            _ => None,
        }) else {
            return;
        };
        self.draft.set(selected);
        self.query.set(String::new());
        self.error.set(None);
        self.trigger.set(trigger);
        self.open.set(true);
        dom::next_frame(|| dom::focus(SEARCH_ID));
    }

    fn close(self) {
        self.open.set(false);
        let trigger = self.trigger.get_untracked();
        dom::next_frame(move || dom::focus(trigger));
    }

    fn toggle(self, entry: &Entry) {
        let fits = self.tree.with_untracked(|tree| match tree {
            TreeView::Ready(tree) => {
                let total = self.draft.with_untracked(|draft| tree.bytes_of(draft));
                !entry.blocked
                    && entry.bytes <= MAXIMUM_FILE_BYTES
                    && total + entry.bytes <= MAXIMUM_CONTEXT_BYTES
            }
            _ => false,
        });
        self.draft.update(|draft| {
            if let Some(index) = draft.iter().position(|path| path == &entry.path) {
                draft.remove(index);
            } else if fits {
                draft.push(entry.path.clone());
            }
        });
    }

    /// Keeps the draft, or goes back to the default with `None`.
    fn save(self, paths: Option<Vec<String>>) {
        let Some(workspace) = self.of.get_untracked() else {
            return;
        };
        self.busy.set(true);
        self.error.set(None);
        let payload = json!({ "input": { "workspace": workspace, "paths": paths } }).to_string();
        ipc::call_with(SET, &payload, move |outcome| {
            self.busy.set(false);
            match settle(outcome) {
                TreeView::Ready(tree) => {
                    if self.of.get_untracked().as_deref() == Some(workspace.as_str()) {
                        self.tree.set(TreeView::Ready(tree));
                    }
                    self.close();
                }
                TreeView::Failed(message) => self.error.set(Some(message)),
                _ => {}
            }
        });
    }
}

fn settle(outcome: ipc::Outcome) -> TreeView {
    match ipc::reply(outcome) {
        Ok(text) => match serde_json::from_str::<Tree>(&text) {
            Ok(tree) => TreeView::Ready(tree),
            Err(_) => TreeView::Failed(ipc::UNEXPECTED_MESSAGE.to_string()),
        },
        Err(failure) => TreeView::Failed(failure.message),
    }
}

/// The line that says what a call carries, with the button that opens the dialog. `id` is the button's id.
pub fn summary(state: ContextState, id: &'static str) -> impl IntoView {
    view! {
        <div class="ctx-line" data-testid="context-summary">
            <span class="ctx-line__words" role="status">
                {move || state.tree.with(|tree| match tree {
                    TreeView::None => String::new(),
                    TreeView::Loading => "Lendo os arquivos do projeto…".to_string(),
                    TreeView::Failed(message) => format!("Não foi possível ler os arquivos. {message}"),
                    TreeView::Ready(tree) => {
                        let count = tree.selected.len();
                        format!(
                            "Vai junto: {count} {} · {} KiB de {} KiB · árvore com {} nomes{}",
                            if count == 1 { "arquivo" } else { "arquivos" },
                            kib(tree.bytes_of(&tree.selected)),
                            MAXIMUM_CONTEXT_BYTES / 1024,
                            tree.names(),
                            if tree.truncated { " (parcial)" } else { "" },
                        )
                    }
                })}
            </span>
            <button
                id=id
                class="btn btn--sm btn--ghost"
                type="button"
                data-testid="context-open"
                aria-haspopup="dialog"
                disabled=move || !state.tree.with(|tree| matches!(tree, TreeView::Ready(_)))
                on:click=move |_| state.show(id)
            >
                "Escolher arquivos"
            </button>
        </div>
    }
}

/// The dialog, drawn over the window while it is open.
pub fn dialog(state: ContextState) -> impl IntoView {
    let keys = move |event: KeyboardEvent| match event.key().as_str() {
        "Escape" => {
            event.prevent_default();
            state.close();
        }
        "Tab" => {
            // The focus stays in the dialog: it wraps from the last control to the search, and back.
            let focused = dom::focused_id();
            if event.shift_key() && focused == SEARCH_ID {
                event.prevent_default();
                dom::focus(SAVE_ID);
            } else if !event.shift_key() && focused == SAVE_ID {
                event.prevent_default();
                dom::focus(SEARCH_ID);
            }
        }
        _ => {}
    };
    move || {
        if !state.open.get() {
            return ().into_any();
        }
        let TreeView::Ready(tree) = state.tree.get() else {
            return ().into_any();
        };
        let tree = StoredValue::new(tree);
        let total = move || tree.with_value(|tree| state.draft.with(|draft| tree.bytes_of(draft)));
        view! {
            <div class="modal-backdrop" aria-hidden="true" on:click=move |_| state.close()></div>
            <div
                class="modal"
                role="dialog"
                aria-modal="true"
                aria-labelledby="ctx-title"
                data-testid="context-dialog"
                on:keydown=keys
            >
                <div class="modal__head">
                    <h2 id="ctx-title" class="h2">"Escolher arquivos"</h2>
                    <span class="note" role="status">
                        {move || format!("{} KiB de {} KiB", kib(total()), MAXIMUM_CONTEXT_BYTES / 1024)}
                    </span>
                </div>
                <p class="note">
                    "Vão o texto dos arquivos marcados (até 32 KiB cada) e a lista de nomes e tamanhos do projeto, sem conteúdo. Arquivos sensíveis ficam bloqueados."
                </p>
                <input
                    id=SEARCH_ID
                    class="field"
                    type="search"
                    aria-label="Buscar arquivo"
                    placeholder="Buscar arquivo"
                    prop:value=move || state.query.get()
                    on:input=move |event| state.query.set(event_target_value(&event))
                />
                <ul class="ctx-list" aria-label="Arquivos do projeto">
                    {move || {
                        let query = state.query.get().to_lowercase();
                        tree.with_value(|tree| {
                            let matching: Vec<Entry> = tree
                                .entries
                                .iter()
                                .filter(|entry| query.is_empty() || entry.path.to_lowercase().contains(&query))
                                .cloned()
                                .collect();
                            let hidden = matching.len().saturating_sub(SHOWN_ROWS);
                            let rows = matching
                                .into_iter()
                                .take(SHOWN_ROWS)
                                .map(|entry| row(state, entry))
                                .collect_view();
                            view! {
                                {rows}
                                {(hidden > 0).then(|| view! {
                                    <li class="note ctx-list__more">{format!("Mais {hidden} arquivos. Refine a busca.")}</li>
                                })}
                            }
                        })
                    }}
                </ul>
                {move || state.error.get().map(|message| view! { <p class="note" role="alert">{message}</p> })}
                <div class="modal__foot">
                    <button
                        class="btn btn--sm btn--ghost"
                        type="button"
                        disabled=move || state.busy.get()
                        on:click=move |_| state.save(None)
                    >
                        "Usar o padrão"
                    </button>
                    <span class="modal__spacer"></span>
                    <button class="btn btn--sm" type="button" on:click=move |_| state.close()>"Cancelar"</button>
                    <button
                        id=SAVE_ID
                        class="btn btn--sm btn--primary"
                        type="button"
                        disabled=move || state.busy.get()
                        on:click=move |_| state.save(Some(state.draft.get_untracked()))
                    >
                        "Salvar"
                    </button>
                </div>
            </div>
        }
        .into_any()
    }
}

fn row(state: ContextState, entry: Entry) -> impl IntoView {
    let path = entry.path.clone();
    let checked = {
        let path = path.clone();
        move || state.draft.with(|draft| draft.contains(&path))
    };
    let reason = if entry.blocked {
        Some("bloqueado")
    } else if entry.bytes > MAXIMUM_FILE_BYTES {
        Some("maior que 32 KiB")
    } else {
        None
    };
    let bytes = entry.bytes;
    let disabled = {
        let checked = checked.clone();
        let blocked = reason.is_some();
        move || {
            blocked
                || (!checked()
                    && state.tree.with(|tree| match tree {
                        TreeView::Ready(tree) => {
                            state.draft.with(|draft| tree.bytes_of(draft)) + bytes
                                > MAXIMUM_CONTEXT_BYTES
                        }
                        _ => true,
                    }))
        }
    };
    let entry = StoredValue::new(entry);
    view! {
        <li>
            <label class=move || if reason.is_some() { "ctx-row is-blocked" } else { "ctx-row" }>
                <input
                    type="checkbox"
                    prop:checked=checked
                    disabled=disabled
                    on:change=move |_| entry.with_value(|entry| state.toggle(entry))
                />
                <span class="ctx-row__path">{path}</span>
                <span class="ctx-row__size">{reason.map_or_else(|| format!("{} KiB", kib(bytes)), str::to_string)}</span>
            </label>
        </li>
    }
}
