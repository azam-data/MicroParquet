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
    ink: hex(0x1B2430),
    ink2: hex(0x3D4856),
    muted: hex(0x5B6675),
    accent: hex(0x1F5FAD),
    accent_dark: hex(0x174A87),
    accent_soft: hex(0xEAF1FB),
    accent_soft_border: hex(0xC9DBF3),
    summary_bg: hex(0x1F3A5F),
    summary_fg: hex(0xFFFFFF),
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
pub const TITLE_BAR_H: f32 = 36.0;
pub const TOOLBAR_H: f32 = 52.0;
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


impl Tokens {
    /// Every token with its name, in one fixed order (used by the light/dark completeness test).
    pub fn named(&self) -> [(&'static str, Color32); 23] {
        [
            ("bg", self.bg),
            ("surface", self.surface),
            ("border", self.border),
            ("grid", self.grid),
            ("zebra", self.zebra),
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

/// Where Source Sans 3 is looked for: next to the executable, one level up (development builds),
/// and the source tree the program was built from.
fn sans_font_dirs() -> Vec<std::path::PathBuf> {
    let mut dirs = Vec::new();
    if let Ok(exe) = std::env::current_exe() {
        if let Some(d) = exe.parent() {
            dirs.push(d.join("assets").join("fonts"));
            if let Some(up) = d.parent() {
                dirs.push(up.join("assets").join("fonts"));
                if let Some(up2) = up.parent() {
                    dirs.push(up2.join("assets").join("fonts"));
                }
            }
        }
    }
    dirs.push(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("assets").join("fonts"));
    dirs
}

fn read_sans(file: &str) -> Option<Vec<u8>> {
    sans_font_dirs().into_iter().find_map(|d| std::fs::read(d.join(file)).ok())
}

/// Loads the fonts: JetBrains Mono (bundled in the program), Source Sans 3 (the TTF files in
/// `assets/fonts/`; without them the UI keeps egui's default text font) and the Phosphor icons.
/// Returns true when Source Sans 3 was found.
pub fn load_fonts(ctx: &egui::Context) -> bool {
    use egui::FontData;
    let mut fonts = egui::FontDefinitions::default();
    let proportional_defaults = fonts.families.get(&FontFamily::Proportional).cloned().unwrap_or_default();
    let mono_defaults = fonts.families.get(&FontFamily::Monospace).cloned().unwrap_or_default();

    fonts.font_data.insert(
        "jbm-regular".into(),
        FontData::from_static(include_bytes!("../../assets/fonts/JetBrainsMono-Regular.ttf")),
    );
    fonts.font_data.insert(
        "jbm-semibold".into(),
        FontData::from_static(include_bytes!("../../assets/fonts/JetBrainsMono-SemiBold.ttf")),
    );

    let mut sans_found = false;
    let mut sans_keys: [Option<&str>; 3] = [None, None, None];
    for (i, (key, file)) in [
        ("sans-regular", "SourceSans3-Regular.ttf"),
        ("sans-semibold-font", "SourceSans3-Semibold.ttf"),
        ("sans-bold-font", "SourceSans3-Bold.ttf"),
    ]
    .into_iter()
    .enumerate()
    {
        if let Some(bytes) = read_sans(file) {
            fonts.font_data.insert(key.into(), FontData::from_owned(bytes));
            sans_keys[i] = Some(key);
        }
    }
    if sans_keys[0].is_some() {
        sans_found = true;
    }

    // Proportional: Source Sans 3 first, then egui's own fallbacks.
    if let Some(k) = sans_keys[0] {
        let mut list = vec![k.to_string()];
        list.extend(proportional_defaults.iter().cloned());
        fonts.families.insert(FontFamily::Proportional, list);
    }
    // Monospace: JetBrains Mono first.
    let mut mono_list = vec!["jbm-regular".to_string()];
    mono_list.extend(mono_defaults.iter().cloned());
    fonts.families.insert(FontFamily::Monospace, mono_list);

    // Named families always exist (egui panics on an unknown family); they fall back to the
    // regular fonts when a weight is missing.
    let named = |first: Option<&str>, base: &Vec<String>| -> Vec<String> {
        let mut v: Vec<String> = first.map(|k| vec![k.to_string()]).unwrap_or_default();
        v.extend(base.iter().cloned());
        v
    };
    let prop_now = fonts.families.get(&FontFamily::Proportional).cloned().unwrap_or_default();
    fonts.families.insert(family_semibold(), named(sans_keys[1], &prop_now));
    fonts.families.insert(family_bold(), named(sans_keys[2].or(sans_keys[1]), &prop_now));
    let mono_now = fonts.families.get(&FontFamily::Monospace).cloned().unwrap_or_default();
    fonts.families.insert(family_mono_semibold(), named(Some("jbm-semibold"), &mono_now));

    // Icons: Phosphor, as a fallback behind the first font of every family.
    egui_phosphor::add_to_fonts(&mut fonts, egui_phosphor::Variant::Regular);
    for fam in [FontFamily::Monospace, family_semibold(), family_bold(), family_mono_semibold()] {
        if let Some(list) = fonts.families.get_mut(&fam) {
            let at = 1.min(list.len());
            list.insert(at, "phosphor".into());
        }
    }
    ctx.set_fonts(fonts);
    sans_found
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

/// A modal window with the same title style everywhere.
pub fn modal(title: &str) -> egui::Window<'static> {
    egui::Window::new(RichText::new(title).family(family_semibold())).collapsible(false)
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
        assert_eq!(DARK.bg, Color32::from_rgb(0x0F, 0x14, 0x19));
        assert_eq!(DARK.accent, Color32::from_rgb(0xE8, 0xA3, 0x3D));
        assert_eq!(DARK.success_soft, Color32::from_rgb(0x15, 0x30, 0x1F));
        let names: Vec<&str> = LIGHT.named().iter().map(|(n, _)| *n).collect();
        assert_eq!(names.len(), 23);
        assert!(names.contains(&"selector_off_fg") && names.contains(&"accent_soft_border"));
    }

    #[test]
    fn sizes_follow_the_spec() {
        assert_eq!((TITLE_BAR_H, TOOLBAR_H, TAB_STRIP_H, STATUS_BAR_H, RIGHT_PANEL_W), (36.0, 52.0, 40.0, 32.0, 316.0));
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
