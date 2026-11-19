//! Construir and Documentar: the changes proposed for the current project, approved or rejected one at a time, and undone
//! after. Documentar shows the changes to Markdown files and Construir the others; the flow is the same. Each
//! pending change shows its file and a line diff against the text the file had when it was proposed. Nothing is written
//! until the person presses Aprovar; the shell checks again that the file still holds that text, and refuses otherwise.
//! An applied change can be undone while the file still holds what the approval wrote.

use leptos::prelude::*;
use noct_core::file_change::{self, FileChange, FileChangeStatus};

use crate::diff::{self, Line};
use crate::ipc::{self, Failure};
use crate::sidebar::{self, Shared};

const LIST: &str = "file_changes_list";
const APPROVE: &str = "file_change_approve";
const REJECT: &str = "file_change_reject";
const UNDO: &str = "file_change_undo";
/// The most diff lines drawn for one change; a longer diff says how many lines are left out.
const SHOWN_LINES: usize = 400;

#[derive(Debug, Clone, PartialEq, Eq)]
enum ChangesView {
    Loading,
    Ready(Vec<FileChange>),
    Failed(Failure),
}

fn payload(value: &str) -> String {
    serde_json::json!({ "input": value }).to_string()
}

fn settle(outcome: ipc::Outcome) -> ChangesView {
    match ipc::reply(outcome) {
        Ok(text) => match serde_json::from_str::<Vec<FileChange>>(&text) {
            Ok(changes) => ChangesView::Ready(changes),
            Err(_) => ChangesView::Failed(ipc::invalid_response()),
        },
        Err(failure) => ChangesView::Failed(failure),
    }
}

/// Which changes a panel shows: Construir the code and other files, Documentar the Markdown files.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scope {
    Build,
    Docs,
}

impl Scope {
    fn holds(self, change: &FileChange) -> bool {
        file_change::is_markdown(&change.path) == (self == Scope::Docs)
    }
}

/// The panel of Construir or of Documentar.
pub fn panel(shared: Shared, scope: Scope) -> impl IntoView {
    let view = RwSignal::new(ChangesView::Loading);
    let busy = RwSignal::new(None::<String>);
    let error = RwSignal::new(None::<String>);

    // The list is read when the project changes, when an answer proposed changes, and after each decision.
    let read = move || {
        let Some(path) = shared.current.get_untracked() else {
            return;
        };
        ipc::call_with(LIST, &payload(&path), move |outcome| {
            if shared.current.get_untracked().as_deref() == Some(path.as_str()) {
                view.set(match settle(outcome) {
                    ChangesView::Ready(changes) => ChangesView::Ready(
                        changes
                            .into_iter()
                            .filter(|change| scope.holds(change))
                            .collect(),
                    ),
                    other => other,
                });
            }
        });
    };
    Effect::new(move |_| {
        let _ = shared.current.get();
        shared.changes_version.track();
        error.set(None);
        read();
    });

    let decide = move |command: &'static str, id: String| {
        if busy.get_untracked().is_some() {
            return;
        }
        busy.set(Some(id.clone()));
        error.set(None);
        ipc::call_with(command, &payload(&id), move |outcome| {
            busy.set(None);
            if let Err(failure) = ipc::reply(outcome) {
                error.set(Some(failure.message));
            }
            read();
        });
    };

    view! {
        <div class="stack" data-testid=match scope { Scope::Build => "build", Scope::Docs => "docs" }>
            <div>
                <h1 class="h1">{match scope { Scope::Build => "Construir", Scope::Docs => "Documentar" }}</h1>
                <p class="lead">
                    {match scope {
                        Scope::Build => "Cada alteração espera a sua aprovação. Toda alteração aplicada pode ser desfeita.",
                        Scope::Docs => "Alterações em arquivos Markdown do projeto. Cada uma espera a sua aprovação e pode ser desfeita.",
                    }}
                </p>
            </div>
            {move || {
                error.get().map(|message| {
                    view! { <p class="note note--error" role="alert" data-testid="change-error">{message}</p> }
                })
            }}
            {move || {
                if shared.current.with(Option::is_none) {
                    return view! {
                        <p class="lead" data-testid="build-no-project">"Adicione um projeto para ver as alterações propostas."</p>
                    }
                    .into_any();
                }
                match view.get() {
                    ChangesView::Loading => view! {
                        <p class="note" role="status" data-testid="changes-loading">"Carregando alterações…"</p>
                    }
                    .into_any(),
                    ChangesView::Failed(failure) => view! {
                        <div class="card error-card" role="alert" data-testid="changes-error">
                            <div>
                                <strong>"Não foi possível ler as alterações."</strong>
                                <p>{failure.message}</p>
                            </div>
                            <button class="btn" type="button" on:click=move |_| read()>"Tentar novamente"</button>
                        </div>
                    }
                    .into_any(),
                    ChangesView::Ready(changes) if changes.is_empty() => steps_view(scope).into_any(),
                    ChangesView::Ready(changes) => lists_view(shared, changes, busy, decide).into_any(),
                }
            }}
        </div>
    }
}

