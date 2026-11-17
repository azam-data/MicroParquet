//! Design tokens (light and dark), sizes, fonts, icons and the shared widgets. The only place in
//! the UI code that holds colour or font-size literals.
#![allow(dead_code)] // some tokens and helpers are used by later screens

use eframe::egui::{self, Color32, FontFamily, FontId, Response, RichText, Rounding, Stroke};

const fn hex(rgb: u32) -> Color32 {
    Color32::from_rgb(((rgb >> 16) & 0xFF) as u8, ((rgb >> 8) & 0xFF) as u8, (rgb & 0xFF) as u8)
}

/// One complete colour set. Every field exists in both `LIGHT` and `DARK`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Tokens {
    pub bg: Color32,
    pub surface: Color32,
    pub border: Color32,
    pub grid: Color32,
    pub zebra: Color32,
    /// Background of a table's heading row (the data grid, the schema lists).
    pub header_bg: Color32,
    pub ink: Color32,
    pub ink2: Color32,
    pub muted: Color32,
    pub accent: Color32,
    pub accent_dark: Color32,
    pub accent_soft: Color32,
    pub accent_soft_border: Color32,
    pub summary_bg: Color32,
    pub summary_fg: Color32,
    pub summary_divider: Color32,
    pub selector_on_bg: Color32,
    pub selector_on_border: Color32,
    pub selector_on_fg: Color32,
    pub selector_off_fg: Color32,
    pub success: Color32,
    pub success_soft: Color32,
    pub warning: Color32,
    pub danger: Color32,
}

pub const LIGHT: Tokens = Tokens {
    bg: hex(0xF6F7F9),
    surface: hex(0xFFFFFF),
    border: hex(0xE3E6EB),
    grid: hex(0xF0F2F5),
    zebra: hex(0xFAFBFC),
    header_bg: hex(0xE9EDF3),
    ink: hex(0x1B2430),
    ink2: hex(0x3D4856),
    muted: hex(0x5B6675),
    accent: hex(0x1F5FAD),
    accent_dark: hex(0x174A87),
    accent_soft: hex(0xEAF1FB),
    accent_soft_border: hex(0xC9DBF3),
    summary_bg: hex(micro_parquet::table_layout::SUMMARY_BG),
    summary_fg: hex(micro_parquet::table_layout::SUMMARY_FG),
    summary_divider: hex(0x2E4C75),
    selector_on_bg: hex(0xFFFFFF),
    selector_on_border: hex(0xB9C7DA),
    selector_on_fg: hex(0x1F3A5F),
    selector_off_fg: hex(0x8792A0),
    success: hex(0x1E7A4C),
    success_soft: hex(0xE6F4EC),
    warning: hex(0xB26A00),
    danger: hex(0xB42318),
};

/// The spec lists dark values for most tokens. The ones it leaves out (accent_soft_border, the
/// selector colours, warning) are derived from the same palette; see `<accepted_deviations>`.
pub const DARK: Tokens = Tokens {
    bg: hex(0x0F1419),
    surface: hex(0x161C23),
    border: hex(0x2A3440),
    grid: hex(0x1E2630),
    zebra: hex(0x131920),
    header_bg: hex(0x1D252E),
    ink: hex(0xE6EAF0),
    ink2: hex(0xB7C2CF),
    muted: hex(0x93A0AF),
    accent: hex(0xE8A33D),
    accent_dark: hex(0xF2C27A),
    accent_soft: hex(0x2A2214),
    accent_soft_border: hex(0x4A3A1C),
    summary_bg: hex(0x2A2214),
    summary_fg: hex(0xF2C27A),
    summary_divider: hex(0x3A2F1C),
    selector_on_bg: hex(0x161C23),
    selector_on_border: hex(0x3A4756),
    selector_on_fg: hex(0xF2C27A),
    selector_off_fg: hex(0x7B8794),
    success: hex(0x6BD39A),
    success_soft: hex(0x15301F),
    warning: hex(0xF0A93A),
    danger: hex(0xF97066),
};

/// The token set for the current theme.
pub fn tokens(dark: bool) -> &'static Tokens {
    if dark {
        &DARK
    } else {
        &LIGHT
    }
}

