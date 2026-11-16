//! What can go wrong with a lookup, worded so the user knows what to do next.

/// A lookup that could not produce an answer.
#[derive(Debug, thiserror::Error)]
pub enum AiError {
    /// The command line in Preferences can't be used as written.
    #[error("The command can't be used: {0}. Check Preferences > AI.")]
    BadCommand(String),
    /// The provider's program isn't installed or isn't on PATH.
    #[error("Couldn't find `{0}`. Install it, or set its full path in Preferences > AI.")]
    NotFound(String),
    /// The provider took longer than the limit.
    #[error("The provider didn't answer within {0} seconds. Try again, or choose a faster model.")]
    Timeout(u64),
    /// The user cancelled the lookup.
    #[error("The lookup was cancelled.")]
    Cancelled,
    /// The provider ran and exited with an error.
    #[error("`{program}` failed{}: {stderr_tail}", code.map(|c| format!(" (exit code {c})")).unwrap_or_default())]
    Failed { program: String, code: Option<i32>, stderr_tail: String },
    /// The provider answered with nothing usable.
    #[error("The provider gave an empty answer. Try again, or try another model.")]
    Empty,
    /// Reading from or writing to the provider, or the cache, failed.
    #[error("{0}")]
    Io(#[from] std::io::Error),
    /// Lookups need a desktop build (the web build can't run programs).
    #[error("Define needs the desktop app: the web version can't run your AI tool.")]
    Unsupported,
}