/// The changes, in three groups: waiting for a decision, applied, and closed.
fn lists_view(
    shared: Shared,
    changes: Vec<FileChange>,
    busy: RwSignal<Option<String>>,
    decide: impl Fn(&'static str, String) + Copy + Send + Sync + 'static,
) -> impl IntoView {
    let of = |status: FileChangeStatus| -> Vec<FileChange> {
        changes
            .iter()
            .filter(|change| change.status == status)
            .cloned()
            .collect()
    };
    let pending = of(FileChangeStatus::Pending);
    let applied = of(FileChangeStatus::Applied);
    let closed: Vec<FileChange> = changes
        .iter()
        .filter(|change| {
            matches!(
                change.status,
                FileChangeStatus::Rejected | FileChangeStatus::Undone
            )
        })
        .cloned()
        .collect();
    let project = sidebar::current_name(shared).unwrap_or_default();
    view! {
        <div class="group" data-testid="changes-pending">
            <div class="group__title">{format!("Esperando aprovação · {}", pending.len())}</div>
            {if pending.is_empty() {
                view! { <p class="note">{format!("Nada esperando aprovação em {project}.")}</p> }.into_any()
            } else {
                pending
                    .into_iter()
                    .map(|change| change_card(change, busy, decide))
                    .collect_view()
                    .into_any()
            }}
        </div>
        {(!applied.is_empty()).then(|| {
            view! {
                <div class="group" data-testid="changes-applied">
                    <div class="group__title">"Aplicadas"</div>
                    <div class="list">
                        {applied
                            .into_iter()
                            .map(|change| {
                                let id = change.id.clone();
                                let label = format!("Desfazer a alteração em {}", change.path);
                                let busy_id = change.id.clone();
                                view! {
                                    <div class="item">
                                        <div class="item__txt">
                                            <div class="item__name mono">{change.path.clone()}</div>
                                            <div class="item__desc">"Aplicada. Pode ser desfeita enquanto o arquivo não mudar."</div>
                                        </div>
                                        <button
                                            class="btn"
                                            type="button"
                                            aria-label=label
                                            disabled=move || busy.get().is_some_and(|working| working == busy_id)
                                            on:click=move |_| decide(UNDO, id.clone())
                                        >
                                            "Desfazer"
                                        </button>
                                    </div>
                                }
                            })
                            .collect_view()}
                    </div>
                </div>
            }
        })}
        {(!closed.is_empty()).then(|| {
            view! {
                <div class="group" data-testid="changes-closed">
                    <div class="group__title">"Recusadas e desfeitas"</div>
                    <div class="list">
                        {closed
                            .into_iter()
                            .map(|change| {
                                let state = match change.status {
                                    FileChangeStatus::Rejected => "Recusada",
                                    _ => "Desfeita",
                                };
                                view! {
                                    <div class="item">
                                        <div class="item__txt">
                                            <div class="item__name mono">{change.path.clone()}</div>
                                        </div>
                                        <span class="badge">{state}</span>
                                    </div>
                                }
                            })
                            .collect_view()}
                    </div>
                </div>
            }
        })}
    }
}

/// One pending change: its file, what it adds and removes, the diff, and the two decisions.
fn change_card(
    change: FileChange,
    busy: RwSignal<Option<String>>,
    decide: impl Fn(&'static str, String) + Copy + Send + Sync + 'static,
) -> impl IntoView {
    let base = change.base.clone().unwrap_or_default();
    let new_file = change.base.is_none();
    let lines = diff::diff(&base, &change.content);
    let summary = match &lines {
        Some(lines) => {
            let (added, removed) = diff::counts(lines);
            format!("+{added} −{removed}")
        }
        None => "Grande demais para comparar linha a linha".to_string(),
    };
    let body = match lines {
        Some(lines) => {
            let total = lines.len();
            let shown: Vec<AnyView> = lines.into_iter().take(SHOWN_LINES).map(diff_line).collect();
            view! {
                <div class="diff" role="group" aria-label="Diferença">
                    {shown}
                    {(total > SHOWN_LINES).then(|| view! {
                        <div class="diff__line diff__gap">{format!("… mais {} linhas", total - SHOWN_LINES)}</div>
                    })}
                </div>
            }
            .into_any()
        }
        None => view! { <pre class="diff diff__whole">{change.content.clone()}</pre> }.into_any(),
    };
    let approve_id = change.id.clone();
    let reject_id = change.id.clone();
    let busy_id = change.id.clone();
    let working = move || busy.get().is_some_and(|working| working == busy_id);
    let path = change.path.clone();
    view! {
        <section class="card card--flush change" data-testid="change">
            <div class="card__head">
                <div class="change__title">
                    <span class="mono change__path" data-testid="change-path">{change.path.clone()}</span>
                    {if new_file {
                        view! { <span class="tag tag--new" data-testid="change-new">"Arquivo novo"</span> }.into_any()
                    } else {
                        view! {
                            <span class="tag tag--overwrite" data-testid="change-overwrites">"Sobrescreve"</span>
                        }
                        .into_any()
                    }}
                </div>
                <span class="note change__count">{summary}</span>
            </div>
            {file_change::runs_on_build(&change.path, change.base.as_deref(), &change.content).then(|| view! {
                <p class="change__warning change__warning--build" role="note" data-testid="change-runs-on-build">
                    "Este arquivo roda código quando você compila ou instala. Leia o diff com atenção."
                </p>
            })}
            {(!new_file).then(|| view! {
                <p class="change__warning" role="note">
                    "Este arquivo já existe no projeto. Aprovar substitui o conteúdo inteiro dele pelo da proposta."
                </p>
            })}
            {body}
            <div class="card__foot">
                <button
                    class="btn btn--ghost"
                    type="button"
                    aria-label=format!("Recusar a alteração em {path}")
                    disabled=working.clone()
                    on:click=move |_| decide(REJECT, reject_id.clone())
                >
                    "Recusar"
                </button>
                <button
                    class="btn btn--primary"
                    type="button"
                    aria-label=format!("Aprovar a alteração em {}", change.path)
                    disabled=working
                    on:click=move |_| decide(APPROVE, approve_id.clone())
                >
                    "Aprovar"
                </button>
            </div>
        </section>
    }
}

fn diff_line(line: Line<'_>) -> AnyView {
    match line {
        Line::Same(text) => view! { <div class="diff__line"><span class="diff__mark" aria-hidden="true">" "</span><span>{text.to_string()}</span></div> }.into_any(),
        Line::Removed(text) => view! { <div class="diff__line diff__line--del"><span class="diff__mark">"−"</span><span>{text.to_string()}</span></div> }.into_any(),
        Line::Added(text) => view! { <div class="diff__line diff__line--add"><span class="diff__mark">"+"</span><span>{text.to_string()}</span></div> }.into_any(),
        Line::Gap(count) => view! { <div class="diff__line diff__gap">{format!("… {count} linhas iguais")}</div> }.into_any(),
    }
}

/// The start of Construir, while no change was proposed: how a change goes, step by step.
fn steps_view(scope: Scope) -> impl IntoView {
    let (asked, empty) = match scope {
        Scope::Build => (
            " uma alteração em Revisar.",
            "Nenhuma alteração proposta ainda.",
        ),
        Scope::Docs => (
            " uma atualização da documentação em Revisar.",
            "Nenhuma alteração de documentação proposta ainda.",
        ),
    };
    view! {
        <section class="card" data-testid="build-steps">
            <div class="step">
                <span class="step__n">"1"</span>
                <p><b>"Você pede"</b>{asked}</p>
            </div>
            <div class="step">
                <span class="step__n">"2"</span>
                <p><b>"O NOCT mostra o diff"</b>" de cada arquivo aqui, antes de gravar."</p>
            </div>
            <div class="step">
                <span class="step__n">"3"</span>
                <p><b>"Você aprova ou recusa"</b>" cada arquivo. Nada é gravado sem a sua aprovação."</p>
            </div>
        </section>
        <p class="lead" data-testid="changes-empty">{empty}</p>
    }
}