// ---- Sizes (logical px at 100% UI scale)
pub const TEXT_BASE: f32 = 14.0;
pub const TEXT_SMALL: f32 = 13.0;
pub const TEXT_LABEL: f32 = 12.0;
pub const TEXT_INSPECTOR_TITLE: f32 = 20.0;
/// The single top bar (app mark, menus, toolbar groups, search); replaces the 36 px title strip
/// and the 52 px toolbar.
pub const TOP_BAR_H: f32 = 44.0;
pub const TAB_STRIP_H: f32 = 40.0;
pub const STATUS_BAR_H: f32 = 32.0;
pub const RIGHT_PANEL_W: f32 = 316.0;
pub const HEADER_ROW_H: f32 = 38.0;
pub const SELECTOR_ROW_H: f32 = 30.0;
pub const SUMMARY_BAND_H: f32 = 38.0;
pub const DATA_ROW_H: f32 = 32.0;
pub const RADIUS_CONTROL: f32 = 6.0;
pub const RADIUS_PANEL: f32 = 8.0;
pub const SPACE_XS: f32 = 4.0;
pub const SPACE_S: f32 = 8.0;
pub const SPACE_M: f32 = 12.0;
pub const SPACE_L: f32 = 16.0;
pub const COLUMN_MAX_W: f32 = 420.0;
/// Heading text style (modal titles); the inspector title is `TEXT_INSPECTOR_TITLE`.
pub const TEXT_HEADING: f32 = 17.0;
pub const TEXT_MONO: f32 = 13.0;
/// The progress bar in the status bar.
pub const PROGRESS_BAR_W: f32 = 120.0;
/// The schema dialog: width, the preview's fixed 5 rows, and the columns list.
pub const SCHEMA_DIALOG_W: f32 = 1040.0;
/// Space between the items of the two option rows.
pub const SCHEMA_ROW_GAP: f32 = 6.0;
pub const PREVIEW_ROWS: usize = 5;
/// Header row, 5 data rows (each followed by egui's 6 px row spacing) and the frame around them.
pub const PREVIEW_GRID_H: f32 = HEADER_ROW_H + PREVIEW_ROWS as f32 * (DATA_ROW_H + 6.0) + 8.0;
pub const SCHEMA_COMBO_W: f32 = 100.0;
pub const SCHEMA_COL_NAME_W: f32 = 220.0;
pub const SCHEMA_COL_TYPE_W: f32 = 150.0;
pub const SCHEMA_COL_PII_W: f32 = 150.0;
pub const SCHEMA_COL_SIGN_W: f32 = 150.0;
pub const SCHEMA_COL_FAIL_W: f32 = 110.0;
pub const SCHEMA_COL_MIN_W: f32 = 80.0;
/// Top bar pieces.
pub const APP_MARK_SIZE: f32 = 18.0;
pub const APP_MARK_RADIUS: f32 = 4.0;
pub const TEXT_MARK: f32 = 11.0;
pub const TEXT_KEYCAP: f32 = 11.0;
pub const BAR_ITEM_GAP: f32 = 2.0;
/// Side padding of the buttons in the top bar (the default is 12), so more groups fit.
pub const BAR_BUTTON_PAD_X: f32 = 8.0;
pub const BAR_SEPARATOR_W: f32 = 17.0;
pub const BAR_SEPARATOR_H: f32 = 20.0;
/// Added to a measured button width, so a rounding difference never makes two items touch.
pub const BAR_WIDTH_SLACK: f32 = 2.0;
/// The command search box: preferred, smallest, and the icon-only width.
pub const SEARCH_MAX_W: f32 = 310.0;
pub const SEARCH_MIN_W: f32 = 170.0;
pub const SEARCH_ICON_W: f32 = 44.0;
/// The box's own padding and border (10 + 10 + 1 + 1).
pub const SEARCH_FRAME_PAD: f32 = 22.0;
/// From this width the box says "Search actions and columns"; narrower it says "Search…".
pub const SEARCH_LONG_HINT_W: f32 = 260.0;


impl Tokens {
    /// Every token with its name, in one fixed order (used by the light/dark completeness test).
    pub fn named(&self) -> [(&'static str, Color32); 24] {
        [
            ("bg", self.bg),
            ("surface", self.surface),
            ("border", self.border),
            ("grid", self.grid),
            ("zebra", self.zebra),
            ("header_bg", self.header_bg),
            ("ink", self.ink),
            ("ink2", self.ink2),
            ("muted", self.muted),
            ("accent", self.accent),
            ("accent_dark", self.accent_dark),
            ("accent_soft", self.accent_soft),
            ("accent_soft_border", self.accent_soft_border),
            ("summary_bg", self.summary_bg),
            ("summary_fg", self.summary_fg),
            ("summary_divider", self.summary_divider),
            ("selector_on_bg", self.selector_on_bg),
            ("selector_on_border", self.selector_on_border),
            ("selector_on_fg", self.selector_on_fg),
            ("selector_off_fg", self.selector_off_fg),
            ("success", self.success),
            ("success_soft", self.success_soft),
            ("warning", self.warning),
            ("danger", self.danger),
        ]
    }
}

// ------------------------------------------------------------------------------- colours in use

fn t(ui: &egui::Ui) -> &'static Tokens {
    tokens(ui.visuals().dark_mode)
}
pub fn danger(ui: &egui::Ui) -> Color32 {
    t(ui).danger
}
pub fn warning(ui: &egui::Ui) -> Color32 {
    t(ui).warning
}
pub fn success(ui: &egui::Ui) -> Color32 {
    t(ui).success
}
pub fn accent(ui: &egui::Ui) -> Color32 {
    t(ui).accent
}
pub fn muted(ui: &egui::Ui) -> Color32 {
    t(ui).muted
}

