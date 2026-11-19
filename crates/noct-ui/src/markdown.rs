//! The answer of the assistant as Markdown, drawn without HTML.
//!
//! `parse` reads the answer with pulldown-cmark (no HTML output feature) into a small tree whose leaves are only text, and
//! `view` draws that tree with Leptos elements, whose text is always set as text. So nothing in an answer becomes markup:
//! raw HTML from the parser is kept as text, a link is its text and its address written out (nothing to click), and an
//! image is its description (nothing is loaded). Elements carry classes only; the page's policy grants no inline style.
//!
//! The tree is cut at `MAXIMUM_DEPTH` nested elements: deeper content is kept as text of the last allowed element. That
//! bounds the recursion of the drawing, of the hash and of the drop, whatever the answer holds.
//!
//! While an answer streams, it is parsed again at each piece; each top-level block is keyed by its position and a hash of
//! its content, so the blocks that did not change keep their elements and only the last one is redrawn. An open code
//! fence is a code block up to the end, and a table without its delimiter line is a paragraph until the line arrives.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use leptos::prelude::*;
use pulldown_cmark::{Alignment, CodeBlockKind, Event, Options, Parser, Tag, TagEnd};

use crate::dom;

/// The most elements nested in one another. Content deeper than this stays as text of the element at the limit.
pub const MAXIMUM_DEPTH: usize = 16;
/// The most characters of a code block's language label that are shown.
const MAXIMUM_LANGUAGE_CHARACTERS: usize = 24;

/// The alignment of a table column.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Align {
    None,
    Left,
    Center,
    Right,
}

/// What an element of the answer is. Only `Text`, `Code` and the code block's text hold characters of the answer.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Kind {
    Root,
    Paragraph,
    /// A paragraph of raw HTML from the answer, shown as its text.
    Raw,
    Heading(u8),
    Quote,
    List(Option<u64>),
    Item,
    CodeBlock {
        language: String,
        text: String,
    },
    Table(Vec<Align>),
    TableHead,
    TableRow,
    TableCell,
    Strong,
    Emphasis,
    Strike,
    /// A link: its children are its text, and the address is shown after them as text.
    Link(String),
    /// An image: only its description is kept, as text. Nothing is loaded.
    Image,
    /// An element the drawing does not know, kept as its children.
    Span,
    Text(String),
    Code(String),
    Break,
    Rule,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Node {
    pub kind: Kind,
    pub children: Vec<Node>,
}

impl Node {
    fn new(kind: Kind) -> Self {
        Self {
            kind,
            children: Vec::new(),
        }
    }

    /// The text of the node and its children, as the person reads it.
    pub fn text(&self) -> String {
        let mut out = String::new();
        self.collect_text(&mut out);
        out
    }

    fn collect_text(&self, out: &mut String) {
        match &self.kind {
            Kind::Text(text) | Kind::Code(text) => out.push_str(text),
            Kind::CodeBlock { text, .. } => out.push_str(text),
            Kind::Break => out.push('\n'),
            _ => {}
        }
        for child in &self.children {
            child.collect_text(out);
        }
        if let Kind::Link(url) = &self.kind {
            out.push_str(&link_suffix(&self.children_text(), url));
        }
    }

    fn children_text(&self) -> String {
        let mut out = String::new();
        for child in &self.children {
            child.collect_text(&mut out);
        }
        out
    }

    /// A hash of the node, which keys a block of the answer while it streams.
    pub fn digest(&self) -> u64 {
        let mut hasher = DefaultHasher::new();
        self.hash(&mut hasher);
        hasher.finish()
    }
}

/// The address written after a link's text, unless the text is the address itself.
fn link_suffix(text: &str, url: &str) -> String {
    if url.is_empty() || text.trim() == url {
        String::new()
    } else {
        format!(" ({url})")
    }
}

