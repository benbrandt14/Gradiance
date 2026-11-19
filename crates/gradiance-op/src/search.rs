//! Ranking text against a query: the scorer behind F3 search (brief U-9).
//!
//! Pure and allocation-light, so it is unit-tested here and every surface
//! that searches the registry (the F3 box, the `ui.search` operation) ranks
//! the same way. A candidate is a list of [`Field`]s (its name, menu path,
//! Lisp name, doc), each with a weight.
//!
//! The query splits on whitespace into terms, and **every** term must match
//! some field (so `del sel` narrows rather than widens). A term's score in
//! one field is the best of, in falling order:
//!
//! | Match | Score |
//! |---|---|
//! | the whole field | 100 |
//! | a prefix of the field | 80 |
//! | a prefix of a word in the field | 60 |
//! | a substring | 40 |
//! | a subsequence (`spb` in `spawn-box`) | 10, plus 4 per character that starts a word, at most 30 |
//!
//! Matching ignores case. Words split at anything that is not a letter or a
//! digit, so `spawn-box`, `Edit/Delete selection` and `body.spawn_box` all
//! have word starts where a reader sees them. A term's score is its best
//! field's score times that field's weight; a candidate's score is the sum
//! over the terms. [`rank`] sorts by score, highest first, and keeps the
//! input order between equal scores (a stable sort), so an empty query lists
//! the candidates as given.

/// One searchable text of a candidate, with its weight.
#[derive(Debug, Clone, Copy)]
pub struct Field<'a> {
    /// The text.
    pub text: &'a str,
    /// What a match in this field counts for (a name outranks a doc).
    pub weight: u32,
}

impl<'a> Field<'a> {
    /// A field of `text` that counts `weight` times.
    pub fn new(text: &'a str, weight: u32) -> Self {
        Self { text, weight }
    }
}

/// The score of one query term in one text, or `None` when it does not
/// match. `term` must already be lower-case.
pub fn term_score(term: &str, text: &str) -> Option<u32> {
    if term.is_empty() {
        return Some(0);
    }
    let text = text.to_lowercase();
    if text == term {
        return Some(100);
    }
    if text.starts_with(term) {
        return Some(80);
    }
    if word_starts(&text).any(|at| text[at..].starts_with(term)) {
        return Some(60);
    }
    if text.contains(term) {
        return Some(40);
    }
    subsequence(term, &text)
}

/// The byte offsets in `text` where a word starts.
fn word_starts(text: &str) -> impl Iterator<Item = usize> + '_ {
    let mut previous_is_word = false;
    text.char_indices().filter_map(move |(at, c)| {
        let is_word = c.is_alphanumeric();
        let starts = is_word && !previous_is_word;
        previous_is_word = is_word;
        starts.then_some(at)
    })
}

/// `term`'s characters in order inside `text` (greedy, earliest match), with
/// a bonus for each one that lands on a word start.
fn subsequence(term: &str, text: &str) -> Option<u32> {
    let starts: Vec<usize> = word_starts(text).collect();
    let mut chars = text.char_indices();
    let mut bonus = 0u32;
    for wanted in term.chars() {
        let (at, _) = chars.by_ref().find(|(_, c)| *c == wanted)?;
        if starts.contains(&at) {
            bonus += 4;
        }
    }
    Some((10 + bonus).min(30))
}

/// The score of `query` against a candidate's fields, or `None` when some
/// term matches no field. An empty query scores 0 for every candidate.
pub fn score(query: &str, fields: &[Field<'_>]) -> Option<u32> {
    let query = query.to_lowercase();
    let mut total = 0;
    for term in query.split_whitespace() {
        let best = fields
            .iter()
            .filter_map(|field| term_score(term, field.text).map(|s| s * field.weight))
            .max()?;
        total += best;
    }
    Some(total)
}

/// The indices of the candidates that match `query`, best first; equal
/// scores keep their input order. `fields` gives each candidate's fields.
pub fn rank<T>(query: &str, items: &[T], fields: impl Fn(&T) -> Vec<Field<'_>>) -> Vec<usize> {
    let mut scored: Vec<(usize, u32)> = items
        .iter()
        .enumerate()
        .filter_map(|(i, item)| score(query, &fields(item)).map(|s| (i, s)))
        .collect();
    // `sort_by_key` is stable: equal scores stay in input order.
    scored.sort_by_key(|a| std::cmp::Reverse(a.1));
    scored.into_iter().map(|(i, _)| i).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn match_kinds_rank_in_order() {
        let exact = term_score("box", "box");
        let prefix = term_score("box", "boxes");
        let word = term_score("box", "spawn-box");
        let inner = term_score("pawn", "spawn-box");
        let sparse = term_score("sbx", "spawn-box");
        assert_eq!(exact, Some(100));
        assert_eq!(prefix, Some(80));
        assert_eq!(word, Some(60));
        assert_eq!(inner, Some(40));
        assert!(sparse.is_some_and(|s| (10..40).contains(&s)), "{sparse:?}");
    }

    #[test]
    fn case_is_ignored_and_a_miss_is_none() {
        assert_eq!(term_score("delete", "Edit/Delete selection"), Some(60));
        assert_eq!(score("DELETE", &[Field::new("delete", 1)]), Some(100));
        assert_eq!(term_score("xyz", "spawn-box"), None);
        // Out of order is not a subsequence.
        assert_eq!(term_score("xob", "box"), None);
    }

    #[test]
    fn word_starts_earn_a_subsequence_bonus() {
        // `sb` hits two word starts in `spawn-box`; `pw` hits none.
        let starts = term_score("sb", "spawn-box").unwrap_or_default();
        let inside = term_score("pw", "spawn-box").unwrap_or_default();
        assert!(starts > inside, "{starts} vs {inside}");
    }

    #[test]
    fn every_term_must_match_and_weights_scale() {
        let fields = [
            Field::new("Delete selection", 4),
            Field::new("Delete the i-th body", 1),
        ];
        // `delete` hits the name (word prefix → prefix, 80 × 4).
        assert_eq!(score("delete", &fields), Some(320));
        // Both terms match: the sum.
        assert_eq!(score("delete sel", &fields), Some(320 + 60 * 4));
        // One term misses everywhere: no match at all.
        assert_eq!(score("delete zebra", &fields), None);
        // Only the doc matches: weight 1.
        assert_eq!(score("body", &fields), Some(60));
    }

    #[test]
    fn rank_sorts_best_first_and_keeps_ties_in_order() {
        let items = ["undo", "redo", "box", "boxes", "un-box"];
        let order = rank("box", &items, |t| vec![Field::new(t, 1)]);
        // box (exact), boxes (prefix), un-box (word); undo and redo miss.
        assert_eq!(order, vec![2, 3, 4]);
        // An empty query keeps everything in input order.
        let all = rank("", &items, |t| vec![Field::new(t, 1)]);
        assert_eq!(all, vec![0, 1, 2, 3, 4]);
        // Equal scores: input order.
        let ties = rank("o", &["xo", "yo"], |t| vec![Field::new(t, 1)]);
        assert_eq!(ties, vec![0, 1]);
    }
}