// ------------------------------------------------------------------------------------- icons

/// Phosphor icon names used by the UI (the glyph strings).
pub use egui_phosphor::regular as ph;

/// Symbols that lower layers put in messages, shown as icon-font glyphs so none renders as a box.
pub fn icons_in_text(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        match ch {
            '\u{26A0}' => out.push_str(ph::WARNING),
            '\u{2705}' => out.push_str(ph::CHECK_CIRCLE),
            '\u{274C}' => out.push_str(ph::X_CIRCLE),
            '\u{1F511}' => out.push_str(ph::KEY),
            '\u{FE0F}' => {}
            c => out.push(c),
        }
    }
    out
}

// ------------------------------------------------------------------------------------- fonts

pub fn family_semibold() -> FontFamily {
    FontFamily::Name("sans-semibold".into())
}
pub fn family_bold() -> FontFamily {
    FontFamily::Name("sans-bold".into())
}
pub fn family_mono_semibold() -> FontFamily {
    FontFamily::Name("mono-semibold".into())
}

/// Text in Source Sans 3 SemiBold.
pub fn semibold(text: impl Into<String>) -> RichText {
    RichText::new(text).family(family_semibold())
}
pub fn bold(text: impl Into<String>) -> RichText {
    RichText::new(text).family(family_bold())
}
/// Numbers, IDs, dates and totals: JetBrains Mono.
pub fn mono(text: impl Into<String>) -> RichText {
    RichText::new(text).family(FontFamily::Monospace).size(TEXT_MONO)
}
pub fn mono_semibold(text: impl Into<String>) -> RichText {
    RichText::new(text).family(family_mono_semibold()).size(TEXT_MONO)
}
/// 12 px label text (column labels, captions).
pub fn label_text(text: impl Into<String>) -> RichText {
    RichText::new(text).size(TEXT_LABEL)
}

