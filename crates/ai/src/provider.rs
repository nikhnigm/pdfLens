//! Which program answers a lookup, and the exact command line it gets.

use crate::{AiError, split_command_line};
use std::time::Duration;

/// How long a lookup may take before it is stopped.
pub const TIMEOUT: Duration = Duration::from_secs(60);

/// The kind of program that answers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProviderKind {
    /// Anthropic's Claude Code CLI (`claude`).
    Claude,
    /// OpenAI's Codex CLI (`codex`).
    Codex,
    /// Any command that reads a prompt on stdin and writes the answer to stdout.
    Custom,
}

impl ProviderKind {
    /// The name used in settings and in the `ai_define` tool.
    pub fn id(self) -> &'static str {
        match self {
            Self::Claude => "claude",
            Self::Codex => "codex",
            Self::Custom => "custom",
        }
    }

    /// The kind named `id`, if any.
    pub fn from_id(id: &str) -> Option<Self> {
        [Self::Claude, Self::Codex, Self::Custom].into_iter().find(|k| k.id() == id)
    }
}

/// The user's choice of provider.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProviderConfig {
    pub kind: ProviderKind,
    /// The model to ask for; empty leaves it to the provider.
    pub model: String,
    /// The program's path, for Claude and Codex; empty finds `claude` or `codex` on PATH.
    pub program: String,
    /// The whole command line, for [`ProviderKind::Custom`].
    pub custom: String,
}

impl Default for ProviderConfig {
    fn default() -> Self {
        Self { kind: ProviderKind::Claude, model: "haiku".into(), program: String::new(), custom: String::new() }
    }
}

/// A program and its arguments. The prompt is not among them: it goes on stdin.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Invocation {
    pub program: String,
    pub args: Vec<String>,
}

/// The command line for `cfg`. Claude runs with no tools and no MCP servers, and Codex in its
/// read-only sandbox, since a definition needs neither files nor a shell.
pub fn invocation(cfg: &ProviderConfig) -> Result<Invocation, AiError> {
    let program = |default: &str| if cfg.program.trim().is_empty() { default.to_owned() } else { cfg.program.trim().to_owned() };
    let model = cfg.model.trim();
    match cfg.kind {
        ProviderKind::Claude => {
            let mut args = vec!["-p".to_owned()];
            if !model.is_empty() {
                args.extend(["--model".to_owned(), model.to_owned()]);
            }
            args.extend(["--tools", "", "--strict-mcp-config", "--disable-slash-commands", "--no-session-persistence"].map(String::from));
            Ok(Invocation { program: program("claude"), args })
        }
        ProviderKind::Codex => {
            let mut args: Vec<String> = ["exec", "--sandbox", "read-only", "--skip-git-repo-check", "--ephemeral"].map(String::from).into();
            if !model.is_empty() {
                args.extend(["-m".to_owned(), model.to_owned()]);
            }
            args.push("-".to_owned());
            Ok(Invocation { program: program("codex"), args })
        }
        ProviderKind::Custom => {
            let mut parts = split_command_line(&cfg.custom)?.into_iter();
            let program = parts.next().ok_or_else(|| AiError::BadCommand("it is empty".into()))?;
            Ok(Invocation { program, args: parts.collect() })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(cfg: &ProviderConfig) -> (String, Vec<String>) {
        let inv = invocation(cfg).unwrap();
        (inv.program, inv.args)
    }

    #[test]
    fn invocation_claude_default() {
        let (program, a) = args(&ProviderConfig::default());
        assert_eq!(program, "claude");
        assert_eq!(a, ["-p", "--model", "haiku", "--tools", "", "--strict-mcp-config", "--disable-slash-commands", "--no-session-persistence"]);
    }

    #[test]
    fn invocation_claude_without_model() {
        let cfg = ProviderConfig { model: String::new(), ..ProviderConfig::default() };
        assert!(!args(&cfg).1.contains(&"--model".to_owned()));
    }

    #[test]
    fn invocation_codex_without_model() {
        let cfg = ProviderConfig { kind: ProviderKind::Codex, model: String::new(), ..ProviderConfig::default() };
        let (program, a) = args(&cfg);
        assert_eq!(program, "codex");
        assert_eq!(a, ["exec", "--sandbox", "read-only", "--skip-git-repo-check", "--ephemeral", "-"]);
    }

    #[test]
    fn invocation_codex_with_model() {
        let cfg = ProviderConfig { kind: ProviderKind::Codex, model: "gpt-5".into(), ..ProviderConfig::default() };
        assert_eq!(args(&cfg).1, ["exec", "--sandbox", "read-only", "--skip-git-repo-check", "--ephemeral", "-m", "gpt-5", "-"]);
    }

    #[test]
    fn invocation_custom_uses_split_line() {
        let cfg = ProviderConfig { kind: ProviderKind::Custom, custom: r#""C:\AI\llm.exe" -m tiny"#.into(), ..ProviderConfig::default() };
        assert_eq!(args(&cfg), ("C:\\AI\\llm.exe".to_owned(), vec!["-m".to_owned(), "tiny".to_owned()]));
        let empty = ProviderConfig { kind: ProviderKind::Custom, ..ProviderConfig::default() };
        assert!(matches!(invocation(&empty), Err(crate::AiError::BadCommand(_))));
    }

    #[test]
    fn program_override_wins() {
        let cfg = ProviderConfig { program: r"D:\tools\claude.cmd".into(), ..ProviderConfig::default() };
        assert_eq!(args(&cfg).0, r"D:\tools\claude.cmd");
    }

    #[test]
    fn kind_ids_round_trip() {
        for kind in [ProviderKind::Claude, ProviderKind::Codex, ProviderKind::Custom] {
            assert_eq!(ProviderKind::from_id(kind.id()), Some(kind));
        }
        assert_eq!(ProviderKind::from_id("gemini"), None);
    }
}
