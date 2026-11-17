//! Define: the meaning of a selected word, from the user's own AI tool (bring your own model).
//!
//! Off by default. Nothing is sent anywhere until the user turns it on in Preferences ▸ AI.

use crate::theme::{self, Tokens};
use crate::{Dialog, DocView, PdfCraftApp};
use egui::{Color32, CornerRadius, Pos2, Rect, Sense, Stroke, Vec2, pos2, vec2};
use pdfcraft_ai::{AiError, CACHE_FILE, Cache, Definition, ProviderConfig, ProviderKind, Request, define, is_definable, sentence_around};
use serde_json::{Value, json};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::time::Duration;

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

    /// How many answers are saved (Preferences ▸ AI shows it by Clear cache).
    pub(crate) fn len(&mut self) -> usize {
        self.shared().lock().map_or(0, |cache| cache.len())
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

/// The card's width.
const CARD_WIDTH: f32 = 320.0;
/// The card's corner radius.
const CARD_RADIUS: u8 = 10;
/// Between the selection and the card (the pointer sits in it).
const CARD_GAP: f32 = 12.0;
/// The pointer's width; it rises half of that beyond the card's edge.
const POINTER: f32 = 12.0;
/// The pointer's centre, from the card's left edge, when nothing pushes the card aside.
const POINTER_AT: f32 = 42.0;
/// Below this much room under the selection, the card opens above it.
const FLIP_ROOM: f32 = 160.0;
/// The header's height.
const HEADER: f32 = 44.0;
/// The content edge: header, body and footer text start this far in.
const EDGE: f32 = 16.0;
/// The quiet icon buttons (grip, close, copy, settings) and the footer's text buttons.
const ICON_BUTTON: f32 = 28.0;
/// How long the copy button shows its check, in seconds.
const COPIED_FOR: f64 = 1.5;
/// The card's height before it has been laid out once (a lookup's skeleton).
const HEIGHT_GUESS: f32 = 140.0;

/// The Define popup: the word, its meaning and where it is on screen.
pub struct DefinePopup {
    /// The card's top-left corner on screen, once it has one.
    pos: Option<Pos2>,
    /// The selection on screen, while the card is still anchored to it: not dragged away, and
    /// not opened where an earlier card was dragged.
    anchor: Option<Rect>,
    /// The card's size last frame, to place it above the selection.
    size: Option<Vec2>,
    word: String,
    /// The model named in the header ("haiku"), never the provider.
    model: String,
    /// Who is being asked, for the status line while loading.
    provider: String,
    state: DefineState,
    job: Option<DefineJob>,
    /// When the meaning was copied (egui time), for the check on the copy button.
    copied_at: Option<f64>,
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

    /// The model named in the header, empty when the provider chooses its own.
    pub fn model(&self) -> &str {
        &self.model
    }

    /// Whether the copy button shows its check (the meaning was just copied).
    pub fn copied(&self) -> bool {
        self.copied_at.is_some()
    }

    /// Whether the card sits above the selection it points at.
    pub fn above(&self) -> bool {
        match (self.anchor, self.pos) {
            (Some(anchor), Some(pos)) => pos.y < anchor.top(),
            _ => false,
        }
    }
}

/// The model the header names: the one set in Preferences, or haiku, Claude's default, when the
/// field is empty there. Nothing for Codex's own default, which PdfLens doesn't know, nor for a
/// custom command, which has no model setting.
pub(crate) fn model_label(provider: &ProviderConfig) -> String {
    match (provider.kind, provider.model.trim()) {
        (ProviderKind::Custom, _) => String::new(),
        (ProviderKind::Claude, "") => ProviderConfig::default().model,
        (_, model) => model.to_owned(),
    }
}

/// Who a lookup asks, for the status line: Claude, Codex, or a custom command's program.
pub(crate) fn provider_label(provider: &ProviderConfig) -> String {
    match provider.kind {
        ProviderKind::Claude => "Claude".to_owned(),
        ProviderKind::Codex => "Codex".to_owned(),
        ProviderKind::Custom => {
            let program = pdfcraft_ai::split_command_line(&provider.custom).ok().and_then(|words| words.into_iter().next()).unwrap_or_default();
            // The program's own name, without its folder or `.exe`, on every platform.
            let file = program.rsplit(['/', '\\']).next().unwrap_or_default();
            let name = std::path::Path::new(file).file_stem().and_then(|s| s.to_str()).unwrap_or_default();
            if name.is_empty() { tl!("your command").to_owned() } else { name.to_owned() }
        }
    }
}

/// Where the card goes for a selection at `anchor`, `size` big, inside `screen`: 12 below it, or
/// 12 above when less than 160 is left below; kept on screen. Also the pointer's x on screen.
pub(crate) fn place(anchor: Rect, size: Vec2, screen: Rect) -> (Pos2, f32) {
    let margin = 8.0;
    let aim = anchor.left() + (anchor.width() / 2.0).min(18.0);
    let left = (aim - POINTER_AT).clamp(screen.left() + margin, (screen.right() - size.x - margin).max(screen.left() + margin));
    let below = screen.bottom() - anchor.bottom() >= FLIP_ROOM;
    let top = if below { anchor.bottom() + CARD_GAP } else { (anchor.top() - CARD_GAP - size.y).max(screen.top() + margin) };
    // The pointer stays over the selection, and off the card's rounded corners.
    let reach = f32::from(CARD_RADIUS) + POINTER / 2.0;
    (pos2(left, top), aim.max(left + reach).min(left + size.x - reach))
}

/// `pos` moved, if need be, so a card `size` big stays inside `screen`.
fn keep_on_screen(pos: Pos2, size: Vec2, screen: Rect) -> Pos2 {
    let max = (screen.max - size).max(screen.min);
    pos2(pos.x.clamp(screen.left(), max.x), pos.y.clamp(screen.top(), max.y))
}

impl PdfCraftApp {
    /// Where Define keeps its answers between runs (the settings folder). `None` keeps them in
    /// memory only.
    pub fn set_ai_cache_dir(&mut self, dir: Option<PathBuf>) {
        self.ai_cache = AiCache { dir, cache: None };
    }

    /// Open Preferences at its AI section.
    pub(crate) fn open_ai_preferences(&mut self) {
        self.dialog = Some(Dialog::Preferences);
        self.prefs_show_ai = true;
    }

    /// Edit ▸ Define selection: look up the word selected in view `index` with the user's
    /// provider, in a popup. A lookup still running is stopped.
    pub(crate) fn start_define(&mut self, index: usize) -> bool {
        let Some(view) = self.views.get_mut(index) else { return false };
        let Some((word, sentence)) = define_target(view) else {
            self.notify_tr("Select up to three words on one page to define");
            return false;
        };
        // By the selection, where its action bar was: the bar closes and the card takes its place.
        let anchor = view.selection_end_rect();
        view.dismiss_selection_actions();
        let language = match self.ai.language.trim() {
            "" => DEFAULT_LANGUAGE.to_owned(),
            l => l.to_owned(),
        };
        let request = Request { word: word.clone(), sentence, language };
        let provider = self.ai.provider.clone();
        let (model, asking) = (model_label(&provider), provider_label(&provider));
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
        // Where the user last dragged a card wins over the selection.
        let anchor = if self.define_pos.is_some() { None } else { anchor };
        let mut popup = DefinePopup {
            pos: self.define_pos,
            anchor,
            size: None,
            word,
            model,
            provider: asking,
            state: DefineState::Loading,
            job: Some(job),
            copied_at: None,
        };
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
    Settings,
}

/// A quiet 28 × 28 icon button: grey until hovered, then the text colour on a hover wash.
/// `tooltip` is also its accessible name. `tint` overrides the colour (the copied check).
fn quiet_button(ui: &mut egui::Ui, rect: Rect, id: egui::Id, icon: &str, tooltip: &str, tint: Option<Color32>) -> egui::Response {
    let t = Tokens::get(ui.ctx());
    let resp = ui.interact(rect, id, Sense::click());
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, tooltip));
    let hot = resp.hovered() || resp.is_pointer_button_down_on();
    if hot {
        let fill = if resp.is_pointer_button_down_on() { t.pressed } else { t.hover };
        ui.painter().rect_filled(rect, CornerRadius::same(6), fill);
    }
    let color = tint.unwrap_or(if hot { t.text } else { t.icon_quiet });
    crate::icons::paint(ui, rect, icon, 16.0, color);
    theme::hand(resp).on_hover_text(tooltip)
}