/// The fonts the program draws with, all compiled into it (no font file is read at run time):
/// Source Sans 3 Regular / SemiBold / Bold for text, JetBrains Mono Regular / SemiBold for
/// numbers, and Phosphor for icons. Licences: `assets/fonts/OFL-*` and `THIRD_PARTY_NOTICES.md`.
pub fn font_definitions() -> egui::FontDefinitions {
    use egui::FontData;
    let mut fonts = egui::FontDefinitions::default();
    let proportional_defaults = fonts.families.get(&FontFamily::Proportional).cloned().unwrap_or_default();
    let mono_defaults = fonts.families.get(&FontFamily::Monospace).cloned().unwrap_or_default();

    let embedded: [(&str, &'static [u8]); 5] = [
        ("sans-regular", include_bytes!("../../assets/fonts/SourceSans3-Regular.ttf")),
        ("sans-semibold-font", include_bytes!("../../assets/fonts/SourceSans3-Semibold.ttf")),
        ("sans-bold-font", include_bytes!("../../assets/fonts/SourceSans3-Bold.ttf")),
        ("jbm-regular", include_bytes!("../../assets/fonts/JetBrainsMono-Regular.ttf")),
        ("jbm-semibold", include_bytes!("../../assets/fonts/JetBrainsMono-SemiBold.ttf")),
    ];
    for (key, bytes) in embedded {
        fonts.font_data.insert(key.into(), FontData::from_static(bytes));
    }

    // Proportional: Source Sans 3 first, then egui's own fallbacks (symbols, emoji).
    let mut prop = vec!["sans-regular".to_string()];
    prop.extend(proportional_defaults.iter().cloned());
    fonts.families.insert(FontFamily::Proportional, prop.clone());
    // Monospace: JetBrains Mono first.
    let mut mono_list = vec!["jbm-regular".to_string()];
    mono_list.extend(mono_defaults.iter().cloned());
    fonts.families.insert(FontFamily::Monospace, mono_list.clone());

    // The weights are families of their own (egui has no font-weight): the weight first, then the
    // regular fonts as fallback.
    let with_first = |first: &str, base: &[String]| -> Vec<String> {
        let mut v = vec![first.to_string()];
        v.extend(base.iter().cloned());
        v
    };
    fonts.families.insert(family_semibold(), with_first("sans-semibold-font", &prop));
    fonts.families.insert(family_bold(), with_first("sans-bold-font", &prop));
    fonts.families.insert(family_mono_semibold(), with_first("jbm-semibold", &mono_list));

    // Icons: Phosphor, as a fallback behind the first font of every family.
    egui_phosphor::add_to_fonts(&mut fonts, egui_phosphor::Variant::Regular);
    for fam in [FontFamily::Monospace, family_semibold(), family_bold(), family_mono_semibold()] {
        if let Some(list) = fonts.families.get_mut(&fam) {
            let at = 1.min(list.len());
            list.insert(at, "phosphor".into());
        }
    }
    fonts
}

/// Installs the program's fonts.
pub fn load_fonts(ctx: &egui::Context) {
    ctx.set_fonts(font_definitions());
}

// -------------------------------------------------------------------------------------- style

pub fn apply_ui_theme(ctx: &egui::Context, dark_mode: bool, zoom: f32) {
    ctx.set_zoom_factor(zoom);
    let tk = tokens(dark_mode);

    let mut style = (*ctx.style()).clone();
    style.text_styles.insert(egui::TextStyle::Body, FontId::new(TEXT_BASE, FontFamily::Proportional));
    style.text_styles.insert(egui::TextStyle::Button, FontId::new(TEXT_BASE, FontFamily::Proportional));
    style.text_styles.insert(egui::TextStyle::Small, FontId::new(TEXT_SMALL, FontFamily::Proportional));
    style.text_styles.insert(egui::TextStyle::Monospace, FontId::new(TEXT_MONO, FontFamily::Monospace));
    style.text_styles.insert(egui::TextStyle::Heading, FontId::new(TEXT_HEADING, family_semibold()));
    style.spacing.item_spacing = egui::vec2(SPACE_S, SPACE_S - 2.0);
    style.spacing.button_padding = egui::vec2(SPACE_M, 5.0);
    style.spacing.interact_size = egui::vec2(36.0, 30.0);
    style.spacing.menu_margin = egui::Margin::same(SPACE_S);
    style.spacing.window_margin = egui::Margin::same(SPACE_M);

    let ctl = Rounding::same(RADIUS_CONTROL);
    let mut v = if dark_mode { egui::Visuals::dark() } else { egui::Visuals::light() };
    v.override_text_color = None;
    v.panel_fill = tk.bg;
    v.window_fill = tk.surface;
    v.window_stroke = Stroke::new(1.0_f32, tk.border);
    v.window_rounding = Rounding::same(RADIUS_PANEL);
    v.menu_rounding = ctl;
    v.faint_bg_color = tk.zebra;
    v.extreme_bg_color = tk.surface;
    v.hyperlink_color = tk.accent;
    v.warn_fg_color = tk.warning;
    v.error_fg_color = tk.danger;
    v.selection.bg_fill = tk.accent_soft;
    v.selection.stroke = Stroke::new(1.0_f32, tk.accent_dark);
    v.striped = true;

    v.widgets.noninteractive.bg_fill = tk.surface;
    v.widgets.noninteractive.weak_bg_fill = tk.surface;
    v.widgets.noninteractive.bg_stroke = Stroke::new(1.0_f32, tk.border);
    v.widgets.noninteractive.fg_stroke = Stroke::new(1.0_f32, tk.ink);
    v.widgets.noninteractive.rounding = ctl;

    v.widgets.inactive.bg_fill = tk.surface;
    v.widgets.inactive.weak_bg_fill = tk.surface;
    v.widgets.inactive.bg_stroke = Stroke::new(1.0_f32, tk.border);
    v.widgets.inactive.fg_stroke = Stroke::new(1.0_f32, tk.ink);
    v.widgets.inactive.rounding = ctl;

    v.widgets.hovered.bg_fill = tk.accent_soft;
    v.widgets.hovered.weak_bg_fill = tk.accent_soft;
    v.widgets.hovered.bg_stroke = Stroke::new(1.0_f32, tk.accent_soft_border);
    v.widgets.hovered.fg_stroke = Stroke::new(1.0_f32, tk.ink);
    v.widgets.hovered.rounding = ctl;

    v.widgets.active.bg_fill = tk.accent_soft_border;
    v.widgets.active.weak_bg_fill = tk.accent_soft_border;
    v.widgets.active.bg_stroke = Stroke::new(1.0_f32, tk.accent);
    v.widgets.active.fg_stroke = Stroke::new(1.0_f32, tk.accent_dark);
    v.widgets.active.rounding = ctl;

    v.widgets.open.bg_fill = tk.accent_soft;
    v.widgets.open.weak_bg_fill = tk.accent_soft;
    v.widgets.open.bg_stroke = Stroke::new(1.0_f32, tk.accent_soft_border);
    v.widgets.open.fg_stroke = Stroke::new(1.0_f32, tk.ink);
    v.widgets.open.rounding = ctl;

    style.visuals = v;
    ctx.set_style(style);
}

// ----------------------------------------------------------------------------------- widgets

/// Text colour on an accent-filled control: white in the light theme, the dark window colour on
/// the dark theme's amber.
pub fn on_accent(dark_mode: bool) -> Color32 {
    if dark_mode {
        DARK.bg
    } else {
        Color32::WHITE
    }
}

/// A 1 px line along the bottom edge of the panel being drawn.
pub fn bottom_border(ui: &egui::Ui, color: Color32) {
    let r = ui.max_rect();
    ui.painter().hline(r.x_range(), r.bottom() - 0.5, Stroke::new(1.0_f32, color));
}

/// A 1 px line along the top edge of the panel being drawn.
pub fn top_border(ui: &egui::Ui, color: Color32) {
    let r = ui.max_rect();
    ui.painter().hline(r.x_range(), r.top() + 0.5, Stroke::new(1.0_f32, color));
}

/// A 1 px line along the left edge of the panel being drawn.
pub fn left_border(ui: &egui::Ui, color: Color32) {
    let r = ui.max_rect();
    ui.painter().vline(r.left() + 0.5, r.y_range(), Stroke::new(1.0_f32, color));
}

/// Primary action: accent fill, white text (dark theme: the dark window colour on the amber).
pub fn primary_button(ui: &mut egui::Ui, text: impl Into<String>) -> Response {
    let tk = t(ui);
    let fg = on_accent(ui.visuals().dark_mode);
    ui.add(
        egui::Button::new(RichText::new(text).color(fg).family(family_semibold()))
            .fill(tk.accent)
            .stroke(Stroke::NONE)
            .rounding(RADIUS_CONTROL),
    )
}

/// Destructive action: danger colour text and outline.
pub fn danger_button(ui: &mut egui::Ui, text: impl Into<String>) -> Response {
    let tk = t(ui);
    ui.add(
        egui::Button::new(RichText::new(text).color(tk.danger).family(family_semibold()))
            .stroke(Stroke::new(1.0_f32, tk.danger))
            .rounding(RADIUS_CONTROL),
    )
}

/// Flat toolbar / panel button with an icon and a label.
pub fn flat_button(ui: &mut egui::Ui, icon: &str, label: &str) -> Response {
    let tk = t(ui);
    let text = if label.is_empty() { icon.to_string() } else { format!("{}  {}", icon, label) };
    ui.add(
        egui::Button::new(RichText::new(text).color(tk.ink))
            .fill(Color32::TRANSPARENT)
            .stroke(Stroke::NONE)
            .rounding(RADIUS_CONTROL)
            .min_size(egui::vec2(0.0, 34.0)),
    )
}

/// Icon-only button with a hover label.
pub fn icon_button(ui: &mut egui::Ui, icon: &str, hover: &str) -> Response {
    let tk = t(ui);
    ui.add(
        egui::Button::new(RichText::new(icon).color(tk.ink2))
            .fill(Color32::TRANSPARENT)
            .stroke(Stroke::NONE)
            .rounding(RADIUS_CONTROL)
            .min_size(egui::vec2(28.0, 28.0)),
    )
    .on_hover_text(hover)
}

/// A card: a bordered panel with a heading, as tall as its content.
pub fn card(ui: &mut egui::Ui, heading: &str, add: impl FnOnce(&mut egui::Ui)) -> egui::Rect {
    card_with(ui, heading, |_| {}, add)
}

/// A card whose heading row also carries something at its right edge.
pub fn card_with(
    ui: &mut egui::Ui,
    heading: &str,
    right: impl FnOnce(&mut egui::Ui),
    add: impl FnOnce(&mut egui::Ui),
) -> egui::Rect {
    let tk = *t(ui);
    egui::Frame::none()
        .fill(tk.surface)
        .stroke(Stroke::new(1.0_f32, tk.border))
        .rounding(Rounding::same(RADIUS_PANEL))
        .inner_margin(egui::Margin::same(SPACE_M))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.label(semibold(heading).color(tk.ink));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), right);
            });
            add(ui);
        })
        .response
        .rect
}

