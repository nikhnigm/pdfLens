//! Answers already looked up, so the same word in the same sentence comes back at once.

use crate::{AiError, ProviderConfig, Request};
use std::collections::HashMap;
use std::path::Path;

/// The most answers kept; past it the oldest goes first.
pub const CACHE_CAP: usize = 2000;

/// The cache's file name, in the settings folder.
pub const CACHE_FILE: &str = "ai-cache.json";

/// Answers by lookup key, oldest first when it is full.
#[derive(Clone, Debug, Default)]
pub struct Cache {
    entries: HashMap<String, (u64, String)>,
    next: u64,
}

impl Cache {
    /// The key for `req` asked of `cfg`: the same word in another sentence, or of another model or
    /// in another language, is another lookup.
    pub fn key(req: &Request, cfg: &ProviderConfig) -> String {
        [req.word.as_str(), &req.sentence, cfg.kind.id(), &cfg.model, &cfg.custom, &req.language].join("\u{1f}")
    }

    pub fn get(&self, key: &str) -> Option<&str> {
        self.entries.get(key).map(|(_, v)| v.as_str())
    }

    /// Keeps `value` under `key`, dropping the oldest answer when there are more than
    /// [`CACHE_CAP`].
    pub fn insert(&mut self, key: String, value: String) {
        self.entries.insert(key, (self.next, value));
        self.next = self.next.saturating_add(1);
        while self.entries.len() > CACHE_CAP {
            let Some(oldest) = self.entries.iter().min_by_key(|(_, (n, _))| *n).map(|(k, _)| k.clone()) else { break };
            self.entries.remove(&oldest);
        }
    }

    pub fn clear(&mut self) {
        self.entries.clear();
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// The cache saved at `path`; empty when the file is missing or unreadable, since a cache is
    /// only a shortcut.
    pub fn load(path: &Path) -> Cache {
        let mut cache = Cache::default();
        let Ok(text) = std::fs::read_to_string(path) else { return cache };
        let Ok(serde_json::Value::Array(pairs)) = serde_json::from_str(&text) else { return cache };
        for pair in pairs.iter().take(CACHE_CAP) {
            if let (Some(k), Some(v)) = (pair.get(0).and_then(|k| k.as_str()), pair.get(1).and_then(|v| v.as_str())) {
                cache.insert(k.to_owned(), v.to_owned());
            }
        }
        cache
    }

    /// Writes the cache to `path`, oldest first, through a temporary file so a crash never leaves
    /// half a file.
    pub fn save(&self, path: &Path) -> Result<(), AiError> {
        let mut entries: Vec<_> = self.entries.iter().collect();
        entries.sort_by_key(|(_, (n, _))| *n);
        let pairs: Vec<_> = entries.into_iter().map(|(k, (_, v))| serde_json::json!([k, v])).collect();
        let text = serde_json::Value::Array(pairs).to_string();
        let mut tmp = path.as_os_str().to_owned();
        tmp.push(".tmp");
        std::fs::write(&tmp, text)?;
        std::fs::rename(&tmp, path)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ProviderConfig, Request};

    fn req(sentence: &str) -> Request {
        Request { word: "ephemeral".into(), sentence: sentence.into(), language: "English".into() }
    }

    #[test]
    fn cache_evicts_oldest_over_cap() {
        let mut c = Cache::default();
        for i in 0..=CACHE_CAP {
            c.insert(format!("k{i}"), format!("v{i}"));
        }
        assert_eq!(c.len(), CACHE_CAP);
        assert_eq!(c.get("k0"), None);
        assert_eq!(c.get(&format!("k{CACHE_CAP}")), Some(format!("v{CACHE_CAP}").as_str()));
    }

    #[test]
    fn cache_round_trips_through_disk() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(CACHE_FILE);
        let mut c = Cache::default();
        c.insert("a".into(), "first".into());
        c.insert("b".into(), "second".into());
        c.save(&path).unwrap();
        let back = Cache::load(&path);
        assert_eq!((back.get("a"), back.get("b"), back.len()), (Some("first"), Some("second"), 2));
    }

    #[test]
    fn corrupt_cache_loads_empty() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(CACHE_FILE);
        std::fs::write(&path, "{not json").unwrap();
        assert_eq!(Cache::load(&path).len(), 0);
        assert_eq!(Cache::load(&dir.path().join("missing.json")).len(), 0);
    }

    #[test]
    fn save_to_unwritable_dir_is_an_error_not_a_panic() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("no-such-folder").join(CACHE_FILE);
        assert!(matches!(Cache::default().save(&path), Err(crate::AiError::Io(_))));
    }

    #[test]
    fn key_changes_with_sentence() {
        let cfg = ProviderConfig::default();
        assert_ne!(Cache::key(&req("It was ephemeral."), &cfg), Cache::key(&req("Fame is ephemeral."), &cfg));
        let other = ProviderConfig { model: "sonnet".into(), ..ProviderConfig::default() };
        assert_ne!(Cache::key(&req("It was ephemeral."), &cfg), Cache::key(&req("It was ephemeral."), &other));
    }

    #[test]
    fn clear_empties_it() {
        let mut c = Cache::default();
        c.insert("a".into(), "b".into());
        c.clear();
        assert_eq!(c.len(), 0);
    }
}
