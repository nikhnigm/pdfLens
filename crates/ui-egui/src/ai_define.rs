//! Define: the meaning of a selected word, from the user's own AI tool (bring your own model).
//!
//! Off by default. Nothing is sent anywhere until the user turns it on in Preferences ▸ AI.

use crate::{DocView, PdfCraftApp};
use egui::Pos2;
use pdfcraft_ai::{AiError, CACHE_FILE, Cache, Definition, ProviderConfig, ProviderKind, Request, define, is_definable, sentence_around};
use serde_json::{Value, json};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, mpsc};

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
    /// Choose the provider. Another provider starts from its own default model (haiku for Claude,
    /// the provider's own for the others): a model name is one provider's.
    pub fn set_provider_kind(&mut self, kind: ProviderKind) {
        if kind == self.provider.kind {
            return;
        }
        self.provider.kind = kind;
        self.provider.model = if kind == ProviderKind::Claude { ProviderConfig::default().model } else { String::new() };
    }

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
    pub(crate) fn path(&self) -> Option<PathBuf> {
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

/// The most glyphs a selection Define looks up has: as many as the characters it allows.
const MAX_GLYPHS: usize = 64;

/// The selected words, when Define can look them up: up to three words on one line of one page.
/// Also the text layer and the selection's glyph range, for the sentence around it.
fn selected_words(view: &DocView) -> Option<(Arc<pdfcraft_render::PageText>, std::ops::Range<usize>, String)> {
    let (_, text, glyphs) = view.selected_glyphs()?;
    if glyphs.len() > MAX_GLYPHS {
        return None;
    }
    let range = *glyphs.first()?..glyphs.last()?.saturating_add(1);
    let raw = text.text_of(range.clone());
    let word = raw.trim();
    is_definable(word).then(|| word.to_owned()).map(|word| (text, range, word))
}

/// Whether the selection is one Define looks up (the selection bar then offers it).
pub(crate) fn can_define(view: &DocView) -> bool {
    selected_words(view).is_some()
}

/// The selected word and the sentence it is in, when Define can look it up.
pub(crate) fn define_target(view: &DocView) -> Option<(String, String)> {
    let (text, range, word) = selected_words(view)?;
    // `text_of` adds the spaces and line breaks between the glyphs it is given, not those at its
    // ends: they are put back on both sides of the selection, as the page's whole text has them.
    let before = text.text_of(0..range.start) + gap_before(&text, range.start);
    let after = gap_before(&text, range.end).to_owned() + &text.text_of(range.end..text.glyphs.len());
    Some((word.clone(), sentence_around(&before, &word, &after)))
}

/// What `PageText::text_of` puts before glyph `i`: a line break at a new line, a space at a word
/// break, nothing at the page's start or end.
fn gap_before(text: &pdfcraft_render::PageText, i: usize) -> &'static str {
    let (Some(line), Some(prev)) = (text.line_of.get(i), i.checked_sub(1).and_then(|p| text.line_of.get(p))) else { return "" };
    if line != prev {
        "\n"
    } else if text.space_before.get(i) == Some(&true) {
        " "
    } else {
        ""
    }
}

/// A lookup running on its own thread. Dropping it stops the provider.
struct DefineJob {
    receiver: mpsc::Receiver<Result<Definition, AiError>>,
    cancel: Arc<AtomicBool>,
}

impl Drop for DefineJob {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
}

/// What the Define popup shows.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DefineState {
    /// The provider hasn't answered yet.
    Loading,
    Answer(String),
    /// Why there is no answer, for the user.
    Error(String),
}

/// The Define popup: the word, its meaning and where it is on screen.
pub struct DefinePopup {
    pos: Option<Pos2>,
    placed: bool,
    word: String,
    state: DefineState,
    job: Option<DefineJob>,
}

impl DefinePopup {
    pub fn word(&self) -> &str {
        &self.word
    }

    pub fn state(&self) -> &DefineState {
        &self.state
    }

    /// The popup's top-left corner on screen, once it has one.
    pub fn pos(&self) -> Option<Pos2> {
        self.pos
    }
}

impl PdfCraftApp {
    /// Where Define keeps its answers between runs (the settings folder). `None` keeps them in
    /// memory only.
    pub fn set_ai_cache_dir(&mut self, dir: Option<PathBuf>) {
        self.ai_cache = AiCache { dir, cache: None };
    }