/// A card that takes all the height that is left in `ui`.
pub fn card_fill(ui: &mut egui::Ui, heading: &str, add: impl FnOnce(&mut egui::Ui)) -> egui::Rect {
    let tk = *t(ui);
    let h = ui.available_height();
    egui::Frame::none()
        .fill(tk.surface)
        .stroke(Stroke::new(1.0_f32, tk.border))
        .rounding(Rounding::same(RADIUS_PANEL))
        .inner_margin(egui::Margin::same(SPACE_M))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.set_min_height((h - 2.0 * SPACE_M - 2.0).max(0.0));
            ui.set_max_height((h - 2.0 * SPACE_M - 2.0).max(0.0));
            ui.label(semibold(heading).color(tk.ink));
            ui.add_space(SPACE_XS);
            add(ui);
        })
        .response
        .rect
}

/// A muted dot between the items of a row of options.
pub fn dot(ui: &mut egui::Ui) {
    ui.label(RichText::new("\u{b7}").color(t(ui).muted));
}

/// An "i" icon whose hover text explains something at length.
pub fn info_icon(ui: &mut egui::Ui, text: &str) {
    ui.label(RichText::new(ph::INFO).color(t(ui).muted)).on_hover_text(text);
}

/// Fills a heading cell's area with `header_bg` (a little wider than the cell, so the gap
/// between two cells is filled too).
pub fn paint_header_bg(ui: &mut egui::Ui, rect: egui::Rect, color: Color32) {
    let gap = ui.spacing().item_spacing.x / 2.0;
    let wide = rect.expand2(egui::vec2(gap, 0.0));
    // a cell clips to itself; the fill goes into the gap, so widen the clip for this cell
    let clip = ui.clip_rect().union(wide);
    ui.set_clip_rect(clip);
    ui.painter().rect_filled(wide, 0.0, color);
}