/// Reads an answer into its top-level blocks.
pub fn parse(answer: &str) -> Vec<Node> {
    let options =
        Options::ENABLE_TABLES | Options::ENABLE_STRIKETHROUGH | Options::ENABLE_TASKLISTS;
    let mut stack = vec![Node::new(Kind::Root)];
    // The starts that were not opened because the tree was at its depth limit; their ends are skipped too.
    let mut skipped = 0_usize;
    for event in Parser::new_ext(answer, options) {
        match event {
            Event::Start(tag) => {
                if stack.len() >= MAXIMUM_DEPTH {
                    skipped += 1;
                    continue;
                }
                stack.push(Node::new(kind_of(tag)));
            }
            Event::End(end) => {
                if skipped > 0 {
                    skipped -= 1;
                    continue;
                }
                if stack.len() > 1 {
                    let node = stack.pop().expect("the stack holds the root and the node");
                    let node = finish(node, end);
                    stack
                        .last_mut()
                        .expect("the root stays")
                        .children
                        .push(node);
                }
            }
            Event::Text(text) => push_text(&mut stack, &text),
            Event::Code(code) => push_leaf(&mut stack, Kind::Code(code.into_string())),
            // Raw HTML is never markup: it is kept as the text the answer wrote.
            Event::Html(html) | Event::InlineHtml(html) => push_text(&mut stack, &html),
            Event::InlineMath(text) | Event::DisplayMath(text) => push_text(&mut stack, &text),
            Event::FootnoteReference(label) => push_text(&mut stack, &format!("[^{label}]")),
            Event::SoftBreak => push_text(&mut stack, " "),
            Event::HardBreak => push_leaf(&mut stack, Kind::Break),
            Event::Rule => push_leaf(&mut stack, Kind::Rule),
            Event::TaskListMarker(done) => {
                push_text(&mut stack, if done { "[x] " } else { "[ ] " })
            }
        }
    }
    // An answer cut in the middle leaves nodes open: they are closed in order, so what arrived is still shown.
    while stack.len() > 1 {
        let node = stack.pop().expect("the stack holds the root and the node");
        stack
            .last_mut()
            .expect("the root stays")
            .children
            .push(node);
    }
    stack.pop().map(|root| root.children).unwrap_or_default()
}

fn kind_of(tag: Tag<'_>) -> Kind {
    match tag {
        Tag::Paragraph => Kind::Paragraph,
        Tag::Heading { level, .. } => Kind::Heading(level as u8),
        Tag::BlockQuote(_) => Kind::Quote,
        Tag::CodeBlock(kind) => Kind::CodeBlock {
            language: match kind {
                CodeBlockKind::Fenced(info) => info
                    .split_whitespace()
                    .next()
                    .unwrap_or("")
                    .chars()
                    .take(MAXIMUM_LANGUAGE_CHARACTERS)
                    .collect(),
                CodeBlockKind::Indented => String::new(),
            },
            text: String::new(),
        },
        Tag::HtmlBlock => Kind::Raw,
        Tag::List(start) => Kind::List(start),
        Tag::Item => Kind::Item,
        Tag::Table(aligns) => Kind::Table(
            aligns
                .into_iter()
                .map(|align| match align {
                    Alignment::None => Align::None,
                    Alignment::Left => Align::Left,
                    Alignment::Center => Align::Center,
                    Alignment::Right => Align::Right,
                })
                .collect(),
        ),
        Tag::TableHead => Kind::TableHead,
        Tag::TableRow => Kind::TableRow,
        Tag::TableCell => Kind::TableCell,
        Tag::Emphasis => Kind::Emphasis,
        Tag::Strong => Kind::Strong,
        Tag::Strikethrough => Kind::Strike,
        Tag::Link { dest_url, .. } => Kind::Link(dest_url.into_string()),
        Tag::Image { .. } => Kind::Image,
        _ => Kind::Span,
    }
}

/// Closes a node. A code block drops its last line break, which the fence leaves.
fn finish(mut node: Node, end: TagEnd) -> Node {
    if let (Kind::CodeBlock { text, .. }, TagEnd::CodeBlock) = (&mut node.kind, end)
        && text.ends_with('\n')
    {
        text.pop();
    }
    node
}

fn push_text(stack: &mut [Node], text: &str) {
    let top = stack.last_mut().expect("the root stays");
    if let Kind::CodeBlock { text: code, .. } = &mut top.kind {
        code.push_str(text);
        return;
    }
    if let Some(Node {
        kind: Kind::Text(last),
        ..
    }) = top.children.last_mut()
    {
        last.push_str(text);
        return;
    }
    top.children.push(Node::new(Kind::Text(text.to_string())));
}

