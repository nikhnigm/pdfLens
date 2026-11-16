//! A lookup from request to answer.

use crate::{AiError, Cache, ProviderConfig, Request};
use std::sync::Mutex;
use std::sync::atomic::AtomicBool;

/// A meaning, and whether it came from the cache.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Definition {
    pub text: String,
    pub cached: bool,
}

/// The meaning of `req.word` in `req.sentence`, from `cache` when it has it, otherwise from the
/// provider (which then fills the cache). A poisoned cache lock is treated as no cache.
pub fn define(req: &Request, cfg: &ProviderConfig, cache: Option<&Mutex<Cache>>, cancel: &AtomicBool) -> Result<Definition, AiError> {
    let key = Cache::key(req, cfg);
    if let Some(text) = cache.and_then(|c| c.lock().ok()).and_then(|c| c.get(&key).map(str::to_owned)) {
        return Ok(Definition { text, cached: true });
    }
    let text = ask(req, cfg, cancel)?;
    if let Some(mut c) = cache.and_then(|c| c.lock().ok()) {
        c.insert(key, text.clone());
    }
    Ok(Definition { text, cached: false })
}

#[cfg(not(target_arch = "wasm32"))]
fn ask(req: &Request, cfg: &ProviderConfig, cancel: &AtomicBool) -> Result<String, AiError> {
    let raw = crate::run(&crate::invocation(cfg)?, &crate::prompt(req), crate::TIMEOUT, cancel)?;
    crate::tidy_answer(&raw)
}

#[cfg(target_arch = "wasm32")]
fn ask(_: &Request, _: &ProviderConfig, _: &AtomicBool) -> Result<String, AiError> {
    Err(AiError::Unsupported)
}