/// The text of a heading cell in a list: bold ink on `header_bg`.
pub fn header_cell(ui: &mut egui::Ui, text: &str) {
    let tk = *t(ui);
    paint_header_bg(ui, ui.max_rect(), tk.header_bg);
    ui.add(egui::Label::new(bold(text).color(tk.ink)).truncate());
}

/// A removable chip (filter conditions). Returns (clicked, removed).
pub fn chip(ui: &mut egui::Ui, text: &str) -> (bool, bool) {
    let tk = t(ui);
    let mut clicked = false;
    let mut removed = false;
    egui::Frame::none()
        .fill(tk.accent_soft)
        .stroke(Stroke::new(1.0_f32, tk.accent_soft_border))
        .rounding(Rounding::same(14.0))
        .inner_margin(egui::Margin { left: 12.0, right: 4.0, top: 3.0, bottom: 3.0 })
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = SPACE_S;
                let r = ui.add(
                    egui::Label::new(RichText::new(text).color(tk.accent_dark)).sense(egui::Sense::click()),
                );
                clicked = r.clicked();
                let x = ui.add(
                    egui::Button::new(RichText::new(ph::X).size(11.0).color(tk.accent_dark))
                        .fill(Color32::TRANSPARENT)
                        .stroke(Stroke::NONE)
                        .rounding(Rounding::same(10.0))
                        .min_size(egui::vec2(20.0, 20.0)),
                );
                removed = x.on_hover_text("Remove").clicked();
            });
        });
    (clicked, removed)
}

/// Space kept free of any dialog: a dialog is at most the window height minus this.
pub const DIALOG_WINDOW_MARGIN: f32 = 48.0;
/// Room kept for the fixed footer (separator and one row of buttons).
pub const DIALOG_FOOTER_H: f32 = 56.0;
/// A dialog never gets a scrolling middle shorter than this, however small the window.
pub const DIALOG_BODY_MIN_H: f32 = 80.0;

/// The heights of a dialog in a window of a given height.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DialogLayout {
    /// The tallest the whole dialog (title bar, middle, footer, frame) may be.
    pub max_total_h: f32,
    /// The tallest the content under the title bar may be (middle + footer).
    pub max_content_h: f32,
    /// The tallest the scrolling middle may be.
    pub body_max_h: f32,
}

/// Pure layout of a dialog: `window_h` is the window height in points, `title_h` the height of
/// the title text. The dialog's frame has the window margin above and below its content, and
/// egui puts one more margin pair between the title bar and the content.
pub fn dialog_layout(window_h: f32, title_h: f32, item_spacing_y: f32) -> DialogLayout {
    let overhead = 4.0 * SPACE_M + title_h;
    let max_total_h = (window_h - DIALOG_WINDOW_MARGIN).max(overhead + DIALOG_FOOTER_H + DIALOG_BODY_MIN_H);
    let max_content_h = max_total_h - overhead;
    let body_max_h = (max_content_h - DIALOG_FOOTER_H - item_spacing_y).max(DIALOG_BODY_MIN_H);
    DialogLayout { max_total_h, max_content_h, body_max_h }
}

/// A dialog without a set width is about this wide.
pub const DIALOG_DEFAULT_W: f32 = 520.0;