fn push_leaf(stack: &mut [Node], kind: Kind) {
    let top = stack.last_mut().expect("the root stays");
    if let Kind::CodeBlock { text, .. } = &mut top.kind {
        // Only text reaches a code block; anything else is kept as its characters.
        if let Kind::Code(code) = &kind {
            text.push_str(code);
        }
        return;
    }
    top.children.push(Node::new(kind));
}

/// The blocks of an answer, keyed for the list that draws them.
pub fn keyed(answer: &str) -> Vec<((usize, u64), Node)> {
    parse(answer)
        .into_iter()
        .enumerate()
        .map(|(index, node)| ((index, node.digest()), node))
        .collect()
}

/// Draws one block of the answer and what it holds.
pub fn view(node: Node) -> AnyView {
    let Node { kind, children } = node;
    let inner = |children: Vec<Node>| children.into_iter().map(view).collect_view();
    match kind {
        Kind::Root | Kind::Span => view! { <span>{inner(children)}</span> }.into_any(),
        Kind::Paragraph => view! { <p>{inner(children)}</p> }.into_any(),
        Kind::Raw => view! { <p class="md-raw">{inner(children)}</p> }.into_any(),
        Kind::Heading(1) => view! { <h3 class="md-h md-h1">{inner(children)}</h3> }.into_any(),
        Kind::Heading(2) => view! { <h4 class="md-h md-h2">{inner(children)}</h4> }.into_any(),
        Kind::Heading(3) => view! { <h5 class="md-h md-h3">{inner(children)}</h5> }.into_any(),
        Kind::Heading(_) => view! { <h6 class="md-h md-h4">{inner(children)}</h6> }.into_any(),
        Kind::Quote => {
            view! { <blockquote class="md-quote">{inner(children)}</blockquote> }.into_any()
        }
        Kind::List(None) => view! { <ul class="md-list">{inner(children)}</ul> }.into_any(),
        Kind::List(Some(start)) => {
            view! { <ol class="md-list" start=start.to_string()>{inner(children)}</ol> }.into_any()
        }
        Kind::Item => view! { <li>{inner(children)}</li> }.into_any(),
        Kind::CodeBlock { language, text } => code_block(language, text),
        Kind::Table(aligns) => table(aligns, children),
        // Head, rows and cells are drawn by `table`; met elsewhere they are their text.
        Kind::TableHead | Kind::TableRow | Kind::TableCell => {
            view! { <span>{inner(children)}</span> }.into_any()
        }
        Kind::Strong => view! { <strong>{inner(children)}</strong> }.into_any(),
        Kind::Emphasis => view! { <em>{inner(children)}</em> }.into_any(),
        Kind::Strike => view! { <s>{inner(children)}</s> }.into_any(),
        Kind::Link(url) => {
            let text: String = children.iter().map(Node::text).collect();
            let suffix = link_suffix(&text, &url);
            let shown = if text.trim().is_empty() {
                url
            } else {
                String::new()
            };
            view! {
                <span class="md-link">{inner(children)}{shown}</span>
                {(!suffix.is_empty()).then(|| view! { <span class="md-url">{suffix}</span> })}
            }
            .into_any()
        }
        Kind::Image => {
            let alt: String = children.iter().map(Node::text).collect();
            let alt = alt.trim().to_string();
            let words = if alt.is_empty() {
                "[imagem]".to_string()
            } else {
                format!("[imagem: {alt}]")
            };
            view! { <span class="md-image">{words}</span> }.into_any()
        }
        Kind::Text(text) => text.into_any(),
        Kind::Code(code) => view! { <code class="md-code-inline">{code}</code> }.into_any(),
        Kind::Break => view! { <br /> }.into_any(),
        Kind::Rule => view! { <hr class="md-rule" /> }.into_any(),
    }
}

