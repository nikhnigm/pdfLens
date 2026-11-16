//! Define: the meaning of a selected word, from the user's own AI tool (bring your own model).
//!
//! Off by default. Nothing is sent anywhere until the user turns it on in Preferences ▸ AI.

use pdfcraft_ai::{CACHE_FILE, Cache, ProviderConfig, ProviderKind};
use serde_json::{Value, json};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

/// What the user is told the first time they turn Define on, and below the switch.
pub(crate) const NOTICE: &str = "Selected words and their sentence are sent to the provider you choose.";

/// The answer language when none is set.
const DEFAULT_LANGUAGE: &str = "English";

/// The AI preferences.
#[derive(Clone, Debug, PartialEq)]
pub struct AiPrefs {
    /// Whether Define may run at all.
    pub enabled: bool,
    pub provider: ProviderConfig,
    /// The language answers are written in.
    pub language: String,
    /// Whether the user has seen [`NOTICE`] after turning Define on.
    pub notice_shown: bool,
}

impl Default for AiPrefs {
    fn default() -> Self {
        Self { enabled: false, provider: ProviderConfig::default(), language: DEFAULT_LANGUAGE.into(), notice_shown: false }
    }
}

impl AiPrefs {
    pub(crate) fn to_json(&self) -> Value {
        let p = &self.provider;
        json!({
            "enabled": self.enabled,
            "provider": p.kind.id(),
            "model": p.model,
            "program": p.program,
            "custom": p.custom,
            "language": self.language,
            "notice_shown": self.notice_shown,
        })
    }

    /// The preferences saved in `v`; a missing or damaged value keeps its default.
    pub(crate) fn from_json(v: &Value) -> AiPrefs {
        let mut prefs = AiPrefs::default();
        let text = |key: &str| v[key].as_str().map(str::to_owned);
        if let Some(on) = v["enabled"].as_bool() {
            prefs.enabled = on;
        }
        if let Some(seen) = v["notice_shown"].as_bool() {
            prefs.notice_shown = seen;
        }
        if let Some(language) = text("language").filter(|l| !l.trim().is_empty()) {
            prefs.language = language;
        }
        // An unknown provider keeps the default provider whole, not its model with another kind.
        if let Some(kind) = v["provider"].as_str().and_then(ProviderKind::from_id) {
            prefs.provider.kind = kind;
            prefs.provider.model = text("model").unwrap_or_default();
            prefs.provider.program = text("program").unwrap_or_default();
            prefs.provider.custom = text("custom").unwrap_or_default();
        }
        prefs
    }
}

/// The answers already looked up, kept in the settings folder and loaded on first use.
#[derive(Default)]
pub(crate) struct AiCache {
    dir: Option<PathBuf>,
    cache: Option<Arc<Mutex<Cache>>>,
}

impl AiCache {
    fn path(&self) -> Option<PathBuf> {
        self.dir.as_ref().map(|d| d.join(CACHE_FILE))
    }

    /// The cache, loaded from disk the first time. Without a settings folder it lives in memory.
    pub(crate) fn shared(&mut self) -> Arc<Mutex<Cache>> {
        let path = self.path();
        self.cache.get_or_insert_with(|| Arc::new(Mutex::new(path.as_deref().map(Cache::load).unwrap_or_default()))).clone()
    }

    /// Writes the cache to disk. A failure is only logged: the cache is a shortcut, not data.
    pub(crate) fn save(&self) {
        let (Some(path), Some(cache)) = (self.path(), self.cache.as_ref()) else { return };
        let Ok(cache) = cache.lock() else { return };
        if let Err(e) = cache.save(&path) {
            log::warn!("could not save the Define cache: {e}");
        }
    }

    /// Forgets every answer, on disk too.
    pub(crate) fn clear(&mut self) {
        if let Ok(mut cache) = self.shared().lock() {
            cache.clear();
        }
        self.save();
    }
}

impl crate::PdfCraftApp {
    /// Where Define keeps its answers between runs (the settings folder). `None` keeps them in
    /// memory only.
    pub fn set_ai_cache_dir(&mut self, dir: Option<PathBuf>) {
        self.ai_cache = AiCache { dir, cache: None };
    }
}
