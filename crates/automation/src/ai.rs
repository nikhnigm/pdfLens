//! `ai_define`: the meaning of a word in its sentence, from the user's own model. `ai_models`:
//! the models Preferences ▸ AI offers for a provider.

use super::error::{Result, ToolError, failed};
use super::json::Args;
use super::{Automation, Value, json};
use pdfcraft_ai::{ProviderConfig, ProviderKind, Request, define, is_definable};
use std::sync::atomic::AtomicBool;

impl Automation {
    /// Asks the chosen provider for the meaning of `word` in `sentence`. Nothing is cached here:
    /// the cache belongs to the app.
    pub(crate) fn ai_define(&self, a: &Args) -> Result<Value> {
        let word = a.str("word")?;
        if !is_definable(word) {
            return Err(ToolError::InvalidArgs("word must be 1–3 words on one line".into()));
        }
        let id = a.opt_str("provider")?.unwrap_or("claude");
        let kind = ProviderKind::from_id(id)
            .ok_or_else(|| ToolError::InvalidArgs(format!("provider {id:?} is unknown; use one of claude, codex, custom")))?;
        let custom = a.opt_str("command")?.unwrap_or_default();
        if kind == ProviderKind::Custom && custom.trim().is_empty() {
            return Err(ToolError::InvalidArgs("provider custom needs command, the command line to run".into()));
        }
        let mut cfg =
            ProviderConfig { kind, program: a.opt_str("program")?.unwrap_or_default().into(), custom: custom.into(), ..ProviderConfig::default() };
        if let Some(model) = a.opt_str("model")? {
            cfg.model = model.into();
        } else if kind != ProviderKind::Claude {
            cfg.model.clear();
        }
        let req =
            Request { word: word.trim().into(), sentence: a.str("sentence")?.into(), language: a.opt_str("language")?.unwrap_or("English").into() };
        let answer = define(&req, &cfg, None, &AtomicBool::new(false)).map_err(failed)?;
        Ok(json!({ "meaning": answer.text }))
    }

    /// The models Preferences ▸ AI lists for `provider`: Claude's aliases, or the models Codex's own
    /// cache on this machine lists. `default` is what an empty model means for it.
    pub(crate) fn ai_models(&self, a: &Args) -> Result<Value> {
        let id = a.str("provider")?;
        let (models, default) = match ProviderKind::from_id(id) {
            Some(ProviderKind::Claude) => (pdfcraft_ai::claude_models(), ProviderConfig::default().model),
            Some(ProviderKind::Codex) => (pdfcraft_ai::codex_models(), String::new()),
            _ => return Err(ToolError::InvalidArgs(format!("provider {id:?} has no model list; use one of claude, codex"))),
        };
        let models: Vec<Value> = models.into_iter().map(|m| json!({ "id": m.id, "label": m.label, "note": m.note })).collect();
        Ok(json!({ "models": models, "default": default }))
    }
}