/// A 28-tall text button with its label 10 in from its edge: bordered, or a ghost one (no
/// border, a wash when hovered).
fn text_button(ui: &mut egui::Ui, at: Pos2, id: egui::Id, label: &str, bordered: bool) -> egui::Response {
    let t = Tokens::get(ui.ctx());
    let galley = crate::bidi::layout_ui(ui, label.to_owned(), theme::regular(13.0), t.text);
    let rect = Rect::from_min_size(at, vec2(galley.size().x + 20.0, ICON_BUTTON));
    let resp = ui.interact(rect, id, Sense::click());
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, label));
    let press = theme::Press::track(ui, &resp);
    let (fill, stroke) =
        if bordered { (press.fill(t.chrome, t.hover, t.pressed), Stroke::new(1.0, t.border)) } else { (press.wash_color(&t, false), Stroke::NONE) };
    ui.painter().rect(rect, CornerRadius::same(6), fill, stroke, egui::StrokeKind::Inside);
    ui.painter().galley(rect.left_center() + vec2(10.0, -galley.size().y / 2.0) + press.offset(), galley, t.text);
    theme::hand(resp)
}

/// `text` wrapped to `width`, in `font` on lines `line_height` apart.
fn paragraph(ui: &egui::Ui, text: &str, font: egui::FontId, line_height: f32, color: Color32, width: f32) -> Arc<egui::Galley> {
    let format = egui::TextFormat { font_id: font, color, line_height: Some(line_height), ..Default::default() };
    let mut job = egui::text::LayoutJob::single_section(text.to_owned(), format);
    job.wrap.max_width = width;
    ui.fonts_mut(|f| f.layout_job(job))
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
    let t = Tokens::get(ctx);
    let screen = ctx.content_rect();
    let size = popup.size.unwrap_or(vec2(CARD_WIDTH, HEIGHT_GUESS));
    // Anchored, the card follows the room by the selection; dragged, or reopened where a card was
    // dragged, it stays where it is.
    let (at, pointer) = match (popup.anchor, popup.pos) {
        (Some(anchor), _) => {
            let (at, x) = place(anchor, size, screen);
            (at, Some(x))
        }
        (None, Some(pos)) => (keep_on_screen(pos, size, screen), None),
        // Nothing to point at (the selection was off screen): the middle of the window.
        (None, None) => (keep_on_screen(screen.center() - size / 2.0, size, screen), None),
    };
    let now = ctx.input(|i| i.time);
    let copied = popup.copied_at.is_some_and(|at| now - at < COPIED_FOR);
    if let Some(at) = popup.copied_at {
        if copied {
            ctx.request_repaint_after(Duration::from_secs_f64((at + COPIED_FOR - now).max(0.0)));
        } else {
            popup.copied_at = None;
        }
    }
    let id = egui::Id::unique("ai-define-popup");
    let mut choice = None;
    let mut dragged = Vec2::ZERO;
    let shown = egui::Area::new(id).order(egui::Order::Foreground).fixed_pos(at).show(ctx, |ui| {
        let frame = egui::Frame::new()
            .fill(t.card)
            .stroke(Stroke::new(1.0, t.border))
            .corner_radius(CornerRadius::same(CARD_RADIUS))
            .shadow(ui.visuals().popup_shadow);
        frame.show(ui, |ui| {
            // The frame's 1 px border is inside the card's 320.
            ui.set_width(CARD_WIDTH - 2.0);
            ui.spacing_mut().item_spacing = Vec2::ZERO;
            let width = ui.max_rect().width();
            let body_width = width - 2.0 * EDGE;
            // Header: the word and the model, the grip and close at the right. Dragging it
            // anywhere but its buttons moves the card.
            let (header, _) = ui.allocate_exact_size(vec2(width, HEADER), Sense::hover());
            let drag = ui.interact(header, id.with("drag"), Sense::drag());
            if drag.dragged() {
                dragged = drag.drag_delta();
            }
            let close =
                Rect::from_min_size(pos2(header.right() - 8.0 - ICON_BUTTON, header.center().y - ICON_BUTTON / 2.0), Vec2::splat(ICON_BUTTON));
            let grip = close.translate(vec2(-ICON_BUTTON, 0.0));
            let show_model = !popup.model.is_empty() && !matches!(popup.state, DefineState::Error(_));
            let model = show_model.then(|| crate::bidi::layout_ltr(ui, popup.model.clone(), theme::regular(12.0), t.text_faint));
            let model_room = model.as_ref().map_or(0.0, |g| 8.0 + g.size().x);
            let word_room = (grip.left() - 8.0 - (header.left() + EDGE) - model_room).max(24.0);
            let mut job = egui::text::LayoutJob::simple_singleline(popup.word.clone(), theme::semibold(15.0), t.text);
            job.wrap = egui::text::TextWrapping::truncate_at_width(word_room);
            let word = ui.fonts_mut(|f| f.layout_job(job));
            let word_at = pos2(header.left() + EDGE, header.center().y - word.size().y / 2.0);
            let word_end = word_at.x + word.size().x;
            ui.painter().galley(word_at, word, t.text);
            if let Some(model) = model {
                ui.painter().galley(pos2(word_end + 8.0, header.center().y - model.size().y / 2.0), model, t.text_faint);
            }
            let over_grip = drag.dragged() || drag.hover_pos().is_some_and(|p| grip.contains(p));
            crate::icons::paint(ui, grip, "grip-vertical", 16.0, if over_grip { t.text } else { t.icon_quiet });
            if drag.dragged() {
                ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
            } else if drag.hovered() {
                ui.ctx().set_cursor_icon(egui::CursorIcon::Grab);
            }
            if quiet_button(ui, close, id.with("close"), "x", tl!("Close"), None).clicked() {
                choice = Some(Choice::Close);
            }
            match &popup.state {
                DefineState::Loading => {
                    // Two bars shaped like the answer that is coming, pulsing between the hover
                    // and pressed colours (still while animation effects are off).
                    let pulse = if crate::motion::animated(ui.ctx()) {
                        ui.ctx().request_repaint();
                        ((now * std::f64::consts::TAU / 1.4).sin() * 0.5 + 0.5) as f32
                    } else {
                        0.0
                    };
                    let fill = t.hover.lerp_to_gamma(t.pressed, pulse);
                    let (bars, _) = ui.allocate_exact_size(vec2(width, 10.0 + 8.0 + 10.0), Sense::hover());
                    for (y, share) in [(0.0, 1.0), (18.0, 0.62)] {
                        let bar = Rect::from_min_size(pos2(bars.left() + EDGE, bars.top() + y), vec2(body_width * share, 10.0));
                        ui.painter().rect_filled(bar, CornerRadius::same(5), fill);
                    }
                    // Footer: Cancel where Copy will be, who is being asked at the right.
                    let (footer, _) = ui.allocate_exact_size(vec2(width, 12.0 + ICON_BUTTON + 8.0), Sense::hover());
                    let row = footer.top() + 12.0;
                    if text_button(ui, pos2(footer.left() + EDGE - 10.0, row), id.with("cancel"), tl!("Cancel"), false).clicked() {
                        choice = Some(Choice::Close);
                    }
                    let status = crate::i18n::fmt(tl!("Asking {provider}…"), &[("provider", &popup.provider)]);
                    let status = crate::bidi::layout_ui(ui, status, theme::regular(12.0), t.text_faint);
                    let mid = row + ICON_BUTTON / 2.0;
                    let status_at = pos2(footer.right() - EDGE - status.size().x, mid - status.size().y / 2.0);
                    ui.painter().galley(status_at, status, t.text_faint);
                    paint_spinner(ui, pos2(status_at.x - 6.0 - 6.0, mid), now, &t);
                }
                DefineState::Answer(answer) => {
                    let meaning = paragraph(ui, answer, theme::regular(14.0), 21.0, t.text, body_width);
                    let (body, resp) = ui.allocate_exact_size(vec2(width, meaning.size().y), Sense::hover());
                    resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Label, true, answer));
                    ui.painter().galley(pos2(body.left() + EDGE, body.top()), meaning, t.text);
                    // Footer: copy with its icon on the content edge, model settings under close.
                    let (footer, _) = ui.allocate_exact_size(vec2(width, 4.0 + ICON_BUTTON + 8.0), Sense::hover());
                    let copy = Rect::from_min_size(pos2(footer.left() + EDGE - 6.0, footer.top() + 4.0), Vec2::splat(ICON_BUTTON));
                    let (icon, tip, tint) = if copied { ("check", tl!("Copied"), Some(t.ok)) } else { ("copy", tl!("Copy meaning"), None) };
                    if quiet_button(ui, copy, id.with("copy"), icon, tip, tint).clicked() {
                        choice = Some(Choice::Copy(answer.clone()));
                    }
                    let gear = Rect::from_min_size(pos2(close.left(), copy.top()), Vec2::splat(ICON_BUTTON));
                    if quiet_button(ui, gear, id.with("settings"), "settings", tl!("Model settings"), None).clicked() {
                        choice = Some(Choice::Settings);
                    }
                }
                DefineState::Error(why) => {
                    // The icon alone is in the error colour, so the reason stays readable.
                    let text_left = EDGE + 16.0 + 8.0;
                    let text_width = width - text_left - EDGE;
                    let title = paragraph(ui, tl!("Couldn't look this up"), theme::semibold(14.0), 20.0, t.text, text_width);
                    let reason = paragraph(ui, why, theme::regular(13.0), 19.0, t.text_muted, text_width);
                    let title_height = title.size().y;
                    let (body, resp) = ui.allocate_exact_size(vec2(width, title_height + 2.0 + reason.size().y), Sense::hover());
                    let title_text = tl!("Couldn't look this up");
                    resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Label, true, format!("{title_text}. {why}")));
                    let icon = Rect::from_min_size(pos2(body.left() + EDGE, body.top() + 2.0), Vec2::splat(16.0));
                    crate::icons::paint(ui, icon, "circle-alert", 16.0, t.danger);
                    ui.painter().galley(pos2(body.left() + text_left, body.top()), title, t.text);
                    ui.painter().galley(pos2(body.left() + text_left, body.top() + title_height + 2.0), reason, t.text_muted);
                    let (footer, _) = ui.allocate_exact_size(vec2(width, 12.0 + ICON_BUTTON + 12.0), Sense::hover());
                    let at = pos2(footer.left() + EDGE, footer.top() + 12.0);
                    if text_button(ui, at, id.with("open-settings"), tl!("Open AI settings"), true).clicked() {
                        choice = Some(Choice::Settings);
                    }
                }
            }
        });
    });
    let rect = shown.response.rect;
    if popup.size != Some(rect.size()) {
        popup.size = Some(rect.size());
        ctx.request_repaint();
    }
    // The pointer, over the card's border, towards the selection.
    if let (Some(x), Some(anchor)) = (pointer, popup.anchor) {
        let painter = ctx.layer_painter(shown.response.layer_id);
        let half = POINTER / 2.0;
        let (edge, tip) = if rect.top() > anchor.top() { (rect.top() + 0.5, rect.top() - half) } else { (rect.bottom() - 0.5, rect.bottom() + half) };
        let (a, b, c) = (pos2(x - half, edge), pos2(x, tip), pos2(x + half, edge));
        painter.add(egui::Shape::convex_polygon(vec![a, b, c], t.card, Stroke::NONE));
        painter.line_segment([a, b], Stroke::new(1.0, t.border));
        painter.line_segment([b, c], Stroke::new(1.0, t.border));
    }
    // A card placed by the user keeps the place asked for: it is only drawn kept on screen, as
    // its height changes from the lookup's to the answer's.
    if popup.anchor.is_some() || popup.pos.is_none() {
        popup.pos = Some(rect.min);
    }
    if dragged != Vec2::ZERO {
        // Moved by the user: it stays where they put it, and later lookups open there.
        let to = keep_on_screen(rect.min + dragged, rect.size(), screen);
        popup.pos = Some(to);
        popup.anchor = None;
        app.define_pos = Some(to);
    }
    // Escape closes it, unless a text field has the keyboard.
    let escape = !ctx.egui_wants_keyboard_input() && ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape));
    match choice {
        Some(Choice::Copy(text)) => {
            ctx.copy_text(text);
            popup.copied_at = Some(now);
            ctx.request_repaint_after(Duration::from_secs_f64(COPIED_FOR));
        }
        Some(Choice::Close) => app.define_popup = None,
        Some(Choice::Settings) => {
            app.define_popup = None;
            app.open_ai_preferences();
        }
        None => {}
    }
    if escape {
        app.define_popup = None;
    }
}

