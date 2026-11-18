//! The models Preferences ▸ AI offers in its dropdown: Claude's aliases, and the models Codex
//! itself lists in its own cache (`<CODEX_HOME or ~/.codex>/models_cache.json`), so the list
//! follows the user's Codex account without PdfCraft knowing any model names.

use std::path::{Path, PathBuf};

/// One choice in the model dropdown.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ModelChoice {
    /// What goes to the provider (`--model`).
    pub id: String,
    /// What the dropdown shows.
    pub label: String,
    /// One line about it; may be empty.
    pub note: String,
}

impl ModelChoice {
    fn new(id: &str, label: &str, note: &str) -> Self {
        Self { id: id.to_owned(), label: label.to_owned(), note: note.to_owned() }
    }
}

/// The file Codex keeps its model list in, inside its home folder.
pub const CODEX_MODELS_CACHE: &str = "models_cache.json";

/// Codex's cache is a few dozen KB; anything far larger is not it and is not read.
const MAX_CACHE_BYTES: u64 = 8 * 1024 * 1024;

/// Claude Code's model aliases, fastest first. They always mean the newest model of each family.
pub fn claude_models() -> Vec<ModelChoice> {
    vec![
        ModelChoice::new("haiku", "Haiku", "Fastest and cheapest"),
        ModelChoice::new("sonnet", "Sonnet", "Balanced"),
        ModelChoice::new("opus", "Opus", "Most capable, slowest"),
    ]
}

/// The models a Codex `models_cache.json` lists for the user (`"visibility": "list"`), in its
/// own `priority` order. Hidden models, and names that are not a plain model name (they become a
/// command-line argument), are left out; anything unreadable gives an empty list.
pub fn codex_models_from_cache(json: &str) -> Vec<ModelChoice> {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(json) else { return Vec::new() };
    let Some(models) = value.get("models").and_then(serde_json::Value::as_array) else { return Vec::new() };
    let mut listed: Vec<(i64, ModelChoice)> = models
        .iter()
        .filter(|m| m.get("visibility").and_then(serde_json::Value::as_str) == Some("list"))
        .filter_map(|m| {
            let id = m.get("slug").and_then(serde_json::Value::as_str).filter(|s| is_model_name(s))?;
            let label = m.get("display_name").and_then(serde_json::Value::as_str).map(|s| short(s, 80)).filter(|s| !s.is_empty());
            let note = m.get("description").and_then(serde_json::Value::as_str).map(|s| short(s, 160)).unwrap_or_default();
            let priority = m.get("priority").and_then(serde_json::Value::as_i64).unwrap_or(i64::MAX);
            Some((priority, ModelChoice { id: id.to_owned(), label: label.unwrap_or_else(|| id.to_owned()), note }))
        })
        .collect();
    listed.sort_by_key(|(priority, _)| *priority);
    listed.into_iter().map(|(_, m)| m).collect()
}

/// The models Codex lists in `codex_home` (its `models_cache.json`); empty when there is none.
pub fn codex_models_in(codex_home: &Path) -> Vec<ModelChoice> {
    let path = codex_home.join(CODEX_MODELS_CACHE);
    match std::fs::metadata(&path) {
        Ok(meta) if meta.is_file() && meta.len() <= MAX_CACHE_BYTES => {}
        _ => return Vec::new(),
    }
    std::fs::read_to_string(&path).map(|text| codex_models_from_cache(&text)).unwrap_or_default()
}

/// Codex's home folder: `CODEX_HOME`, else `.codex` in the user's home folder.
pub fn codex_home() -> Option<PathBuf> {
    std::env::var_os("CODEX_HOME").filter(|v| !v.is_empty()).map(PathBuf::from).or_else(|| std::env::home_dir().map(|h| h.join(".codex")))
}

/// The models Codex lists on this machine; empty when Codex has never run here.
pub fn codex_models() -> Vec<ModelChoice> {
    codex_home().map(|home| codex_models_in(&home)).unwrap_or_default()
}

/// A plain model name: 1 to 100 letters, digits, `.`, `_`, `-` or `:`, not starting with `-`.
fn is_model_name(s: &str) -> bool {
    (1..=100).contains(&s.len()) && !s.starts_with('-') && s.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-' | ':'))
}

/// `s` trimmed, without control characters, cut to `max` characters.
fn short(s: &str, max: usize) -> String {
    s.trim().chars().filter(|c| !c.is_control()).take(max).collect()
}
