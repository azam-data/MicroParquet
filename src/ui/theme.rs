//! Design tokens (light and dark), sizes and the egui style. The only place in the UI code that
//! holds colour or font-size literals. R1 keeps the app's look as it was (`apply_ui_theme` uses the
//! pre-Phase-3 values); L1 switches it to the token sets below.
#![allow(dead_code)] // the token tables are used from L1 on

use eframe::egui::{self, Color32};

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

// ---- Colours the app used before Phase 3 (R1 keeps them so nothing changes by eye; L1 replaces
// them with the tokens above)
pub const WARN_ORANGE: Color32 = Color32::from_rgb(210, 70, 0);
pub const WARN_AMBER: Color32 = Color32::from_rgb(220, 120, 0);
pub const INFO_BLUE: Color32 = Color32::from_rgb(0, 110, 200);
pub const ERR_RED: Color32 = Color32::from_rgb(200, 30, 30);

// pre-Phase-3 window colours
const OLD_DARK_TEXT: Color32 = Color32::from_rgb(240, 244, 248);
const OLD_DARK_PANEL: Color32 = Color32::from_rgb(22, 27, 34);
const OLD_DARK_FAINT: Color32 = Color32::from_rgb(33, 38, 45);
const OLD_LIGHT_TEXT: Color32 = Color32::from_rgb(15, 23, 42);
const OLD_LIGHT_PANEL: Color32 = Color32::from_rgb(248, 250, 252);
const OLD_LIGHT_WINDOW: Color32 = Color32::from_rgb(255, 255, 255);
const OLD_LIGHT_FAINT: Color32 = Color32::from_rgb(236, 242, 249);
const OLD_LIGHT_SELECTION: Color32 = Color32::from_rgb(2, 132, 199);
const OLD_LIGHT_STROKE: Color32 = Color32::from_rgb(203, 213, 225);

pub fn apply_ui_theme(ctx: &egui::Context, dark_mode: bool, zoom: f32) {
    ctx.set_zoom_factor(zoom);

    let mut style = (*ctx.style()).clone();
    style.text_styles.insert(
        egui::TextStyle::Body,
        egui::FontId::new(13.5, egui::FontFamily::Proportional),
    );
    style.text_styles.insert(
        egui::TextStyle::Button,
        egui::FontId::new(13.5, egui::FontFamily::Proportional),
    );
    style.text_styles.insert(
        egui::TextStyle::Monospace,
        egui::FontId::new(13.0, egui::FontFamily::Monospace),
    );
    style.text_styles.insert(
        egui::TextStyle::Heading,
        egui::FontId::new(17.5, egui::FontFamily::Proportional),
    );
    style.spacing.item_spacing = egui::vec2(7.0, 5.0);
    style.spacing.button_padding = egui::vec2(7.0, 3.5);
    ctx.set_style(style);

    let mut visuals = if dark_mode {
        let mut v = egui::Visuals::dark();
        v.override_text_color = Some(OLD_DARK_TEXT);
        v.panel_fill = OLD_DARK_PANEL;
        v.faint_bg_color = OLD_DARK_FAINT;
        v
    } else {
        let mut v = egui::Visuals::light();
        v.override_text_color = Some(OLD_LIGHT_TEXT);
        v.panel_fill = OLD_LIGHT_PANEL;
        v.window_fill = OLD_LIGHT_WINDOW;
        v.faint_bg_color = OLD_LIGHT_FAINT;
        v.extreme_bg_color = OLD_LIGHT_WINDOW;
        v.selection.bg_fill = OLD_LIGHT_SELECTION;
        v.selection.stroke = egui::Stroke::new(1.0_f32, egui::Color32::WHITE);
        v.widgets.noninteractive.bg_stroke =
            egui::Stroke::new(1.0_f32, OLD_LIGHT_STROKE);
        v
    };

    visuals.striped = true;
    ctx.set_visuals(visuals);
}
