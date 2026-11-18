//! The prompt a lookup sends, the user's custom command line, and the cleanup of the answer.

use crate::AiError;

/// The longest answer kept, in characters.
const MAX_ANSWER: usize = 300;

/// The most lines of an answer kept.
const MAX_LINES: usize = 2;

/// What to look up.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Request {
    /// The selected word or short phrase.
    pub word: String,
    /// The sentence it was selected in.
    pub sentence: String,
    /// The language to answer in, as the user wrote it ("English", "Hindi"…).
    pub language: String,
}

/// The prompt sent to the provider on stdin.
pub fn prompt(req: &Request) -> String {
    format!(
        "Give the meaning of the word or phrase \"{}\" as it is used in this sentence: \"{}\". Answer in {}, in one line of plain text, at most two short lines. Start with the meaning itself: no preamble or label such as \"Here:\", no quotes, no markdown.",
        req.word, req.sentence, req.language
    )
}

/// Splits a command line the way a user types it: on whitespace, with a double-quoted part kept as
/// one argument. Backslashes are literal, so Windows paths need no escaping.
pub fn split_command_line(line: &str) -> Result<Vec<String>, AiError> {
    let (mut args, mut current, mut quoted, mut started) = (Vec::new(), String::new(), false, false);
    for c in line.chars() {
        match c {
            '"' => {
                quoted = !quoted;
                started = true;
            }
            c if c.is_whitespace() && !quoted => {
                if started {
                    args.push(std::mem::take(&mut current));
                    started = false;
                }
            }
            c => {
                current.push(c);
                started = true;
            }
        }
    }
    if quoted {
        return Err(AiError::BadCommand("a quote is not closed".into()));
    }
    if started {
        args.push(current);
    }
    if args.is_empty() {
        return Err(AiError::BadCommand("it is empty".into()));
    }
    Ok(args)
}

/// The provider's answer, cleaned for the popup: code fences, blank lines, a leading label
/// ("Meaning:", "Definition:", "Here:") and surrounding quotes removed, the first letter made
/// upper case after "Here:", at most two lines and [`MAX_ANSWER`] characters.
pub fn tidy_answer(raw: &str) -> Result<String, AiError> {
    const QUOTES: &[char] = &['"', '\'', '\u{201C}', '\u{201D}', '\u{2018}', '\u{2019}'];
    let lines: Vec<&str> = raw.lines().map(str::trim).filter(|l| !l.is_empty() && !l.starts_with("```")).take(MAX_LINES).collect();
    let text = lines.join("\n");
    let lower = text.to_lowercase();
    let label = ["meaning:", "definition:", "here:"].iter().find(|label| lower.starts_with(*label));
    let text = label.and_then(|label| text.get(label.len()..)).unwrap_or(&text).trim().trim_matches(QUOTES).trim();
    if text.is_empty() {
        return Err(AiError::Empty);
    }
    // "Here: giving up…" reads "Giving up…" once the label is gone.
    let capitalised;
    let text = match (label, text.chars().next()) {
        (Some(&"here:"), Some(first)) if first.is_lowercase() => {
            capitalised = first.to_uppercase().chain(text.chars().skip(1)).collect::<String>();
            capitalised.as_str()
        }
        _ => text,
    };
    if text.chars().count() <= MAX_ANSWER {
        return Ok(text.to_owned());
    }
    let mut cut: String = text.chars().take(MAX_ANSWER - 1).collect();
    cut.push('\u{2026}');
    Ok(cut)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::AiError;

    #[test]
    fn prompt_contains_word_sentence_language_and_rules() {
        let p = prompt(&Request { word: "ephemeral".into(), sentence: "The effect was ephemeral.".into(), language: "English".into() });
        for needle in ["ephemeral", "The effect was ephemeral.", "English", "one line"] {
            assert!(p.contains(needle), "{needle}");
        }
    }

    #[test]
    fn split_respects_quotes() {
        assert_eq!(split_command_line(r#""C:\Tools\llm.exe" -m gpt "a b""#).unwrap(), ["C:\\Tools\\llm.exe", "-m", "gpt", "a b"]);
        assert_eq!(split_command_line("ollama run llama3.2").unwrap(), ["ollama", "run", "llama3.2"]);
        assert!(matches!(split_command_line("  "), Err(AiError::BadCommand(_))));
        assert!(matches!(split_command_line(r#"tool "unclosed"#), Err(AiError::BadCommand(_))));
    }

    #[test]
    fn tidy_strips_wrapping_and_caps_lines() {
        assert_eq!(tidy_answer("  \"Lasting a very short time.\"  ").unwrap(), "Lasting a very short time.");
        assert_eq!(tidy_answer("```\nMeaning: brief\n```").unwrap(), "brief");
        assert_eq!(tidy_answer("one\n\ntwo\nthree\nfour").unwrap(), "one\ntwo");
        assert!(tidy_answer(&"a".repeat(1000)).unwrap().chars().count() <= 300);
        assert!(matches!(tidy_answer(" \n "), Err(AiError::Empty)));
    }

    #[test]
    fn a_here_label_is_dropped_and_the_meaning_capitalised() {
        assert_eq!(
            tidy_answer("Here: giving up or no longer keeping something, in this case a law.").unwrap(),
            "Giving up or no longer keeping something, in this case a law."
        );
        assert_eq!(tidy_answer("here: \"stopping\"").unwrap(), "Stopping");
        assert_eq!(tidy_answer("Here is why").unwrap(), "Here is why", "only the label, not the word");
        assert_eq!(tidy_answer("giving up").unwrap(), "giving up", "no label: left as written");
    }
}
