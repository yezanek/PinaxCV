//! Word-level diff for rewrite cards (spec section 12.6).

/// One run of a diff.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Part {
    Same(String),
    Removed(String),
    Added(String),
}

/// The words of `before` and `after`, by longest common subsequence. Runs of
/// the same kind are joined with single spaces; bullets are short, so the
/// quadratic table stays small.
pub fn word_diff(before: &str, after: &str) -> Vec<Part> {
    let a: Vec<&str> = before.split_whitespace().collect();
    let b: Vec<&str> = after.split_whitespace().collect();
    let mut lcs = vec![vec![0u16; b.len() + 1]; a.len() + 1];
    for i in (0..a.len()).rev() {
        for j in (0..b.len()).rev() {
            lcs[i][j] = if a[i] == b[j] {
                lcs[i + 1][j + 1] + 1
            } else {
                lcs[i + 1][j].max(lcs[i][j + 1])
            };
        }
    }
    let mut out: Vec<Part> = Vec::new();
    let mut push = |part: Part| {
        let joined = match (out.last_mut(), &part) {
            (Some(Part::Same(x)), Part::Same(y))
            | (Some(Part::Removed(x)), Part::Removed(y))
            | (Some(Part::Added(x)), Part::Added(y)) => {
                x.push(' ');
                x.push_str(y);
                true
            }
            _ => false,
        };
        if !joined {
            out.push(part);
        }
    };
    let (mut i, mut j) = (0, 0);
    while i < a.len() || j < b.len() {
        if i < a.len() && j < b.len() && a[i] == b[j] {
            push(Part::Same(a[i].to_owned()));
            i += 1;
            j += 1;
        } else if i < a.len() && (j == b.len() || lcs[i + 1][j] >= lcs[i][j + 1]) {
            push(Part::Removed(a[i].to_owned()));
            i += 1;
        } else {
            push(Part::Added(b[j].to_owned()));
            j += 1;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn words_are_marked_removed_and_added() {
        assert_eq!(
            word_diff(
                "Led the migration of billing to Rust",
                "Migrated billing to Rust"
            ),
            [
                Part::Removed("Led the migration of".into()),
                Part::Added("Migrated".into()),
                Part::Same("billing to Rust".into()),
            ]
        );
        assert_eq!(word_diff("a b", "a b"), [Part::Same("a b".into())]);
        assert_eq!(word_diff("", "new"), [Part::Added("new".into())]);
    }
}