/// A 12 px spinner centred on `center`: a divider-coloured ring with an accent arc that turns
/// (still while animation effects are off).
fn paint_spinner(ui: &egui::Ui, center: Pos2, now: f64, t: &Tokens) {
    let radius = 5.25;
    let painter = ui.painter();
    painter.circle_stroke(center, radius, Stroke::new(1.5, t.divider));
    let start = if crate::motion::animated(ui.ctx()) { (now * std::f64::consts::TAU / 0.8) as f32 } else { 0.0 };
    let points: Vec<Pos2> = (0..=12)
        .map(|i| {
            let a = start + i as f32 / 12.0 * std::f32::consts::FRAC_PI_2;
            center + vec2(a.cos(), a.sin()) * radius
        })
        .collect();
    painter.add(egui::Shape::line(points, Stroke::new(1.5, t.accent)));
}

#[cfg(test)]
mod tests {
    use super::*;

    const SCREEN: Rect = Rect { min: Pos2::ZERO, max: Pos2 { x: 1200.0, y: 800.0 } };

    #[test]
    fn the_card_opens_12_below_the_selection_with_its_pointer_over_it() {
        let word = Rect::from_min_size(pos2(400.0, 200.0), vec2(60.0, 18.0));
        let (at, pointer) = place(word, vec2(CARD_WIDTH, 140.0), SCREEN);
        assert_eq!(at.y, word.bottom() + 12.0);
        assert!(word.x_range().contains(pointer), "the pointer is over the word: {pointer}");
        assert!(at.x < word.left(), "the card starts left of the word");
    }