/// Where a dialog first opens: horizontally centred, `DIALOG_WINDOW_MARGIN / 2` below the top, so
/// a dialog of the largest height has the same space above and below. (An egui pivot is not
/// used: during the window's first, invisible sizing frame it moves the window away.)
pub fn dialog_start_pos(screen: egui::Rect, width: f32) -> egui::Pos2 {
    egui::pos2(
        (screen.left() + (screen.width() - width) / 2.0).max(screen.left() + DIALOG_WINDOW_MARGIN / 2.0),
        screen.top() + DIALOG_WINDOW_MARGIN / 2.0,
    )
}

/// A dialog: the title bar (header) and the button footer stay in place, the middle scrolls.
/// Same title style everywhere; at most the window height minus 48 px, centred at first.
pub struct Dialog<'a> {
    window: egui::Window<'a>,
    width: f32,
}

/// The modal window every dialog uses.
pub fn modal<'a>(title: &str) -> Dialog<'a> {
    Dialog {
        window: egui::Window::new(RichText::new(title).family(family_semibold())).collapsible(false),
        width: DIALOG_DEFAULT_W,
    }
}

impl<'a> Dialog<'a> {
    pub fn collapsible(mut self, v: bool) -> Self {
        self.window = self.window.collapsible(v);
        self
    }
    pub fn default_width(mut self, w: f32) -> Self {
        self.window = self.window.default_width(w);
        self.width = w;
        self
    }
    pub fn resizable(mut self, v: bool) -> Self {
        self.window = self.window.resizable(v);
        self
    }
    pub fn open(mut self, open: &'a mut bool) -> Self {
        self.window = self.window.open(open);
        self
    }

    /// Shows the dialog. `add` calls `body` once for the scrolling middle and `footer` once for
    /// the buttons.
    pub fn show<R>(
        self,
        ctx: &egui::Context,
        add: impl FnOnce(&mut DialogUi<'_>) -> R,
    ) -> Option<egui::InnerResponse<Option<R>>> {
        let screen = ctx.screen_rect();
        // (ctx.fonts holds the context's lock: read the style before, not inside)
        let spacing_y = ctx.style().spacing.item_spacing.y;
        let title_h = ctx.fonts(|f| {
            f.row_height(&FontId::new(TEXT_HEADING, family_semibold()))
                .max(f.row_height(&FontId::new(TEXT_BASE, family_semibold())))
        });
        let lay = dialog_layout(screen.height(), title_h, spacing_y);
        self.window
            .max_height(lay.max_content_h)
            .default_pos(dialog_start_pos(screen, self.width))
            .show(ctx, |ui| add(&mut DialogUi { ui, body_max_h: lay.body_max_h }))
    }
}

/// The inside of a dialog under construction.
pub struct DialogUi<'a> {
    pub ui: &'a mut egui::Ui,
    body_max_h: f32,
}

impl DialogUi<'_> {
    /// The scrolling middle: as tall as its content, up to what the window leaves.
    pub fn body<R>(&mut self, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
        egui::ScrollArea::vertical()
            .max_height(self.body_max_h)
            .auto_shrink([true, true])
            .show(self.ui, add)
            .inner
    }

    /// A middle that does not scroll as a whole: it is as tall as the dialog may be, and the
    /// content shares that height itself (the schema dialog's lists scroll on their own).
    pub fn body_fill<R>(&mut self, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
        let w = self.ui.available_width();
        self.ui.allocate_ui(egui::vec2(w, self.body_max_h), add).inner
    }

    /// The fixed footer: a separator, then the buttons.
    pub fn footer<R>(&mut self, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
        self.ui.separator();
        add(self.ui)
    }

    /// Content between the title bar and the footer that must stay in view (a banner).
    pub fn header<R>(&mut self, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
        add(self.ui)
    }
}

/// Error line shown in modals and panels (danger colour, icon).
pub fn error_label(ui: &mut egui::Ui, msg: &str) {
    ui.colored_label(danger(ui), format!("{}  {}", ph::X_CIRCLE, msg));
}

