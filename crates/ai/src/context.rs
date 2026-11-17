//! What a lookup is about: whether a selection is short enough to define, and the sentence it
//! sits in.

/// The longest sentence sent with a word, in characters.
pub const MAX_SENTENCE: usize = 300;

/// The most words a selection may have to be defined.
const MAX_WORDS: usize = 3;

/// The longest selection, in characters, that is defined.
const MAX_SELECTION: usize = 64;

/// Characters that end a sentence: Latin, the Devanagari danda, CJK full stops and marks.
const TERMINATORS: [char; 7] = ['.', '?', '!', '\u{0964}', '\u{3002}', '\u{FF1F}', '\u{FF01}'];

/// Whether `selection` is a word or short phrase Define can look up: one to three words on one
/// line, not too long.
pub fn is_definable(selection: &str) -> bool {
    let text = selection.trim();
    let words = text.split_whitespace().count();
    !text.contains('\n') && (1..=MAX_WORDS).contains(&words) && text.chars().count() <= MAX_SELECTION
}

/// The sentence around `selection`, given the page text `before` and `after` it: from the nearest
/// sentence end (or line break) before it to the nearest one after it, keeping the closing
/// punctuation. Whitespace is collapsed, and a sentence longer than [`MAX_SENTENCE`] characters is
/// cut to a window around the selection.
pub fn sentence_around(before: &str, selection: &str, after: &str) -> String {
    let is_boundary = |c: char| c == '\n' || TERMINATORS.contains(&c);
    let head_start = before.char_indices().rev().find(|&(_, c)| is_boundary(c)).map_or(0, |(i, c)| i + c.len_utf8());
    let head: Vec<char> = before.get(head_start..).unwrap_or("").chars().collect();
    let tail: Vec<char> = match after.char_indices().find(|&(_, c)| is_boundary(c)) {
        Some((i, '\n')) => after.get(..i),
        Some((i, c)) => after.get(..i + c.len_utf8()),
        None => Some(after),
    }
    .unwrap_or("")
    .chars()
    .collect();
    let selection: Vec<char> = selection.chars().take(MAX_SENTENCE).collect();
    // Share the room around the selection; a short side gives what it doesn't use to the other.
    let room = MAX_SENTENCE - selection.len();
    let left = head.len().min(room - tail.len().min(room / 2));
    let right = tail.len().min(room - left);
    let text: String = head[head.len() - left..].iter().chain(&selection).chain(&tail[..right]).collect();
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn definable_up_to_three_words() {
        assert!(is_definable("ephemeral"));
        assert!(is_definable("  per se  "));
        assert!(is_definable("de facto standard"));
        assert!(!is_definable("one two three four"));
        assert!(!is_definable(""));
        assert!(!is_definable("line\nbreak"));
    }

    #[test]
    fn sentence_is_cut_at_terminators() {
        let s = sentence_around("Intro text. The effect was ", "ephemeral", " at best! Next one.");
        assert_eq!(s, "The effect was ephemeral at best!");
    }

    #[test]
    fn sentence_falls_back_to_line() {
        assert_eq!(sentence_around("Heading\nTable of ", "contents", "\nChapter 1"), "Table of contents");
    }

    #[test]
    fn sentence_is_capped() {
        let long = "x ".repeat(400);
        let s = sentence_around(&long, "word", &long);
        assert!(s.chars().count() <= MAX_SENTENCE && s.contains("word"), "{s}");
    }

    #[test]
    fn sentence_handles_multibyte_text() {
        let s = sentence_around("यह एक वाक्य है। यह ", "शब्द", " है। 次の文。");
        assert_eq!(s, "यह शब्द है।");
    }
}
