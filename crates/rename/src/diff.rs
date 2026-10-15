//! Which characters of a new name are not in the old one, for showing a rename as a diff.

/// A stretch of a new name: unchanged from the old name, or new.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Span {
    pub text: String,
    pub changed: bool,
}

/// Longer names are compared by their shared start and end only (the exact comparison is
/// quadratic); file names are short, so this is a safety net.
const EXACT_LIMIT: usize = 400;

/// Splits `new` into stretches that also appear (in order) in `old` and stretches that do
/// not. The unchanged characters are a longest common subsequence of the two names.
pub fn changed_spans(old: &str, new: &str) -> Vec<Span> {
    let (old, new): (Vec<char>, Vec<char>) = (old.chars().collect(), new.chars().collect());
    let keep = if old.len() > EXACT_LIMIT || new.len() > EXACT_LIMIT {
        shared_ends(&old, &new)
    } else {
        common_subsequence(&old, &new)
    };
    let mut spans: Vec<Span> = Vec::new();
    for (c, kept) in new.iter().zip(keep) {
        match spans.last_mut() {
            Some(last) if last.changed == !kept => last.text.push(*c),
            _ => spans.push(Span {
                text: c.to_string(),
                changed: !kept,
            }),
        }
    }
    spans
}

/// For each character of `new`: is it part of the common start or end?
fn shared_ends(old: &[char], new: &[char]) -> Vec<bool> {
    let start = old.iter().zip(new).take_while(|(a, b)| a == b).count();
    let end = old[start..]
        .iter()
        .rev()
        .zip(new[start..].iter().rev())
        .take_while(|(a, b)| a == b)
        .count();
    (0..new.len())
        .map(|i| i < start || i >= new.len() - end)
        .collect()
}

/// For each character of `new`: is it part of a longest common subsequence with `old`?
fn common_subsequence(old: &[char], new: &[char]) -> Vec<bool> {
    let (n, m) = (old.len(), new.len());
    // lengths[i][j]: LCS of old[i..] and new[j..].
    let mut lengths = vec![vec![0u16; m + 1]; n + 1];
    for i in (0..n).rev() {
        for j in (0..m).rev() {
            lengths[i][j] = if old[i] == new[j] {
                lengths[i + 1][j + 1] + 1
            } else {
                lengths[i + 1][j].max(lengths[i][j + 1])
            };
        }
    }
    let mut keep = vec![false; m];
    let (mut i, mut j) = (0, 0);
    while i < n && j < m {
        if old[i] == new[j] {
            keep[j] = true;
            i += 1;
            j += 1;
        } else if lengths[i + 1][j] >= lengths[i][j + 1] {
            i += 1;
        } else {
            j += 1;
        }
    }
    keep
}

#[cfg(test)]
mod tests {
    use super::*;

    fn show(old: &str, new: &str) -> String {
        changed_spans(old, new)
            .iter()
            .map(|s| {
                if s.changed {
                    format!("[{}]", s.text)
                } else {
                    s.text.clone()
                }
            })
            .collect()
    }

    #[test]
    fn only_the_new_characters_are_marked() {
        assert_eq!(show("photo.jpg", "photo.jpg"), "photo.jpg");
        assert_eq!(show("photo.jpg", "photo_01.jpg"), "photo[_01].jpg");
        assert_eq!(show("IMG_1234.JPG", "img_1234.jpg"), "[img]_1234.[jpg]");
        assert_eq!(show("a b.txt", "a_b.txt"), "a[_]b.txt");
        assert_eq!(show("report.doc", "2026-report.doc"), "[2026-]report.doc");
    }

    #[test]
    fn totally_different_and_empty_names() {
        assert_eq!(show("abc", "xyz"), "[xyz]");
        assert_eq!(show("abc", ""), "");
        assert_eq!(show("", "new"), "[new]");
        assert!(changed_spans("a", "").is_empty());
    }

    #[test]
    fn polish_letters_are_whole_characters() {
        assert_eq!(show("zazolc.txt", "zażółć.txt"), "za[żółć].txt");
        assert_eq!(show("żółw", "żółw2"), "żółw[2]");
    }

    #[test]
    fn very_long_names_fall_back_to_shared_ends() {
        let old = format!("{}.txt", "a".repeat(500));
        let new = format!("{}X.txt", "a".repeat(500));
        let spans = changed_spans(&old, &new);
        assert_eq!(spans.len(), 3, "{spans:?}");
        assert!(spans[1].changed && spans[1].text == "X");
        assert!(!spans[0].changed && !spans[2].changed);
    }

    #[test]
    fn spans_always_rebuild_the_new_name() {
        for (old, new) in [("abcabc", "bcab"), ("x", "xxxx"), ("aXb", "ab")] {
            let rebuilt: String = changed_spans(old, new)
                .iter()
                .map(|s| s.text.as_str())
                .collect();
            assert_eq!(rebuilt, new);
        }
    }
}