    /// Edit ▸ Define selection: look up the word selected in view `index` with the user's
    /// provider, in a popup. A lookup still running is stopped.
    pub(crate) fn start_define(&mut self, index: usize) -> bool {
        let Some(view) = self.views.get_mut(index) else { return false };
        let Some((word, sentence)) = define_target(view) else {
            self.notify_tr("Select up to three words on one page to define");
            return false;
        };
        // Below the selection, where its action bar was: the bar closes and the popup takes its place.
        let anchor = view.selection_end_rect().map(|r| r.left_bottom() + egui::vec2(0.0, 8.0));
        view.dismiss_selection_actions();
        let language = match self.ai.language.trim() {
            "" => DEFAULT_LANGUAGE.to_owned(),
            l => l.to_owned(),
        };
        let request = Request { word: word.clone(), sentence, language };
        let provider = self.ai.provider.clone();
        let cache = self.ai_cache.shared();
        let path = self.ai_cache.path();
        let (sender, receiver) = mpsc::channel();
        let cancel = Arc::new(AtomicBool::new(false));
        let flag = cancel.clone();
        let ctx = self.ctx.clone();
        let work = move || {
            let result = define(&request, &provider, Some(&cache), &flag);
            // A new answer is kept for next time; failing to write it only costs a lookup.
            if let (Ok(Definition { cached: false, .. }), Some(path)) = (&result, path)
                && let Ok(cache) = cache.lock()
                && let Err(e) = cache.save(&path)
            {
                log::warn!("could not save the Define cache: {e}");
            }
            let _ = sender.send(result);
            if let Some(ctx) = ctx {
                ctx.request_repaint();
            }
        };
        let job = DefineJob { receiver, cancel };
        let mut popup = DefinePopup { pos: self.define_pos.or(anchor), placed: false, word, state: DefineState::Loading, job: Some(job) };
        if let Err(e) = std::thread::Builder::new().name("pdfcraft-ai-define".into()).spawn(work) {
            popup.job = None;
            popup.state = DefineState::Error(e.to_string());
        }
        // Replacing the popup drops its job, which stops that lookup.
        self.define_popup = Some(popup);
        true
    }
}

enum Choice {
    Close,
    Copy(String),
}

/// Draw the Define popup, if one is open, and take in the provider's answer.
pub(crate) fn show(app: &mut PdfCraftApp, ctx: &egui::Context) {
    let Some(popup) = app.define_popup.as_mut() else { return };
    if let Some(job) = &popup.job {
        let received = match job.receiver.try_recv() {
            Ok(result) => Some(result.map(|d| d.text).map_err(|e| e.to_string())),
            Err(mpsc::TryRecvError::Empty) => None,
            Err(mpsc::TryRecvError::Disconnected) => Some(Err(tl!("The lookup stopped unexpectedly.").to_string())),
        };
        if let Some(result) = received {
            popup.state = match result {
                Ok(answer) => DefineState::Answer(answer),
                Err(why) => DefineState::Error(why),
            };
            popup.job = None;
        }
    }
    let mut open = true;
    let mut choice = None;
    let mut window = egui::Window::new(tl!("Define"))
        .id(egui::Id::unique("ai-define-popup"))
        .open(&mut open)
        .collapsible(false)
        .resizable(false)
        .order(egui::Order::Foreground)
        .default_width(320.0)
        .max_width(320.0);
    // Placed once, where it was last dragged to or by the selection; then it is the user's to move.
    if !popup.placed
        && let Some(pos) = popup.pos
    {
        window = window.current_pos(pos);
    }
    let shown = window.show(ctx, |ui| {
        ui.label(egui::RichText::new(&popup.word).strong());
        ui.add_space(4.0);
        match &popup.state {
            DefineState::Loading => {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label(tl!("Looking up…"));
                    if ui.button(tl!("Cancel")).clicked() {
                        choice = Some(Choice::Close);
                    }
                });
            }
            DefineState::Answer(answer) => {
                ui.label(answer);
                ui.add_space(6.0);
                if ui.button(tl!("Copy")).clicked() {
                    choice = Some(Choice::Copy(answer.clone()));
                }
            }
            DefineState::Error(why) => {
                ui.colored_label(ui.visuals().error_fg_color, why);
            }
        }
    });
    if let Some(shown) = shown {
        let at = shown.response.rect.min;
        // Moved by the user, not by being placed: later lookups open there.
        if popup.placed && popup.pos != Some(at) && ctx.input(|i| i.pointer.primary_down()) {
            app.define_pos = Some(at);
        }
        popup.pos = Some(at);
    }
    popup.placed = true;
    // Escape closes it, unless a text field has the keyboard.
    let escape = !ctx.egui_wants_keyboard_input() && ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape));
    match choice {
        Some(Choice::Copy(text)) => ctx.copy_text(text),
        Some(Choice::Close) => app.define_popup = None,
        None => {}
    }
    if !open || escape {
        app.define_popup = None;
    }
}
