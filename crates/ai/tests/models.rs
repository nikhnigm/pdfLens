//! The models offered in Preferences ▸ AI: Claude's aliases, and Codex's own list from its cache.

use pdfcraft_ai::{ModelChoice, claude_models, codex_models_from_cache, codex_models_in};

fn ids(list: &[ModelChoice]) -> Vec<&str> {
    list.iter().map(|m| m.id.as_str()).collect()
}

#[test]
fn claude_offers_its_aliases_fastest_first() {
    let list = claude_models();
    assert_eq!(ids(&list), ["haiku", "sonnet", "opus"]);
    assert!(list.iter().all(|m| !m.label.is_empty()));
    assert_eq!(pdfcraft_ai::ProviderConfig::default().model, "haiku", "the default is in the list");
}

const CACHE: &str = r#"{
  "fetched_at": "2026-11-16T19:48:55Z",
  "models": [
    { "slug": "gpt-reserve", "display_name": "GPT-Reserve", "visibility": "hide", "priority": 1 },
    { "slug": "gpt-5.6-luna", "display_name": "GPT-5.6-Luna", "description": "Older fast and efficient model.", "visibility": "list", "priority": 4 },
    { "slug": "gpt-6-luna", "display_name": "GPT-6-Luna", "description": "Fast and affordable model for easier tasks.", "visibility": "list", "priority": 2 },
    { "slug": "gpt-5.6-terra", "visibility": "list", "priority": 3 },
    { "slug": "--dangerously-bypass", "display_name": "Bad", "visibility": "list", "priority": 5 },
    { "slug": "has space", "visibility": "list", "priority": 6 },
    { "display_name": "No slug", "visibility": "list", "priority": 7 }
  ]
}"#;

#[test]
fn codex_offers_the_models_its_cache_lists_in_its_own_order() {
    let list = codex_models_from_cache(CACHE);
    assert_eq!(ids(&list), ["gpt-6-luna", "gpt-5.6-terra", "gpt-5.6-luna"], "hidden and unsafe names are left out");
    assert_eq!(list[0].label, "GPT-6-Luna");
    assert_eq!(list[0].note, "Fast and affordable model for easier tasks.");
    assert_eq!(list[1].label, "gpt-5.6-terra", "no display name: the slug");
}

#[test]
fn a_damaged_codex_cache_offers_nothing() {
    for text in ["", "not json", "[]", "{}", r#"{"models": 3}"#, r#"{"models": [1, "x", null]}"#] {
        assert!(codex_models_from_cache(text).is_empty(), "{text:?}");
    }
}

#[test]
fn codex_models_are_read_from_its_home_folder() {
    let dir = tempfile::tempdir().unwrap();
    assert!(codex_models_in(dir.path()).is_empty(), "no cache yet");
    std::fs::write(dir.path().join("models_cache.json"), CACHE).unwrap();
    assert_eq!(ids(&codex_models_in(dir.path())), ["gpt-6-luna", "gpt-5.6-terra", "gpt-5.6-luna"]);
}

#[test]
fn an_oversized_codex_cache_is_not_read() {
    let dir = tempfile::tempdir().unwrap();
    let huge = format!("{{\"models\": [], \"pad\": \"{}\"}}", "x".repeat(9 * 1024 * 1024));
    std::fs::write(dir.path().join("models_cache.json"), huge).unwrap();
    assert!(codex_models_in(dir.path()).is_empty());
}