fn code_block(language: String, text: String) -> AnyView {
    let note = RwSignal::new(None::<&'static str>);
    let copied = text.clone();
    let copy = move |_| {
        dom::copy_text(&copied, move |done| {
            note.set(Some(if done {
                "Copiado."
            } else {
                "Não foi possível copiar."
            }));
        });
    };
    let label = if language.is_empty() {
        "código".to_string()
    } else {
        language
    };
    view! {
        <div class="md-code" data-testid="answer-code">
            <div class="md-code__head">
                <span class="mono">{label}</span>
                {move || note.get().map(|words| view! { <span class="md-code__note" role="status">{words}</span> })}
                <button class="tool" type="button" aria-label="Copiar o código" on:click=copy>
                    "Copiar"
                </button>
            </div>
            <pre class="md-code__body"><code>{text}</code></pre>
        </div>
    }
    .into_any()
}

fn align_class(aligns: &[Align], column: usize) -> &'static str {
    match aligns.get(column) {
        Some(Align::Center) => "is-center",
        Some(Align::Right) => "is-right",
        _ => "",
    }
}

fn cells(aligns: &[Align], row: Vec<Node>, head: bool) -> AnyView {
    row.into_iter()
        .enumerate()
        .map(|(column, cell)| {
            let class = align_class(aligns, column);
            let content = cell.children.into_iter().map(view).collect_view();
            if head {
                view! { <th scope="col" class=class>{content}</th> }.into_any()
            } else {
                view! { <td class=class>{content}</td> }.into_any()
            }
        })
        .collect_view()
        .into_any()
}