/// Warning line (warning colour, icon).
pub fn warning_label(ui: &mut egui::Ui, msg: &str) {
    ui.colored_label(warning(ui), format!("{}  {}", ph::WARNING, msg));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_light_token_has_a_dark_counterpart() {
        let (l, d) = (LIGHT.named(), DARK.named());
        assert_eq!(l.len(), d.len());
        for ((ln, lc), (dn, dc)) in l.iter().zip(d.iter()) {
            assert_eq!(ln, dn, "token order differs");
            assert_ne!(*lc, Color32::TRANSPARENT, "{ln} light is unset");
            assert_ne!(*dc, Color32::TRANSPARENT, "{dn} dark is unset");
        }
        // the two sets are different themes: the window colours differ
        assert_ne!(LIGHT.bg, DARK.bg);
        assert_ne!(LIGHT.surface, DARK.surface);
        assert_ne!(LIGHT.ink, DARK.ink);
    }

    #[test]
    fn token_table_matches_the_spec() {
        // A few values from docs/phase3-spec.md <design_tokens>, light and dark.
        assert_eq!(LIGHT.bg, Color32::from_rgb(0xF6, 0xF7, 0xF9));
        assert_eq!(LIGHT.accent, Color32::from_rgb(0x1F, 0x5F, 0xAD));
        assert_eq!(LIGHT.summary_bg, Color32::from_rgb(0x1F, 0x3A, 0x5F));
        assert_eq!(LIGHT.danger, Color32::from_rgb(0xB4, 0x23, 0x18));
        assert_eq!(LIGHT.header_bg, Color32::from_rgb(0xE9, 0xED, 0xF3));
        assert_eq!(DARK.header_bg, Color32::from_rgb(0x1D, 0x25, 0x2E));
        assert_eq!(DARK.bg, Color32::from_rgb(0x0F, 0x14, 0x19));
        assert_eq!(DARK.accent, Color32::from_rgb(0xE8, 0xA3, 0x3D));
        assert_eq!(DARK.success_soft, Color32::from_rgb(0x15, 0x30, 0x1F));
        let names: Vec<&str> = LIGHT.named().iter().map(|(n, _)| *n).collect();
        assert_eq!(names.len(), 24);
        assert!(names.contains(&"selector_off_fg") && names.contains(&"accent_soft_border"));
    }

    #[test]
    fn sizes_follow_the_spec() {
        // the single top bar (44) replaced the 36 px title strip and the 52 px toolbar
        assert_eq!((TOP_BAR_H, TAB_STRIP_H, STATUS_BAR_H, RIGHT_PANEL_W), (44.0, 40.0, 32.0, 316.0));
        assert_eq!((HEADER_ROW_H, SELECTOR_ROW_H, SUMMARY_BAND_H, DATA_ROW_H), (38.0, 30.0, 38.0, 32.0));
        assert_eq!((TEXT_BASE, TEXT_SMALL, TEXT_LABEL, TEXT_INSPECTOR_TITLE), (14.0, 13.0, 12.0, 20.0));
    }

    #[test]
    fn status_symbols_become_icon_glyphs() {
        let s = icons_in_text("\u{26A0} careful \u{2705} done \u{274C} no \u{1F511} key");
        assert!(!s.contains('\u{26A0}') && !s.contains('\u{2705}') && !s.contains('\u{274C}') && !s.contains('\u{1F511}'));
        assert!(s.contains(ph::WARNING) && s.contains(ph::CHECK_CIRCLE) && s.contains(ph::X_CIRCLE) && s.contains(ph::KEY));
        assert!(s.contains("careful") && s.contains("key"));
    }

    /// Every icon escape and every other non-ASCII character in the UI source must exist in the
    /// loaded fonts, so nothing renders as an empty box.
    #[test]
    fn every_symbol_in_the_ui_source_has_a_glyph() {
        let ctx = egui::Context::default();
        load_fonts(&ctx);
        let _ = ctx.run(Default::default(), |_| {});
        let mut chars: std::collections::BTreeSet<char> = std::collections::BTreeSet::new();
        let mut files: Vec<std::path::PathBuf> = std::fs::read_dir("src/ui")
            .expect("src/ui")
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| p.extension().is_some_and(|x| x == "rs"))
            .collect();
        files.push("src/main.rs".into());
        for f in files {
            if f.ends_with("theme.rs") {
                continue; // holds the symbol table itself
            }
            let text = std::fs::read_to_string(&f).expect("read source");
            for line in text.lines() {
                if line.trim_start().starts_with("//") {
                    continue;
                }
                // \u{XXXX} escapes
                let mut rest = line;
                while let Some(i) = rest.find("\\u{") {
                    let after = &rest[i + 3..];
                    if let Some(j) = after.find('}') {
                        if let Ok(cp) = u32::from_str_radix(&after[..j], 16) {
                            if let Some(c) = char::from_u32(cp) {
                                chars.insert(c);
                            }
                        }
                        rest = &after[j..];
                    } else {
                        break;
                    }
                }
                chars.extend(line.chars().filter(|c| !c.is_ascii()));
            }
        }
        assert!(chars.len() > 10, "found {} symbols", chars.len());
        let families = [FontFamily::Proportional, family_semibold()];
        let missing: Vec<String> = ctx.fonts(|f| {
            let mut m = Vec::new();
            for fam in &families {
                let id = FontId::new(14.0, fam.clone());
                for c in &chars {
                    if !f.has_glyph(&id, *c) {
                        m.push(format!("U+{:04X} {:?} in {:?}", *c as u32, c, fam));
                    }
                }
            }
            m
        });
        assert!(missing.is_empty(), "no glyph for: {:?}", missing);
    }
}
