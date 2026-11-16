//! Define selection: the bring-your-own-model lookup of the selected word.

use pdfcraft_ai::{ProviderConfig, ProviderKind};
use pdfcraft_ui_egui::{AiPrefs, PdfCraftApp};

#[test]
fn ai_prefs_default_off() {
    let app = PdfCraftApp::new();
    assert!(!app.ai.enabled);
    assert_eq!(app.ai.language, "English");
    assert_eq!(app.ai.provider, ProviderConfig::default());
}

#[test]
fn ai_prefs_round_trip() {
    let mut app = PdfCraftApp::new();
    app.ai = AiPrefs {
        enabled: true,
        provider: ProviderConfig {
            kind: ProviderKind::Codex,
            model: "gpt-5".into(),
            program: r"D:\tools\codex.cmd".into(),
            custom: "ollama run x".into(),
        },
        language: "Hindi".into(),
        notice_shown: true,
    };
    let json = app.persist();
    let mut back = PdfCraftApp::new();
    back.restore(&json);
    assert_eq!(back.ai, app.ai);
}

#[test]
fn ai_prefs_ignore_damaged_values() {
    let mut app = PdfCraftApp::new();
    app.restore(r#"{"ai":{"enabled":"yes","provider":"gemini","language":7}}"#);
    assert_eq!(app.ai, AiPrefs::default());
}