/// A table, in a box that scrolls sideways when it is wider than the answer.
fn table(aligns: Vec<Align>, children: Vec<Node>) -> AnyView {
    let mut head = Vec::new();
    let mut rows = Vec::new();
    for child in children {
        match child.kind {
            Kind::TableHead => head = child.children,
            Kind::TableRow => rows.push(child.children),
            _ => {}
        }
    }
    let head_aligns = aligns.clone();
    let body = rows
        .into_iter()
        .map(|row| {
            let aligns = aligns.clone();
            view! { <tr>{cells(&aligns, row, false)}</tr> }
        })
        .collect_view();
    view! {
        <div class="md-table" data-testid="answer-table" tabindex="0" role="region" aria-label="Tabela">
            <table>
                <thead><tr>{cells(&head_aligns, head, true)}</tr></thead>
                <tbody>{body}</tbody>
            </table>
        </div>
    }
    .into_any()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(nodes: &[Node], out: &mut Vec<String>) {
        for node in nodes {
            let name = format!("{:?}", node.kind);
            out.push(name.split(['(', ' ', '{']).next().unwrap_or("").to_string());
            kinds(&node.children, out);
        }
    }

    fn all_kinds(answer: &str) -> Vec<String> {
        let mut out = Vec::new();
        kinds(&parse(answer), &mut out);
        out
    }

    fn all_text(answer: &str) -> String {
        parse(answer)
            .iter()
            .map(Node::text)
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[test]
    fn the_supported_subset_is_read() {
        let answer = "## Título\n\nTexto **forte** e *leve* com `x = 1`.\n\n- um\n- dois\n\n1. a\n2. b\n\n> citação\n\n```rust\nfn main() {}\n```\n\n| a | b |\n|:-|-:|\n| 1 | 2 |\n";
        let blocks = parse(answer);
        let top: Vec<_> = blocks.iter().map(|node| node.kind.clone()).collect();
        assert_eq!(top[0], Kind::Heading(2));
        assert_eq!(top[1], Kind::Paragraph);
        assert_eq!(top[2], Kind::List(None));
        assert_eq!(top[3], Kind::List(Some(1)));
        assert_eq!(top[4], Kind::Quote);
        assert_eq!(
            top[5],
            Kind::CodeBlock {
                language: "rust".into(),
                text: "fn main() {}".into()
            }
        );
        assert_eq!(top[6], Kind::Table(vec![Align::Left, Align::Right]));
        let names = all_kinds(answer);
        for expected in [
            "Strong",
            "Emphasis",
            "Code",
            "Item",
            "TableHead",
            "TableRow",
            "TableCell",
        ] {
            assert!(
                names.iter().any(|name| name == expected),
                "{expected} in {names:?}"
            );
        }
    }

    #[test]
    fn raw_html_stays_text() {
        for answer in [
            "<script>alert(1)</script>",
            "texto <img src=x onerror=alert(1)> fim",
            "<a href=\"javascript:alert(1)\">clique</a>",
            "<div>\n<iframe src=\"https://exemplo.invalid\"></iframe>\n</div>",
        ] {
            let text = all_text(answer);
            assert!(text.contains('<'), "the markup is kept as text: {text:?}");
            let names = all_kinds(answer);
            assert!(
                names
                    .iter()
                    .all(|name| ["Paragraph", "Raw", "Text"].contains(&name.as_str())),
                "only text for {answer:?}: {names:?}"
            );
        }
        assert!(all_text("<script>alert(1)</script>").contains("<script>alert(1)</script>"));
        assert!(
            all_text("a <img src=x onerror=alert(1)> b").contains("<img src=x onerror=alert(1)>")
        );
    }

    #[test]
    fn a_link_is_its_text_and_its_address_and_nothing_to_follow() {
        let blocks = parse("veja [aqui](javascript:alert(1)) e <https://exemplo.invalid/a>");
        let text: String = blocks.iter().map(Node::text).collect();
        assert_eq!(
            text,
            "veja aqui (javascript:alert(1)) e https://exemplo.invalid/a"
        );
    }

    #[test]
    fn an_image_is_its_description_and_is_not_loaded() {
        let names = all_kinds("![um gráfico](https://exemplo.invalid/x.png)");
        assert!(names.contains(&"Image".to_string()));
        let text = all_text("![um gráfico](https://exemplo.invalid/x.png)");
        assert_eq!(text, "um gráfico");
        assert!(!text.contains("exemplo.invalid"));
    }

    #[test]
    fn a_malformed_table_does_not_break_the_answer() {
        let answer = "| a | b | c |\n|---|---|\n| 1 |\n| 1 | 2 | 3 | 4 |\n";
        let text = all_text(answer);
        assert!(text.contains('a') && text.contains('1'));
        let uneven = "| a | b |\n|---|---|\n| 1 |\n| 1 | 2 | 3 |\n";
        let blocks = parse(uneven);
        assert!(matches!(blocks[0].kind, Kind::Table(_)));
    }

    #[test]
    fn an_answer_cut_in_the_middle_is_still_drawn() {
        let open_fence = parse("Antes\n\n```rust\nfn main() {\n");
        assert!(matches!(
            &open_fence[1].kind,
            Kind::CodeBlock { text, .. } if text.starts_with("fn main()")
        ));
        let half_table = parse("| a | b |\n");
        assert_eq!(half_table[0].kind, Kind::Paragraph);
        let half_bold = all_text("um **meio");
        assert_eq!(half_bold, "um **meio");
    }

    #[test]
    fn nesting_is_cut_at_the_depth_limit() {
        let deep = ">".repeat(5000) + " fundo";
        let blocks = parse(&deep);
        fn depth(node: &Node) -> usize {
            1 + node.children.iter().map(depth).max().unwrap_or(0)
        }
        let deepest = blocks.iter().map(depth).max().unwrap_or(0);
        assert!(deepest <= MAXIMUM_DEPTH, "depth {deepest}");
        assert!(
            blocks
                .iter()
                .map(Node::text)
                .collect::<String>()
                .contains("fundo")
        );
        let emphasis = "*".repeat(3000) + "x" + &"*".repeat(3000);
        let _ = parse(&emphasis);
    }

    #[test]
    fn a_huge_answer_is_read() {
        let mut answer = String::new();
        while answer.len() < noct_core::limits::provider_chat::MAXIMUM_ANSWER_CHARACTERS * 4 {
            answer.push_str(
                "## T\n\n| a | b |\n|---|---|\n| 1 | 2 |\n\n- `x` **y** [z](https://e.invalid)\n\n",
            );
        }
        let blocks = parse(&answer);
        assert!(blocks.len() > 1000);
    }

    #[test]
    fn unchanged_blocks_keep_their_keys_while_the_answer_grows() {
        let first = keyed("Um.\n\nDois");
        let second = keyed("Um.\n\nDois e três");
        assert_eq!(first[0].0, second[0].0);
        assert_ne!(first[1].0, second[1].0);
    }
}