    #[test]
    fn the_card_flips_above_when_less_than_160_is_left_below() {
        let size = vec2(CARD_WIDTH, 140.0);
        let word = |bottom: f32| Rect::from_min_max(pos2(400.0, bottom - 18.0), pos2(460.0, bottom));
        let (at, _) = place(word(SCREEN.bottom() - 160.0), size, SCREEN);
        assert!(at.y > SCREEN.bottom() - 160.0, "160 left: still below");
        let low = word(SCREEN.bottom() - 159.0);
        let (at, _) = place(low, size, SCREEN);
        assert_eq!(at.y + size.y, low.top() - 12.0, "159 left: above, 12 from the selection");
    }

    #[test]
    fn the_card_stays_on_screen_at_the_edges() {
        let size = vec2(CARD_WIDTH, 140.0);
        let (at, pointer) = place(Rect::from_min_size(pos2(1180.0, 100.0), vec2(15.0, 18.0)), size, SCREEN);
        assert!(at.x + size.x <= SCREEN.right(), "kept in on the right");
        assert!(pointer <= at.x + size.x - f32::from(CARD_RADIUS), "the pointer clears the corner");
        let (at, _) = place(Rect::from_min_size(pos2(2.0, 100.0), vec2(15.0, 18.0)), size, SCREEN);
        assert!(at.x >= SCREEN.left());
    }

