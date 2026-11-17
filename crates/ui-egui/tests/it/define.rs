//! Define selection: the bring-your-own-model lookup of the selected word.

use egui::{Key, Modifiers, pos2};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use pdfcraft_ai::{ProviderConfig, ProviderKind};
use pdfcraft_ui_egui::{AiPrefs, DefineState, Dialog, PdfCraftApp};

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

/// One 300×400 page with a line of Helvetica text at (40, 150).
fn fixture() -> Vec<u8> {
    fixture_with("BT /F1 14 Tf 40 150 Td (It was an ephemeral joy. Then it was gone.) Tj ET")
}

/// One 300×400 page drawing `body` with Helvetica as /F1.
fn fixture_with(body: &str) -> Vec<u8> {
    let objs = [
        "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 300 400] /Contents 4 0 R /Resources << /Font << /F1 5 0 R >> >> >>".to_string(),
        format!("<< /Length {} >>\nstream\n{body}\nendstream", body.len()),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_string(),
    ];
    let mut out = b"%PDF-1.7\n".to_vec();
    let mut offsets = Vec::new();
    for (i, o) in objs.iter().enumerate() {
        offsets.push(out.len());
        out.extend_from_slice(format!("{} 0 obj\n{o}\nendobj\n", i + 1).as_bytes());
    }
    let xref = out.len();
    out.extend_from_slice(format!("xref\n0 {}\n0000000000 65535 f \n", objs.len() + 1).as_bytes());
    for o in offsets {
        out.extend_from_slice(format!("{o:010} 00000 n \n").as_bytes());
    }
    out.extend_from_slice(format!("trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n", objs.len() + 1).as_bytes());
    out
}

/// A provider that answers "Brief." to anything, without Claude or Codex.
fn echo() -> String {
    if cfg!(windows) { "cmd /C echo Brief.".into() } else { "sh -c \"echo Brief.\"".into() }
}

fn harness(on: bool) -> Harness<'static, PdfCraftApp> {
    harness_with(fixture(), echo(), on)
}

fn harness_with(pdf: Vec<u8>, command: String, on: bool) -> Harness<'static, PdfCraftApp> {
    let mut h = Harness::builder().with_size(egui::vec2(1400.0, 900.0)).build_eframe(move |_cc| {
        let mut app = PdfCraftApp::new();
        app.set_option("language", "en").unwrap();
        app.open_bytes("text.pdf", None, pdf).expect("opens");
        app.ai.enabled = on;
        app.ai.provider = ProviderConfig { kind: ProviderKind::Custom, custom: command, ..ProviderConfig::default() };
        app
    });
    h.run_steps(4);
    crate::support::settle(&mut h);
    h
}

/// Selects "ephemeral" (glyphs 10 to 18 of "It was an ephemeral joy.").
fn select_word(h: &mut Harness<'static, PdfCraftApp>) {
    h.state_mut().views[0].select_text(0, 10, 18);
    h.run_steps(3);
}

fn state(app: &PdfCraftApp) -> Option<DefineState> {
    app.define_popup.as_ref().map(|p| p.state().clone())
}

#[test]
fn shortcut_defines_the_selected_word() {
    let mut h = harness(true);
    select_word(&mut h);
    h.key_press_modifiers(Modifiers::COMMAND | Modifiers::SHIFT, Key::D);
    crate::support::wait_until(&mut h, "the answer", |app| state(app) == Some(DefineState::Answer("Brief.".into())));
    assert_eq!(h.state().define_popup.as_ref().map(|p| p.word()), Some("ephemeral"));
}

#[test]
fn define_when_off_opens_preferences() {
    let mut h = harness(false);
    select_word(&mut h);
    assert!(h.state_mut().execute("ai.define"));
    h.run_steps(2);
    assert_eq!(h.state().dialog, Some(Dialog::Preferences));
    assert!(h.state().define_popup.is_none());
}

#[test]
fn define_button_shows_only_for_short_selections() {
    let mut h = harness(true);
    select_word(&mut h);
    assert!(h.query_by_label("Define").is_some(), "one word offers Define");
    // "It was an ephemeral joy.": five words.
    h.state_mut().views[0].select_text(0, 0, 23);
    h.run_steps(3);
    assert!(h.query_by_label("Copy").is_some(), "the bar is up");
    assert!(h.query_by_label("Define").is_none(), "five words don't offer Define");
}

#[test]
fn define_button_looks_the_word_up() {
    let mut h = harness(true);
    select_word(&mut h);
    h.get_by_label("Define").click();
    crate::support::wait_until(&mut h, "the answer", |app| state(app) == Some(DefineState::Answer("Brief.".into())));
}

#[test]
fn new_lookup_reuses_dragged_position() {
    let mut h = harness(true);
    let at = pos2(500.0, 300.0);
    h.state_mut().define_pos = Some(at);
    select_word(&mut h);
    assert!(h.state_mut().execute("ai.define"));
    h.run_steps(3);
    assert_eq!(h.state().define_popup.as_ref().and_then(|p| p.pos()), Some(at));
    assert_eq!(h.state().define_pos, Some(at));
}

#[test]
fn escape_closes_the_popup() {
    let mut h = harness(true);
    select_word(&mut h);
    assert!(h.state_mut().execute("ai.define"));
    h.run_steps(2);
    assert!(h.state().define_popup.is_some());
    h.key_press(Key::Escape);
    h.run_steps(2);
    assert!(h.state().define_popup.is_none());
}

/// Words spaced by TJ offsets, not space characters: the gaps around the selection reach the model
/// as spaces, as everywhere else in the sentence.
#[test]
fn the_sentence_keeps_the_spaces_around_the_word() {
    let pdf = fixture_with("BT /F1 14 Tf 40 150 Td [(The) -300 (effect) -300 (was) -300 (ephemeral) -300 (at) -300 (best!)] TJ ET");
    // A provider that answers with its own prompt, so the test sees what was sent.
    let cat = if cfg!(windows) { "cmd /C more" } else { "cat" };
    let mut h = harness_with(pdf, cat.into(), true);
    // "ephemeral" is glyphs 12 to 20: the gaps are not glyphs.
    h.state_mut().views[0].select_text(0, 12, 20);
    h.run_steps(3);
    assert!(h.state_mut().execute("ai.define"));
    crate::support::wait_until(&mut h, "the prompt back", |app| matches!(state(app), Some(DefineState::Answer(_) | DefineState::Error(_))));
    let Some(DefineState::Answer(sent)) = state(h.state()) else { panic!("{:?}", state(h.state())) };
    assert!(sent.contains("\"The effect was ephemeral at best!\""), "{sent}");
}

/// A Claude model name means nothing to Codex: switching provider resets it to that provider's
/// default (haiku for Claude, the provider's own for the others).
#[test]
fn switching_provider_resets_the_model() {
    let mut prefs = AiPrefs::default();
    prefs.set_provider_kind(ProviderKind::Codex);
    assert_eq!((prefs.provider.kind, prefs.provider.model.as_str()), (ProviderKind::Codex, ""));
    prefs.set_provider_kind(ProviderKind::Claude);
    assert_eq!(prefs.provider.model, "haiku");
    prefs.provider.model = "sonnet".into();
    prefs.set_provider_kind(ProviderKind::Claude);
    assert_eq!(prefs.provider.model, "sonnet", "choosing the same provider keeps the model");
}
