//! A line diff of a proposed change: the lines the file keeps, loses and gains, shown around the changes with a little
//! context. Pure and bounded: two texts whose line counts multiply past [`MAXIMUM_CELLS`] are not compared, and the
//! caller shows the new text whole.

/// The most cells of the comparison table. Two files of the change limit (32 KiB) stay well under it in practice.
pub const MAXIMUM_CELLS: usize = 1_500_000;
/// Unchanged lines kept around each change.
const CONTEXT: usize = 3;

/// One line of a diff.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Line<'a> {
    Same(&'a str),
    Removed(&'a str),
    Added(&'a str),
    /// Unchanged lines left out, and how many.
    Gap(usize),
}

/// The lines from `old` to `new`, or `None` when the texts are too long to compare. `old` is empty for a new file.
pub fn diff<'a>(old: &'a str, new: &'a str) -> Option<Vec<Line<'a>>> {
    let before: Vec<&str> = old.lines().collect();
    let after: Vec<&str> = new.lines().collect();
    let (rows, columns) = (before.len(), after.len());
    if rows.saturating_mul(columns) > MAXIMUM_CELLS {
        return None;
    }
    // The longest common subsequence, from the end, so the walk below goes forward.
    let width = columns + 1;
    let mut table = vec![0u32; (rows + 1) * width];
    for row in (0..rows).rev() {
        for column in (0..columns).rev() {
            table[row * width + column] = if before[row] == after[column] {
                table[(row + 1) * width + column + 1] + 1
            } else {
                table[(row + 1) * width + column].max(table[row * width + column + 1])
            };
        }
    }
    let mut lines = Vec::with_capacity(rows.max(columns));
    let (mut row, mut column) = (0, 0);
    while row < rows && column < columns {
        if before[row] == after[column] {
            lines.push(Line::Same(before[row]));
            row += 1;
            column += 1;
        } else if table[(row + 1) * width + column] >= table[row * width + column + 1] {
            lines.push(Line::Removed(before[row]));
            row += 1;
        } else {
            lines.push(Line::Added(after[column]));
            column += 1;
        }
    }
    lines.extend(before[row..].iter().map(|line| Line::Removed(line)));
    lines.extend(after[column..].iter().map(|line| Line::Added(line)));
    Some(fold(lines))
}

/// Keeps the changed lines and a few unchanged ones around them; longer runs of unchanged lines become one gap.
fn fold(lines: Vec<Line<'_>>) -> Vec<Line<'_>> {
    let changed: Vec<bool> = lines
        .iter()
        .map(|line| !matches!(line, Line::Same(_)))
        .collect();
    let near = |index: usize| {
        let start = index.saturating_sub(CONTEXT);
        let end = (index + CONTEXT + 1).min(changed.len());
        changed[start..end].iter().any(|flag| *flag)
    };
    let mut folded = Vec::new();
    let mut skipped = 0;
    for (index, line) in lines.iter().enumerate() {
        if near(index) {
            if skipped > 0 {
                folded.push(Line::Gap(skipped));
                skipped = 0;
            }
            folded.push(*line);
        } else {
            skipped += 1;
        }
    }
    if skipped > 0 {
        folded.push(Line::Gap(skipped));
    }
    folded
}

/// How many lines a diff adds and removes.
pub fn counts(lines: &[Line<'_>]) -> (usize, usize) {
    lines
        .iter()
        .fold((0, 0), |(added, removed), line| match line {
            Line::Added(_) => (added + 1, removed),
            Line::Removed(_) => (added, removed + 1),
            _ => (added, removed),
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_changed_line_is_removed_then_added_with_its_context() {
        let old = "a\nb\nc\nd\n";
        let new = "a\nB\nc\nd\n";
        assert_eq!(
            diff(old, new).expect("compared"),
            vec![
                Line::Same("a"),
                Line::Removed("b"),
                Line::Added("B"),
                Line::Same("c"),
                Line::Same("d")
            ]
        );
    }

    #[test]
    fn a_new_file_is_all_added_and_long_unchanged_runs_fold_into_a_gap() {
        assert_eq!(
            diff("", "x\ny\n").expect("compared"),
            vec![Line::Added("x"), Line::Added("y")]
        );
        let old: String = (0..20).map(|n| format!("{n}\n")).collect();
        let new = old.replace("10\n", "dez\n");
        let lines = diff(&old, &new).expect("compared");
        assert_eq!(lines.first(), Some(&Line::Gap(7)));
        assert_eq!(
            lines.last(),
            Some(&Line::Gap(6)),
            "nine unchanged lines after, three kept as context"
        );
        assert_eq!(counts(&lines), (1, 1));
    }

    #[test]
    fn texts_too_long_to_compare_are_not_compared() {
        let old = "x\n".repeat(2_000);
        let new = "y\n".repeat(1_000);
        assert_eq!(diff(&old, &new), None);
    }
}
