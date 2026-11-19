//! The changes an assistant proposes inside an answer. The answer is untrusted text: a change is read only from a block of
//! this exact shape, on lines of their own,
//!
//! `````text
//! ````noct-change path=src/lib.rs
//! the whole new text of the file
//! ````
//! `````
//!
//! with a fence of four or more backticks, closed by the same fence. The path must pass [`check_relative_path`] (relative,
//! no `..`, no sensitive name), the text must fit the limit, a path appears once, and an answer carries at most
//! [`MAXIMUM_PROPOSALS`] blocks. A block that breaks a rule is counted as refused and never proposed; a block that is not
//! closed is ignored. Reading a block proposes nothing by itself: the shell records each block as a pending change, which
//! the person approves or rejects in Construir.

use crate::limits::file_change::MAXIMUM_PROPOSALS;

use crate::file_change::{check_relative_path, fits_limit};

/// The word after the fence that marks a proposed change.
pub const BLOCK_WORD: &str = "noct-change";

/// One block that passed the rules.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProposedBlock {
    pub path: String,
    pub content: String,
}

/// The blocks of an answer: the ones that passed the rules, in order, and how many were refused.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ProposedBlocks {
    pub accepted: Vec<ProposedBlock>,
    pub refused: usize,
}

/// Reads the proposed changes of an answer. See the module documentation for the rules.
pub fn read_blocks(answer: &str) -> ProposedBlocks {
    let mut found = ProposedBlocks::default();
    let mut lines = answer.split_inclusive('\n');
    while let Some(line) = lines.next() {
        let Some((fence, path)) = opening(line) else {
            continue;
        };
        let mut content = String::new();
        let mut closed = false;
        for inner in lines.by_ref() {
            if inner.trim_end_matches(['\n', '\r']) == fence {
                closed = true;
                break;
            }
            content.push_str(inner);
        }
        if !closed {
            break;
        }
        let content = content.replace("\r\n", "\n");
        let duplicate = found.accepted.iter().any(|block| block.path == path);
        let full = found.accepted.len() >= MAXIMUM_PROPOSALS;
        match check_relative_path(path) {
            Ok(names) if !duplicate && !full && fits_limit(&content) => {
                found.accepted.push(ProposedBlock {
                    path: names.join("/"),
                    content,
                });
            }
            _ => found.refused += 1,
        }
    }
    found
}

/// The fence and the path of an opening line: four or more backticks, the word, one space or more, and `path=` with a path
/// that has no space. Anything else on the line makes it an ordinary line.
fn opening(line: &str) -> Option<(&str, &str)> {
    let line = line.trim_end_matches(['\n', '\r']);
    let ticks = line.len() - line.trim_start_matches('`').len();
    if ticks < 4 {
        return None;
    }
    let (fence, rest) = line.split_at(ticks);
    let rest = rest.strip_prefix(BLOCK_WORD)?;
    let trimmed = rest.trim_start_matches(' ');
    if trimmed.len() == rest.len() {
        return None;
    }
    let path = trimmed.strip_prefix("path=")?.trim_end_matches(' ');
    if path.is_empty() || path.contains(char::is_whitespace) {
        return None;
    }
    Some((fence, path))
}

/// The answer as the window shows it: each block of a proposed change becomes one line that names its file, so the whole
/// text of a file does not fill the conversation. Text outside the blocks is kept as it is.
pub fn shown_answer(answer: &str) -> String {
    let mut shown = String::new();
    let mut lines = answer.split_inclusive('\n').peekable();
    while let Some(line) = lines.next() {
        let Some((fence, path)) = opening(line) else {
            shown.push_str(line);
            continue;
        };
        let mut closed = false;
        let mut skipped = Vec::new();
        for inner in lines.by_ref() {
            if inner.trim_end_matches(['\n', '\r']) == fence {
                closed = true;
                break;
            }
            skipped.push(inner);
        }
        if closed {
            shown.push_str(&format!("[alteração proposta: {path}]\n"));
        } else {
            shown.push_str(line);
            for inner in skipped {
                shown.push_str(inner);
            }
        }
    }
    shown
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_closed_block_is_read_with_its_path_and_whole_text() {
        let answer = "Veja:\n````noct-change path=src/lib.rs\nfn main() {}\n```rust\ninner fence\n```\n````\nfim\n";
        let blocks = read_blocks(answer);
        assert_eq!(
            blocks.accepted,
            vec![ProposedBlock {
                path: "src/lib.rs".into(),
                content: "fn main() {}\n```rust\ninner fence\n```\n".into(),
            }]
        );
        assert_eq!(blocks.refused, 0);
        assert_eq!(
            shown_answer(answer),
            "Veja:\n[alteração proposta: src/lib.rs]\nfim\n"
        );
    }

    #[test]
    fn a_path_that_leaves_the_project_or_names_a_secret_is_refused() {
        let answer = [
            "````noct-change path=../etc/passwd\nx\n````\n",
            "````noct-change path=/etc/passwd\nx\n````\n",
            "````noct-change path=.env\nKEY=1\n````\n",
            "````noct-change path=.git/config\nx\n````\n",
            "````noct-change path=keys/server.pem\nx\n````\n",
            "````noct-change path=C:/x.txt\nx\n````\n",
            "````noct-change path=a\\b.txt\nx\n````\n",
        ]
        .concat();
        let blocks = read_blocks(&answer);
        assert!(blocks.accepted.is_empty());
        assert_eq!(blocks.refused, 7);
    }

    #[test]
    fn a_text_over_the_limit_a_repeated_path_and_extra_blocks_are_refused() {
        let big = "x".repeat(crate::limits::file_change::MAXIMUM_FILE_BYTES as usize + 1);
        let mut answer = format!("````noct-change path=big.txt\n{big}\n````\n");
        answer.push_str(
            "````noct-change path=a.txt\none\n````\n````noct-change path=a.txt\ntwo\n````\n",
        );
        for index in 0..MAXIMUM_PROPOSALS {
            answer.push_str(&format!("````noct-change path=f{index}.txt\nx\n````\n"));
        }
        let blocks = read_blocks(&answer);
        assert_eq!(blocks.accepted.len(), MAXIMUM_PROPOSALS);
        assert_eq!(blocks.accepted[0].content, "one\n");
        assert_eq!(
            blocks.refused, 3,
            "the big one, the repeat and the one past the cap"
        );
    }

    #[test]
    fn a_block_that_is_not_closed_or_not_in_the_exact_shape_proposes_nothing() {
        let cut = "````noct-change path=a.txt\nmetade do arquivo";
        assert_eq!(read_blocks(cut), ProposedBlocks::default());
        assert_eq!(shown_answer(cut), cut, "an open block is shown as it is");
        for answer in [
            "```noct-change path=a.txt\nx\n```\n",
            "````noct-change\nx\n````\n",
            "````noct-changepath=a.txt\nx\n````\n",
            "````noct-change path=a b.txt\nx\n````\n",
            "texto ````noct-change path=a.txt\nx\n````\n",
            "````noct-change path=a.txt\nx\n`````\n",
        ] {
            assert!(read_blocks(answer).accepted.is_empty(), "{answer:?}");
        }
    }

    #[test]
    fn windows_line_ends_are_read_and_the_text_is_kept_with_unix_ends() {
        let blocks = read_blocks("````noct-change path=a.txt\r\nlinha 1\r\nlinha 2\r\n````\r\n");
        assert_eq!(blocks.accepted[0].content, "linha 1\nlinha 2\n");
    }
}