    #[test]
    fn the_header_names_the_model_not_the_provider() {
        assert_eq!(model_label(&ProviderConfig::default()), "haiku");
        let empty = ProviderConfig { model: String::new(), ..ProviderConfig::default() };
        assert_eq!(model_label(&empty), "haiku", "Claude's own default");
        let codex = ProviderConfig { kind: ProviderKind::Codex, model: String::new(), ..ProviderConfig::default() };
        assert_eq!(model_label(&codex), "", "Codex chooses its own");
        let named = ProviderConfig { kind: ProviderKind::Codex, model: " gpt-5 ".into(), ..ProviderConfig::default() };
        assert_eq!(model_label(&named), "gpt-5");
        let custom = ProviderConfig { kind: ProviderKind::Custom, custom: "ollama run x".into(), ..ProviderConfig::default() };
        assert_eq!(model_label(&custom), "", "a command has no model setting");
    }

    #[test]
    fn the_status_names_who_is_asked() {
        assert_eq!(provider_label(&ProviderConfig::default()), "Claude");
        let custom = |command: &str| ProviderConfig { kind: ProviderKind::Custom, custom: command.into(), ..ProviderConfig::default() };
        assert_eq!(provider_label(&custom("ollama run llama3.2")), "ollama");
        assert_eq!(provider_label(&custom(r#""C:\tools\llm.exe" -q"#)), "llm");
        assert_eq!(provider_label(&custom("")), "your command");
    }
}
