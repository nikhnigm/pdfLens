//! pdfcraft-ai — bring-your-own-model lookups (L4).
//!
//! PdfCraft has no model of its own. A lookup hands a prompt to a program the user chose (the
//! Claude CLI, the Codex CLI or any command) on stdin and reads the answer from stdout. Nothing
//! runs unless the user turns the feature on.
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

pub mod cache;
pub mod context;
pub mod define;
pub mod error;
pub mod models;
pub mod prompt;
pub mod provider;
#[cfg(not(target_arch = "wasm32"))]
pub mod run;

pub use cache::{CACHE_CAP, CACHE_FILE, Cache};
pub use context::{MAX_SENTENCE, is_definable, sentence_around};
pub use define::{Definition, define};
pub use error::AiError;
pub use models::{CODEX_MODELS_CACHE, ModelChoice, claude_models, codex_home, codex_models, codex_models_from_cache, codex_models_in};
pub use prompt::{Request, prompt, split_command_line, tidy_answer};
pub use provider::{Invocation, ProviderConfig, ProviderKind, TIMEOUT, invocation};
#[cfg(not(target_arch = "wasm32"))]
pub use run::run;
