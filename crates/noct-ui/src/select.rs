//! The app's own select: a button that shows the value, and a list in a popup drawn in the app's style (glass, light and
//! dark), in place of the list the system draws for a `<select>`.
//!
//! It follows the select-only combobox pattern: the button is the `combobox`, the focus stays on it, and the list is a
//! `listbox` whose active option is named by `aria-activedescendant`. Arrows, Home and End move; Enter and Space choose;
//! Esc closes and keeps the focus on the button; a letter jumps to the next option that starts with it; a click outside
//! closes. The list is mounted in `<body>` and placed with fixed coordinates, so no panel with `overflow: hidden` cuts it;
//! it opens upward when there is not enough room below. The coordinates are CSS custom properties set on `<html>` through
//! the CSS object model: no `style` attribute is written, as the page policy requires.

use leptos::portal::Portal;
use leptos::prelude::*;

use crate::dom;

/// One option: the value the caller saves, and the words shown.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Choice {
    pub value: &'static str,
    pub label: &'static str,
}

/// The options of a settings table of `(value, saved value, words)`.
pub fn choices_of<T>(table: &[(T, &'static str, &'static str)]) -> Vec<Choice> {
    table
        .iter()
        .map(|(_, value, label)| Choice { value, label })
        .collect()
}

/// The height of one option and the padding of the list, in CSS pixels, to tell whether the list fits below the button.
const OPTION_HEIGHT: f64 = 38.0;
const LIST_PADDING: f64 = 12.0;
/// The gap between the button and the list.
const GAP: f64 = 6.0;

/// The index of the next enabled option from `from` in a direction, stopping at the ends.
fn step(count: usize, from: usize, forward: bool) -> usize {
    if forward {
        (from + 1).min(count.saturating_sub(1))
    } else {
        from.saturating_sub(1)
    }
}

/// The first letter of a label, for the jump by typing: lower case, without the accent of the common Latin letters.
fn initial(label: &str) -> Option<char> {
    let first = label.chars().next()?.to_lowercase().next()?;
    Some(match first {
        'á' | 'à' | 'â' | 'ã' | 'ä' => 'a',
        'é' | 'è' | 'ê' | 'ë' => 'e',
        'í' | 'ì' | 'î' | 'ï' => 'i',
        'ó' | 'ò' | 'ô' | 'õ' | 'ö' => 'o',
        'ú' | 'ù' | 'û' | 'ü' => 'u',
        'ç' => 'c',
        other => other,
    })
}

/// The option to jump to for a typed letter: the next one after `from` whose label starts with it, wrapping around.
pub fn jump(choices: &[Choice], from: usize, typed: char) -> Option<usize> {
    let typed = initial(&typed.to_string())?;
    let count = choices.len();
    (1..=count)
        .map(|offset| (from + offset) % count)
        .find(|&index| initial(choices[index].label) == Some(typed))
}

/// A select. `id` is the button's id, which a `<label for>` names; `value` is the saved value, `placeholder` the words
/// shown when it matches no option, and `choose` is called with the value of the option chosen.
pub fn select(
    id: &'static str,
    choices: Vec<Choice>,
    value: Signal<String>,
    placeholder: &'static str,
    disabled: Signal<bool>,
    choose: impl Fn(&'static str) + Copy + Send + Sync + 'static,
) -> impl IntoView {
    let choices = StoredValue::new(choices);
    let open = RwSignal::new(false);
    let upward = RwSignal::new(false);
    let active = RwSignal::new(0_usize);
    let list_id = StoredValue::new(format!("{id}-list"));
    let option_id = move |index: usize| format!("{id}-option-{index}");

    let selected = move || {
        let value = value.get();
        choices.with_value(|choices| choices.iter().position(|choice| choice.value == value))
    };
    let shown = move || {
        selected()
            .map(|index| choices.with_value(|choices| choices[index].label))
            .unwrap_or(placeholder)
    };

    // Places the list under the button, or over it when there is not enough room below and more above.
    let place = move || {
        let Some((left, top, bottom, width)) = dom::box_of(id) else {
            return;
        };
        let viewport = dom::viewport_height();
        let needed = choices.with_value(Vec::len) as f64 * OPTION_HEIGHT + LIST_PADDING + GAP;
        let below = viewport - bottom;
        let up = below < needed && top > below;
        upward.set(up);
        dom::set_root_property("--select-left", &format!("{left}px"));
        dom::set_root_property("--select-width", &format!("{width}px"));
        if up {
            dom::set_root_property("--select-bottom", &format!("{}px", viewport - top + GAP));
        } else {
            dom::set_root_property("--select-top", &format!("{}px", bottom + GAP));
        }
    };
    let show = move |at: Option<usize>| {
        if disabled.get_untracked() || open.get_untracked() {
            if let Some(index) = at {
                active.set(index);
            }
            return;
        }
        place();
        active.set(at.or_else(selected).unwrap_or(0));
        open.set(true);
    };
    let close = move || {
        open.set(false);
        dom::focus(id);
    };
    let pick = move |index: usize| {
        let chosen = choices.with_value(|choices| choices.get(index).copied());
        close();
        if let Some(chosen) = chosen
            && Some(index) != selected()
        {
            choose(chosen.value);
        }
    };

    let keydown = move |event: leptos::ev::KeyboardEvent| {
        let count = choices.with_value(Vec::len);
        if count == 0 {
            return;
        }
        let key = event.key();
        let is_open = open.get_untracked();
        let current = active.get_untracked();
        let handled = match key.as_str() {
            "ArrowDown" if event.alt_key() => {
                show(None);
                true
            }
            "ArrowDown" => {
                if is_open {
                    active.set(step(count, current, true));
                } else {
                    show(None);
                }
                true
            }
            "ArrowUp" if event.alt_key() && is_open => {
                pick(current);
                true
            }
            "ArrowUp" => {
                if is_open {
                    active.set(step(count, current, false));
                } else {
                    show(None);
                }
                true
            }
            "Home" => {
                show(Some(0));
                active.set(0);
                true
            }
            "End" => {
                show(Some(count - 1));
                active.set(count - 1);
                true
            }
            "Enter" | " " => {
                if is_open {
                    pick(current);
                } else {
                    show(None);
                }
                true
            }
            "Escape" if is_open => {
                close();
                true
            }
            "Tab" if is_open => {
                open.set(false);
                false
            }
            _ => {
                let mut chars = key.chars();
                match (chars.next(), chars.next()) {
                    (Some(typed), None)
                        if !typed.is_whitespace()
                            && !event.ctrl_key()
                            && !event.meta_key()
                            && !event.alt_key() =>
                    {
                        let from = if is_open {
                            current
                        } else {
                            selected().unwrap_or(0)
                        };
                        if let Some(index) =
                            choices.with_value(|choices| jump(choices, from, typed))
                        {
                            show(Some(index));
                            active.set(index);
                        }
                        true
                    }
                    _ => false,
                }
            }
        };
        if handled {
            event.prevent_default();
            event.stop_propagation();
        }
        if open.get_untracked() {
            let list = format!("{id}-list");
            let option = option_id(active.get_untracked());
            dom::next_frame(move || dom::keep_in_view(&list, &option));
        }
    };

    view! {
        <span class="sel">
            <button
                id=id
                type="button"
                class="select sel__button"
                role="combobox"
                aria-haspopup="listbox"
                aria-controls=list_id.get_value()
                aria-expanded=move || open.get().to_string()
                aria-activedescendant=move || open.get().then(|| option_id(active.get()))
                data-value=move || value.get()
                disabled=move || disabled.get()
                on:click=move |_| {
                    if open.get_untracked() {
                        close();
                    } else {
                        show(None);
                    }
                }
                on:keydown=keydown
            >
                <span class="sel__value">{shown}</span>
                <svg class="sel__chevron" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
                    <path d="m6 9 6 6 6-6" />
                </svg>
            </button>
            <Show when=move || open.get()>
                <Portal>
                    <div class="popover-backdrop sel__backdrop" data-testid="select-backdrop" on:click=move |_| close()></div>
                    <ul
                        id=list_id.get_value()
                        role="listbox"
                        class=move || if upward.get() { "sel__list sel__list--up" } else { "sel__list" }
                        aria-labelledby=id
                        tabindex="-1"
                    >
                        {move || {
                            let current = selected();
                            choices
                                .get_value()
                                .into_iter()
                                .enumerate()
                                .map(|(index, choice)| {
                                    let is_selected = current == Some(index);
                                    view! {
                                        <li
                                            id=option_id(index)
                                            role="option"
                                            class=move || {
                                                if active.get() == index { "sel__option is-active" } else { "sel__option" }
                                            }
                                            aria-selected=is_selected.to_string()
                                            data-value=choice.value
                                            on:mousedown=|event| event.prevent_default()
                                            on:mousemove=move |_| {
                                                if active.get_untracked() != index {
                                                    active.set(index);
                                                }
                                            }
                                            on:click=move |_| pick(index)
                                        >
                                            <span>{choice.label}</span>
                                            <svg class="sel__check" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
                                                <path d="M20 6 9 17l-5-5" />
                                            </svg>
                                        </li>
                                    }
                                })
                                .collect_view()
                        }}
                    </ul>
                </Portal>
            </Show>
        </span>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const CHOICES: [Choice; 4] = [
        Choice {
            value: "pt",
            label: "Português (Brasil)",
        },
        Choice {
            value: "en",
            label: "English",
        },
        Choice {
            value: "es",
            label: "Español",
        },
        Choice {
            value: "pl",
            label: "Polski",
        },
    ];

    #[test]
    fn a_typed_letter_jumps_to_the_next_option_that_starts_with_it() {
        assert_eq!(jump(&CHOICES, 0, 'e'), Some(1));
        assert_eq!(jump(&CHOICES, 1, 'E'), Some(2));
        assert_eq!(jump(&CHOICES, 2, 'e'), Some(1), "it wraps around");
        assert_eq!(jump(&CHOICES, 0, 'p'), Some(3));
        assert_eq!(jump(&CHOICES, 3, 'p'), Some(0));
        assert_eq!(jump(&CHOICES, 0, 'z'), None);
    }

    #[test]
    fn an_accent_does_not_hide_an_option_from_the_jump() {
        let choices = [
            Choice {
                value: "a",
                label: "Ótimo",
            },
            Choice {
                value: "b",
                label: "Água",
            },
        ];
        assert_eq!(jump(&choices, 1, 'o'), Some(0));
        assert_eq!(jump(&choices, 0, 'a'), Some(1));
    }

    #[test]
    fn the_arrows_stop_at_the_ends() {
        assert_eq!(step(3, 0, false), 0);
        assert_eq!(step(3, 2, true), 2);
        assert_eq!(step(3, 1, true), 2);
    }
}
