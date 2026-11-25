//! Design tokens (light and dark), sizes, fonts, icons and the shared widgets. The only place in
//! the UI code that holds colour or font-size literals.
#![allow(dead_code)] // some tokens and helpers are used by later screens

use eframe::egui::{self, Color32, FontFamily, FontId, Response, RichText, Rounding, Stroke};

pub const fn hex(rgb: u32) -> Color32 {
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

// ---- Chart tokens (Phase 7): the design system of the charts. The values live in
// `micro_parquet::charts::tokens` (the HTML and Excel renderers read the same ones); these are the
// UI's `Color32` and `f32` names for them.
use micro_parquet::charts::tokens as ct;

const fn hex_all<const N: usize>(a: [u32; N]) -> [Color32; N] {
    let mut out = [Color32::TRANSPARENT; N];
    let mut i = 0;
    while i < N {
        out[i] = hex(a[i]);
        i += 1;
    }
    out
}

/// The surface, inks, gridline, baseline and de-emphasised colour of a chart.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ChartColours {
    pub surface: Color32,
    pub ink: Color32,
    pub ink2: Color32,
    pub muted: Color32,
    pub grid: Color32,
    pub baseline: Color32,
    pub deemph: Color32,
}

const fn chart_colours(i: ct::ChartInk) -> ChartColours {
    ChartColours { surface: hex(i.surface), ink: hex(i.ink), ink2: hex(i.ink2), muted: hex(i.muted), grid: hex(i.grid), baseline: hex(i.baseline), deemph: hex(i.deemph) }
}

pub const CHART_LIGHT: ChartColours = chart_colours(ct::INK_LIGHT);
pub const CHART_DARK: ChartColours = chart_colours(ct::INK_DARK);
pub const CHART_SERIES_LIGHT: [Color32; ct::SLOTS] = hex_all(ct::SERIES_LIGHT);
pub const CHART_SERIES_DARK: [Color32; ct::SLOTS] = hex_all(ct::SERIES_DARK);
pub const CHART_SEQUENTIAL: [Color32; 13] = hex_all(ct::SEQUENTIAL);
pub const CHART_DIVERGING_POSITIVE: Color32 = hex(ct::DIVERGING_POSITIVE);
pub const CHART_DIVERGING_NEGATIVE: Color32 = hex(ct::DIVERGING_NEGATIVE);
pub const CHART_DIVERGING_ZERO: Color32 = hex(ct::DIVERGING_ZERO);
pub const CHART_STATUS_GOOD: Color32 = hex(ct::STATUS_GOOD);
pub const CHART_STATUS_WARNING: Color32 = hex(ct::STATUS_WARNING);
pub const CHART_STATUS_SERIOUS: Color32 = hex(ct::STATUS_SERIOUS);
pub const CHART_STATUS_CRITICAL: Color32 = hex(ct::STATUS_CRITICAL);
pub const CHART_WATERFALL_START: Color32 = hex(ct::WATERFALL_START);
pub const CHART_WATERFALL_END: Color32 = hex(ct::WATERFALL_END);

pub fn chart_colours_for(dark: bool) -> &'static ChartColours {
    if dark {
        &CHART_DARK
    } else {
        &CHART_LIGHT
    }
}

pub fn chart_series_colour(slot: usize, dark: bool) -> Color32 {
    let t = if dark { &CHART_SERIES_DARK } else { &CHART_SERIES_LIGHT };
    t.get(slot).copied().unwrap_or(chart_colours_for(dark).deemph)
}

pub const CHART_TITLE_PT: f32 = ct::TITLE_PT;
pub const CHART_SUBTITLE_PT: f32 = ct::SUBTITLE_PT;
pub const CHART_TICK_PT: f32 = ct::TICK_PT;
pub const CHART_LABEL_PT: f32 = ct::LABEL_PT;
pub const CHART_DATA_LABEL_PT: f32 = ct::DATA_LABEL_PT;
pub const CHART_FOOTNOTE_PT: f32 = ct::FOOTNOTE_PT;
pub const CHART_BAR_MAX_PX: f32 = ct::BAR_MAX_PX;
pub const CHART_BAR_RADIUS_PX: f32 = ct::BAR_RADIUS_PX;
pub const CHART_SEGMENT_GAP_PX: f32 = ct::SEGMENT_GAP_PX;
pub const CHART_LINE_PX: f32 = ct::LINE_PX;
pub const CHART_MARKER_PX: f32 = ct::MARKER_PX;
pub const CHART_MARKER_RING_PX: f32 = ct::MARKER_RING_PX;
pub const CHART_AREA_OPACITY: f32 = ct::AREA_OPACITY;
pub const CHART_GRID_PX: f32 = ct::GRID_PX;
pub const CHART_REFERENCE_LINE_PX: f32 = ct::REFERENCE_LINE_PX;
pub const CHART_HEAT_RADIUS_PX: f32 = ct::HEAT_RADIUS_PX;
pub const CHART_STACKED_AREA_OPACITY: f32 = ct::STACKED_AREA_OPACITY;
pub const CHART_HEAT_GAP_PX: f32 = ct::HEAT_GAP_PX;
pub const CHART_PADDING_SHARE: f32 = ct::PADDING_SHARE;
pub const CHART_MIN_SPACE_PX: f32 = ct::MIN_SPACE_PX;
pub const CHART_HIT_SLOP_PX: f32 = ct::HIT_SLOP_PX;

// ---- Sizes (logical px at 100% UI scale)
pub const TEXT_BASE: f32 = 14.0;
pub const TEXT_SMALL: f32 = 13.0;
pub const TEXT_LABEL: f32 = 12.0;
pub const TEXT_INSPECTOR_TITLE: f32 = 20.0;
/// The single top bar (app mark, menus, toolbar groups, search); replaces the 36 px title strip
/// and the 52 px toolbar.
pub const TOP_BAR_H: f32 = 44.0;
pub const TAB_STRIP_H: f32 = 40.0;
/// The cross of a tab: its square hit area, and the width of the rename box.
pub const TAB_CLOSE_SIZE: f32 = 22.0;
pub const TAB_RENAME_W: f32 = 160.0;
/// For how many frames the strip keeps scrolling to a tab that became active.
pub const TAB_SCROLL_FRAMES: u8 = 4;
/// One half, for centring.
pub const HALF: f32 = 0.5;
/// The most decimal places a report can show.
pub const MAX_DECIMALS: usize = 6;
/// The most lines the commentary box shows before it scrolls.
pub const MOM_BOX_ROWS: usize = 8;
pub const STATUS_BAR_H: f32 = 32.0;
pub const RIGHT_PANEL_W: f32 = 316.0;
/// The right panel can be dragged as wide as this share of the window, and its left edge reacts
/// within this many points (double-click there goes back to the minimum width).
pub const RIGHT_PANEL_MAX_SHARE: f32 = 0.5;
pub const RIGHT_PANEL_GRAB: f32 = 6.0;
/// The line drawn on the panel's edge while it is pointed at or dragged.
pub const LINE_THICK_W: f32 = 2.0;
pub const HEADER_ROW_H: f32 = 38.0;
pub const SELECTOR_ROW_H: f32 = 30.0;
pub const SUMMARY_BAND_H: f32 = 38.0;
pub const DATA_ROW_H: f32 = 32.0;
/// One level of a Nested pivot's "Row Labels" is indented this far on the screen (Phase 9 P2).
pub const NESTED_INDENT_PX: f32 = 16.0;
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
/// The narrowest the schema dialog can be made: its header buttons still fit.
pub const SCHEMA_DIALOG_MIN_W: f32 = 640.0;
/// The size of the schema dialog's own native window, and the smallest it can be made.
pub const SCHEMA_POPOUT_SIZE: [f32; 2] = [1100.0, 800.0];
pub const SCHEMA_POPOUT_MIN_SIZE: [f32; 2] = [640.0, 480.0];
/// The chart's own window: its size when it opens and the least it can be made.
/// The product and weight fields of the Mudaraba parameter editor.
pub const MUD_PRODUCT_FIELD_W: f32 = 110.0;
pub const MUD_WEIGHT_FIELD_W: f32 = 60.0;
/// The pivot builder's own window (Phase 9 P1): where it opens the first time, and the least it can be made.
pub const PIVOT_POPOUT_SIZE: [f32; 2] = [1280.0, 800.0];
pub const PIVOT_POPOUT_MIN_SIZE: [f32; 2] = [760.0, 520.0];
/// The window is never made bigger than this share of its monitor.
pub const PIVOT_POPOUT_MONITOR_SHARE: f32 = 0.98;
pub const CHART_POPOUT_SIZE: [f32; 2] = [900.0, 560.0];
pub const CHART_POPOUT_MIN_SIZE: [f32; 2] = [520.0, 360.0];
/// Space between the items of the two option rows.
pub const SCHEMA_ROW_GAP: f32 = 6.0;
/// The Columns card is never shorter than this: the dialog scrolls instead.
pub const SCHEMA_COLUMNS_MIN_H: f32 = 260.0;
pub const PREVIEW_ROWS: usize = 5;
/// Header row, 5 data rows (each followed by egui's 6 px row spacing) and the frame around them.
pub const PREVIEW_GRID_H: f32 = HEADER_ROW_H + PREVIEW_ROWS as f32 * (DATA_ROW_H + 6.0) + 8.0;
pub const SCHEMA_COMBO_W: f32 = 96.0;
pub const SCHEMA_COL_NAME_W: f32 = 190.0;
pub const SCHEMA_COL_RENAME_W: f32 = 150.0;
pub const SCHEMA_COL_TYPE_W: f32 = 118.0;
pub const SCHEMA_COL_PII_W: f32 = 118.0;
pub const SCHEMA_COL_SIGN_W: f32 = 118.0;
pub const SCHEMA_COL_FAIL_W: f32 = 84.0;
pub const SCHEMA_COL_MIN_W: f32 = 60.0;
/// Top bar pieces.
pub const APP_MARK_SIZE: f32 = 18.0;
pub const APP_MARK_RADIUS: f32 = 4.0;

/// The app logo (assets/icon/microparquet.svg): a navy tile with three rising column-planks, drawn
/// in a 1024-unit design square. The menu-bar mark paints it with these; the window and taskbar
/// icon is the rendered `assets/icon/icon_256.rgba`.
pub const LOGO_TILE: Color32 = hex(0x1F3A5F);
pub const LOGO_PLANKS: [Color32; 3] = [hex(0x9CC3F0), hex(0xFFFFFF), hex(0x5B9BE6)];
const LOGO_UNITS: f32 = 1024.0;
const LOGO_PLANK_X: [f32; 3] = [175.0, 417.0, 659.0];
const LOGO_PLANK_W: f32 = 190.0;
const LOGO_PLANK_TOP: [f32; 3] = [470.0, 340.0, 210.0];
const LOGO_PLANK_BASE: f32 = 820.0;
const LOGO_PLANK_ROUND: f32 = 34.0;
/// Side of the window icon (`assets/icon/icon_256.rgba`, RGBA, 4 bytes a pixel).
pub const APP_ICON_SIDE: u32 = 256;

/// Paints the app logo in `rect` (a square): the tile, then the three planks.
pub fn paint_app_logo(painter: &egui::Painter, rect: egui::Rect) {
    let s = rect.width() / LOGO_UNITS;
    painter.rect_filled(rect, APP_MARK_RADIUS, LOGO_TILE);
    for i in 0..3 {
        let plank = egui::Rect::from_min_max(
            rect.min + egui::vec2(LOGO_PLANK_X[i] * s, LOGO_PLANK_TOP[i] * s),
            rect.min + egui::vec2((LOGO_PLANK_X[i] + LOGO_PLANK_W) * s, LOGO_PLANK_BASE * s),
        );
        painter.rect_filled(plank, LOGO_PLANK_ROUND * s, LOGO_PLANKS[i]);
    }
}

/// The window and taskbar icon (replaces eframe's default "e").
pub fn app_icon() -> egui::IconData {
    egui::IconData {
        rgba: include_bytes!("../../assets/icon/icon_256.rgba").to_vec(),
        width: APP_ICON_SIDE,
        height: APP_ICON_SIDE,
    }
}
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
pub const SEARCH_MIN_W: f32 = 120.0;
pub const SEARCH_ICON_W: f32 = 44.0;
/// The box's own side padding (10 + 10); egui adds nothing for the border.
pub const SEARCH_FRAME_PAD: f32 = 20.0;
/// From this width the box also shows its "Ctrl K" badge.
pub const SEARCH_BADGE_MIN_W: f32 = 170.0;
/// From this width the box says "Search actions and columns"; narrower it says "Search…".
pub const SEARCH_LONG_HINT_W: f32 = 260.0;


// ---- Window, scale, lines, radii and the remaining layout sizes (Phase 3 fixes, part 2, item 8).
// Every pixel size the screens use is here; the screens name them. (`0.0`, `0.5`, `1.0` and `2.0`
// are arithmetic, not sizes: zero, a half-pixel line offset, one, a half.)
pub const WINDOW_SIZE: [f32; 2] = [1320.0, 740.0];
pub const WINDOW_MIN_SIZE: [f32; 2] = [800.0, 560.0];
pub const UI_ZOOM_DEFAULT: f32 = 1.05;
pub const UI_ZOOM_MIN: f32 = 0.50;
pub const UI_ZOOM_MAX: f32 = 2.00;
pub const UI_ZOOM_STEP: f32 = 0.10;
/// The status bar shows the zoom label only when the bar is at least this wide.
pub const STATUS_ZOOM_MIN_BAR_W: f32 = 900.0;
/// The list of default zooms in the Zoom menu scrolls above this height.
pub const ZOOM_LIST_H: f32 = 280.0;
/// The default zoom is chosen in steps of this much (5%).
pub const UI_ZOOM_CHOICE_STEP: f32 = 0.05;
/// A zoom rounded to the nearest 5% and kept between the minimum and the maximum.
pub fn snap_zoom(z: f32) -> f32 {
    ((z / UI_ZOOM_CHOICE_STEP).round() * UI_ZOOM_CHOICE_STEP).clamp(UI_ZOOM_MIN, UI_ZOOM_MAX)
}
/// The UI scale one step larger / smaller, kept between the minimum and the maximum.
pub fn zoom_in(z: f32) -> f32 {
    snap_zoom(z + UI_ZOOM_STEP)
}
pub fn zoom_out(z: f32) -> f32 {
    snap_zoom(z - UI_ZOOM_STEP)
}
/// Every default zoom the menu offers: 50% to 200% in steps of 5%.
pub fn zoom_choices() -> Vec<f32> {
    let n = ((UI_ZOOM_MAX - UI_ZOOM_MIN) / UI_ZOOM_CHOICE_STEP).round() as usize;
    (0..=n).map(|i| ((UI_ZOOM_MIN + i as f32 * UI_ZOOM_CHOICE_STEP) * 100.0).round() / 100.0).collect()
}
/// A 1 px line (borders, dividers) and a 2 px line (the selected column's underline, the active
/// tab's top line, the steps rail).
pub fn line(color: Color32) -> Stroke {
    Stroke::new(1.0_f32, color)
}
pub fn line_thick(color: Color32) -> Stroke {
    Stroke::new(2.0_f32, color)
}
pub const RADIUS_SMALL: f32 = 4.0;
pub const RADIUS_PILL: f32 = 10.0;
pub const RADIUS_CHIP: f32 = 14.0;
pub const RADIUS_PALETTE: f32 = 10.0;
pub const RADIUS_BAR: f32 = 3.0;
/// Rounded at the top only: the tabs and the histogram bars.
pub const ROUNDING_TAB_TOP: egui::Rounding = egui::Rounding { nw: RADIUS_CONTROL, ne: RADIUS_CONTROL, sw: 0.0, se: 0.0 };
pub const ROUNDING_BAR_TOP: egui::Rounding = egui::Rounding { nw: RADIUS_BAR, ne: RADIUS_BAR, sw: 0.0, se: 0.0 };
// dialogs: widths (the schema dialog's is above) and the lists inside them
pub const DIALOG_W_SORT: f32 = 700.0;
/// The Links and Key check dialogs (Phase 9 M0): a share of the pivot builder's width, the width of a combo box, how many lines of a list.
pub const MODEL_DIALOG_SHARE: f32 = 0.8;
pub const MODEL_COMBO_W: f32 = 190.0;
/// The cell of a data tab's title in the date-link rows of Model > Calendar..., so every box has one width and one left edge.
pub const MODEL_LINK_TITLE_W: f32 = 170.0;
/// Model > Measures...: the dialog, a column name's cell and the tag box.
pub const DIALOG_W_MEASURES: f32 = 640.0;
pub const MEASURE_NAME_W: f32 = 170.0;
pub const MEASURE_TAG_W: f32 = 150.0;
/// The editor of a measure (M5): a wide box, a name box, a sign box, a test box, a value box, a day-count box.
pub const MEASURE_BOX_W: f32 = 190.0;
pub const MEASURE_NAME_BOX_W: f32 = 230.0;
pub const MEASURE_SIGN_W: f32 = 110.0;
pub const MEASURE_FIELD_W: f32 = 170.0;
pub const MEASURE_TEST_W: f32 = 90.0;
pub const MEASURE_VALUE_W: f32 = 130.0;
pub const MEASURE_DAYCOUNT_W: f32 = 240.0;
pub const MODEL_LIST_LINES: usize = 8;
/// The width of the PDF export dialog's title box.
pub const PDF_TITLE_FIELD_W: f32 = 320.0;
pub const DIALOG_W_ABOUT: f32 = 560.0;
pub const DIALOG_W_FILTER: f32 = 640.0;
pub const DIALOG_W_TOOL: f32 = 680.0;
pub const DIALOG_W_PIVOT: f32 = 720.0;
pub const DIALOG_W_PIVOT_CONVERT: f32 = 520.0;
pub const DIALOG_W_PYTHON: f32 = 760.0;
// The pivot builder (Phase 4 V1), after docs/design/Pivot.dc.html.
pub const DIALOG_W_PIVOT_BUILDER: f32 = 1100.0;
pub const DIALOG_MIN_W_PIVOT_BUILDER: f32 = 700.0;
pub const PIVOT_FIELDS_W: f32 = 240.0;
/// The field list takes at most this share of the dialog's width.
pub const PIVOT_FIELDS_MAX_SHARE: f32 = 0.35;
pub const PIVOT_FIELD_ROW_H: f32 = 28.0;
pub const PIVOT_BADGE_W: f32 = 26.0;
pub const PIVOT_BADGE_H: f32 = 18.0;
pub const PIVOT_AREA_MIN_H: f32 = 72.0;
pub const PIVOT_CHIP_H: f32 = 26.0;
pub const PIVOT_PREVIEW_MIN_H: f32 = 110.0;
pub const PIVOT_PREVIEW_COL_W: f32 = 118.0;
pub const PIVOT_PREVIEW_ROW_H: f32 = 28.0;
pub const PIVOT_PREVIEW_LABEL_W: f32 = 190.0;
pub const PIVOT_POPUP_W: f32 = 300.0;
pub const PIVOT_POPUP_LIST_H: f32 = 220.0;
pub const PIVOT_COMBO_W: f32 = 130.0;
/// The pivot's Layout box ("Nested (Excel style)").
pub const PIVOT_COMBO_LAYOUT_W: f32 = 160.0;
pub const PIVOT_COMBO_AGG_W: f32 = 104.0;
/// The aggregation box of a value that has a time aggregation ("Period amount (YTD balance)").
pub const PIVOT_COMBO_TIME_W: f32 = 196.0;
/// The "Time" button of a value that has a time aggregation (window, shift, show, partial period), and its popup.
pub const PIVOT_TIME_BTN_W: f32 = 54.0;
pub const PIVOT_TIME_BTN_H: f32 = PIVOT_CHIP_H - 4.0;
pub const PIVOT_TIME_POPUP_W: f32 = 430.0;
/// The most height the aggregation list of a value has before it scrolls (7 aggregations, a rule and 8 over time).
pub const PIVOT_AGG_LIST_H: f32 = 460.0;
/// The room the aggregation list leaves below it, and the least height it has.
pub const PIVOT_AGG_LIST_MARGIN: f32 = 24.0;
pub const PIVOT_AGG_LIST_MIN_H: f32 = 120.0;
pub const PIVOT_COMBO_WEIGHT_W: f32 = 120.0;
pub const PIVOT_COMBO_SORT_W: f32 = 210.0;
pub const PIVOT_FOOTER_BUTTONS_W: f32 = 250.0;
pub const PIVOT_GAP: f32 = 12.0;
/// How much of the warning colour tints a downgrade in the CL migration matrix (green uses `success_soft`).
pub const SHADE_AMBER_STRENGTH: f32 = 0.22;
/// How much of the danger colour tints a breached limit in the liquidity gap ladder.
pub const SHADE_DANGER_STRENGTH: f32 = 0.18;
pub const MIGRATION_DIALOG_W: f32 = 640.0;
pub const GAP_DIALOG_W: f32 = 940.0;
pub const CURVES_DIALOG_W: f32 = 860.0;
pub const CURVES_LIST_W: f32 = 270.0;
pub const CURVES_BODY_MIN_H: f32 = 300.0;
pub const GAP_COMBO_W: f32 = 150.0;
pub const GAP_COMBO_NARROW_W: f32 = 120.0;
/// The column and default choices of the IRRBB tools' dialogs.
pub const CF_COMBO_W: f32 = 160.0;
pub const CF_DEFAULT_W: f32 = 130.0;
pub const CF_WIDE_W: f32 = 150.0;
pub const CF_DIALOG_W: f32 = 1040.0;
pub const BEHAVIOUR_DIALOG_W: f32 = 1100.0;
pub const BEHAVIOUR_CELL_W: f32 = 80.0;
pub const BEHAVIOUR_SOURCE_W: f32 = 320.0;
pub const MIGRATION_COMBO_W: f32 = 200.0;
pub const PIVOT_CHIP_MENU_W: f32 = 24.0;
/// The height of a row of the Banking and Export menus (compact, so the Banking menu fits a 686 pt window).
pub const MENU_ROW_H: f32 = 24.0;
/// The combo boxes of the LCR / NSFR dialog: the map file and the balance sign.
pub const LIQ_MAP_COMBO_W: f32 = 225.0;
pub const LIQ_SIGN_COMBO_W: f32 = 270.0;
pub const LIST_MAX_H_S: f32 = 120.0;
pub const LIST_MAX_H_M: f32 = 180.0;
pub const LIST_MAX_H_L: f32 = 240.0;
pub const PYTHON_PREVIEW_MAX_H: f32 = 360.0;
// input fields
pub const FIELD_W_S: f32 = 90.0;
/// The conditional-column dialog: a column picker, the operator list, "value / column".
pub const COND_COMBO_W: f32 = 140.0;
pub const COND_OP_W: f32 = 150.0;
pub const COND_KIND_W: f32 = 70.0;
/// The Rate profile dialog: the preset picker, and how many band labels its preview lists.
pub const RATE_PRESET_W: f32 = 220.0;
pub const RATE_LABELS_SHOWN: usize = 24;
/// The Rate profile's start / end / step fields (percent): how fast a drag changes them and
/// what they may be.
pub const RATE_DRAG_SPEED: f64 = 0.1;
pub const RATE_STEP_DRAG_SPEED: f64 = 0.05;
pub const RATE_PCT_RANGE: std::ops::RangeInclusive<f64> = 0.0..=100.0;
pub const RATE_STEP_RANGE: std::ops::RangeInclusive<f64> = 0.01..=50.0;
/// The width the conditional-column rows wrap at, and room for the dialog's scroll bar.
pub const COND_BODY_W: f32 = 720.0;
pub const DIALOG_SCROLL_BAR_ALLOWANCE: f32 = 16.0;
pub const FIELD_W_VALUE: f32 = 130.0;
/// A value box in a narrow panel (the chart studio's "Add a filter" row, W9).
pub const FIELD_W_VALUE_NARROW: f32 = 100.0;
/// The value picker: its search box, its list's height and the button that opens it.
pub const PICKER_SEARCH_W: f32 = 200.0;
/// The room the picker's two buttons need beside its search box, and the least the box is narrowed to.
pub const PICKER_BUTTONS_W: f32 = 130.0;
pub const PICKER_SEARCH_MIN_W: f32 = 90.0;
pub const PICKER_LIST_H: f32 = 200.0;
pub const FIELD_W_PICK: f32 = 150.0;
pub const FIELD_W_OP: f32 = 120.0;
pub const FIELD_W_COLUMN: f32 = 160.0;
pub const FIELD_W_SORT_COLUMN: f32 = 180.0;
/// The order choice (Natural, a list, Tenor) of a sort level.
pub const FIELD_W_SORT_ORDER: f32 = 120.0;
/// The name, columns and several-lines boxes of the custom sort-list manager.
pub const FIELD_W_SORT_LIST: f32 = 300.0;
pub const SQL_EDITOR_MIN_ROWS: usize = 8;
/// The history drop-down's row, the error box's height and the characters of a history entry.
pub const SQL_HISTORY_H: f32 = 28.0;
pub const SQL_ERROR_H: f32 = 96.0;
pub const SQL_HISTORY_CHARS: usize = 44;
pub const SQL_HISTORY_POPUP_H: f32 = 420.0;
// chart
pub const CHART_H: f32 = 175.0;
pub const CHART_TOOLTIP_W: f32 = 260.0;
pub const CHART_GALLERY_W: f32 = 1040.0;
pub const CHART_CARD_W: f32 = 232.0;
pub const CHART_CARD_H: f32 = 196.0;
/// A banking template card: name, tool, form and the use / avoid lines (no thumbnail).
pub const CHART_TEMPLATE_CARD_H: f32 = 268.0;
pub const CHART_THUMB_H: f32 = 70.0;
pub const CHART_TEXT_FIELD_W: f32 = 150.0;
pub const CHART_REF_LABEL_W: f32 = 90.0;
pub const CHART_CONTROLS_W: f32 = 330.0;
pub const CHART_WIDE_MIN_W: f32 = 720.0;
pub const CHART_MIN_W: f32 = 200.0;
/// Seconds between two looks of a chart tab at its source tab (Phase 8 W5).
pub const CHART_TAB_LINK_CHECK_S: f64 = 0.4;
/// How faint a chart tab is drawn while its source tab is closed (Phase 8 W5).
pub const CHART_TAB_DIMMED_OPACITY: f32 = 0.45;
pub const CHART_MIN_H: f32 = 175.0;
pub const CHART_MAX_H: f32 = 520.0;
pub const CHART_BAR_W: f64 = 0.65;
/// The chart's controls and marks.
pub const CHART_COMBO_W: f32 = 130.0;
/// A colour swatch of the picker, and the hex box beside it (Phase 8 W7).
pub const SWATCH: f32 = 24.0;
pub const SWATCH_HEX_W: f32 = 100.0;
/// The swatch in the studio's Colours list (a little lower than wide), and the name box of House colours.
pub const SWATCH_LIST_H: f32 = 17.0;
pub const SWATCH_NAME_W: f32 = 160.0;
/// The list of a chart field picker: tall enough for the columns and the parts of a date column (W2).
pub const CHART_PICKER_POPUP_H: f32 = 460.0;
pub const CHART_COMBO_NARROW_W: f32 = 90.0;
/// Longest category name on the x-axis, and room for one.
pub const CHART_LABEL_CHARS: usize = 14;
pub const CHART_LABEL_W: f32 = 90.0;
/// The width a group of bars takes of its category's 1.0, and how much of its slot a bar fills.
pub const CHART_GROUP_W: f64 = 0.8;
pub const CHART_BAR_FILL: f64 = 0.92;
pub const CHART_LINE_W: f32 = 2.0;
/// Room under the plot for the x-axis names (the render tests look for them there).
pub const CHART_AXIS_BAND: f32 = 28.0;
pub const CHART_POINT_R: f32 = 3.0;
// command palette
pub const PALETTE_W: f32 = 560.0;
pub const PALETTE_TOP: f32 = 90.0;
pub const PALETTE_LIST_MAX_H: f32 = 380.0;
pub const PALETTE_SHADOW_ALPHA: u8 = 60;
pub fn palette_shadow() -> egui::Shadow {
    egui::Shadow { offset: egui::vec2(0.0, 8.0), blur: 24.0, spread: 0.0, color: Color32::from_black_alpha(PALETTE_SHADOW_ALPHA) }
}
// paddings
pub const MARGIN_CENTRAL: egui::Margin = egui::Margin { left: 16.0, right: 16.0, top: 14.0, bottom: 8.0 };
pub const MARGIN_SEARCH_BOX: egui::Margin = egui::Margin::symmetric(10.0, 6.0);
pub const MARGIN_KEYCAP: egui::Margin = egui::Margin::symmetric(5.0, 1.0);
pub const MARGIN_STATUS_BAR: egui::Margin = egui::Margin::symmetric(14.0, 0.0);
pub const STATUS_ITEM_GAP: f32 = 14.0;
/// The longest a long item of the status bar (a note, a chip) may be, as a share of the bar.
pub const STATUS_LABEL_SHARE: f32 = 0.3;
/// The last message is not drawn at all when less than this is left for it.
pub const STATUS_MIN_MESSAGE_W: f32 = 40.0;
pub const STATUS_BUTTON_SLACK: f32 = 4.0;
/// The last message keeps at least this much room while an optional item can be dropped.
pub const STATUS_MESSAGE_ROOM: f32 = 160.0;
/// Allowance for the spinner and the buttons' padding when the job line is measured.
pub const STATUS_JOB_SLACK: f32 = 48.0;
/// What a running job's progress bar, time and Cancel button take at the end of the bar.
pub const STATUS_JOB_RESERVE: f32 = 340.0;
pub const MARGIN_CHIP_ZERO: egui::Margin = egui::Margin::symmetric(6.0, 1.0);
pub const MARGIN_CHIP_PILL: egui::Margin = egui::Margin::symmetric(8.0, 1.0);
pub const MARGIN_EDIT_BANNER: egui::Margin = egui::Margin::symmetric(12.0, 6.0);
pub const EDIT_BANNER_GAP: f32 = 6.0;
pub const MARGIN_PALETTE_ROW: egui::Margin = egui::Margin::symmetric(8.0, 5.0);
pub const MARGIN_TAB_STRIP: egui::Margin = egui::Margin { left: 12.0, right: 12.0, top: 0.0, bottom: 0.0 };
pub fn margin_tab(active: bool) -> egui::Margin {
    egui::Margin::symmetric(14.0, if active { 7.0 } else { 5.0 })
}
// right panel
pub const MARGIN_SEGMENTED: egui::Margin = egui::Margin::same(3.0);
pub const MARGIN_SEGMENTED_OUTER: egui::Margin = egui::Margin { left: 14.0, right: 14.0, top: 12.0, bottom: 4.0 };
pub const SEGMENT_BTN_H: f32 = 30.0;
pub const PANEL_BTN_H: f32 = 32.0;
pub const WIDE_BTN_H: f32 = 34.0;
pub const MARGIN_PANEL_STEPS: egui::Margin = egui::Margin::symmetric(16.0, 8.0);
pub const MARGIN_PANEL_COLUMN: egui::Margin = egui::Margin::symmetric(16.0, 14.0);
pub const TEXT_COUNT: f32 = 11.0;
/// What a blank (null) cell of the table shows, in the muted colour. Exports are not affected.
pub const BLANK_CELL: &str = "\u{2013}";
pub const STEPS_FOOTER_RESERVE: f32 = 110.0;
/// The icon buttons of a selected step: edit, up, down, hide, duplicate, delete.
pub const STEP_BTN_W: f32 = 28.0;
pub const STEP_BTN_H: f32 = 24.0;
pub const STEPS_LIST_MIN_H: f32 = 100.0;
pub const STEP_LIST_INDENT: f32 = 26.0;
pub const MARGIN_STEP_ROW: egui::Margin = egui::Margin { left: 8.0, right: 8.0, top: 3.0, bottom: 8.0 };
pub const STEP_RAIL_X: f32 = 18.0;
pub const STEP_DOT_Y: f32 = 12.0;
pub const STEP_DOT_R: f32 = 6.0;
pub const MARGIN_TILE: egui::Margin = egui::Margin::same(10.0);
pub const TILE_W: f32 = 118.0;
pub const HISTOGRAM_H: f32 = 70.0;
/// The most months the Column inspector draws for a date column (the latest).
pub const INSPECTOR_MAX_MONTHS: usize = 36;
pub const HISTOGRAM_BAR_ALPHA: f32 = 0.8;
pub const HISTOGRAM_BAR_GAP: f32 = 2.0;
pub const DISABLED_OPACITY: f32 = 0.45;
// table: selector row, summary band, row-number column, minimum column width
pub const SELECTOR_INSET: egui::Vec2 = egui::vec2(8.0, 4.0);
pub const SELECTOR_BTN_PAD: egui::Vec2 = egui::vec2(8.0, 1.0);
pub const BAND_INSET: egui::Vec2 = egui::vec2(12.0, 0.0);
pub const ROW_NO_PAD: f32 = 24.0;
pub const ROW_NO_COL_W: f32 = 48.0;
pub const ROW_NO_MIN_W: f32 = 40.0;
pub const COLUMN_INITIAL_MIN_W: f32 = 96.0;

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
/// The fill of an upgrade (green) and of a downgrade (amber) in the CL migration matrix.
pub fn migration_fill(tk: &Tokens, shade: micro_parquet::banking::Shade) -> Option<Color32> {
    match shade {
        micro_parquet::banking::Shade::Upgrade => Some(tk.success_soft),
        micro_parquet::banking::Shade::Downgrade => Some(tk.warning.gamma_multiply(SHADE_AMBER_STRENGTH)),
        micro_parquet::banking::Shade::Breach => Some(tk.danger.gamma_multiply(SHADE_DANGER_STRENGTH)),
        micro_parquet::banking::Shade::None => None,
    }
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
    // Ctrl+= / Ctrl+- / Ctrl+0 are the app's own (View > Zoom): egui's would zoom a second time
    ctx.options_mut(|o| o.zoom_with_keyboard = false);
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

/// The width inside a card drawn in `ui`: what is free less the card's padding and border, so the
/// card itself is exactly as wide as the space it is given.
pub fn card_inner_width(ui: &egui::Ui) -> f32 {
    (ui.available_width() - 2.0 * SPACE_M - 2.0).max(0.0)
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
    let inner_w = card_inner_width(ui);
    egui::Frame::none()
        .fill(tk.surface)
        .stroke(Stroke::new(1.0_f32, tk.border))
        .rounding(Rounding::same(RADIUS_PANEL))
        .inner_margin(egui::Margin::same(SPACE_M))
        .show(ui, |ui| {
            ui.set_width(inner_w);
            ui.horizontal(|ui| {
                ui.label(semibold(heading).color(tk.ink));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), right);
            });
            add(ui);
        })
        .response
        .rect
}

/// A card of a fixed outer height (its content scrolls or is cut inside it).
pub fn card_height(ui: &mut egui::Ui, heading: &str, height: f32, add: impl FnOnce(&mut egui::Ui)) -> egui::Rect {
    let tk = *t(ui);
    let inner_w = card_inner_width(ui);
    egui::Frame::none()
        .fill(tk.surface)
        .stroke(Stroke::new(1.0_f32, tk.border))
        .rounding(Rounding::same(RADIUS_PANEL))
        .inner_margin(egui::Margin::same(SPACE_M))
        .show(ui, |ui| {
            ui.set_width(inner_w);
            let inner = (height - 2.0 * SPACE_M - 2.0).max(0.0);
            ui.set_min_height(inner);
            ui.set_max_height(inner);
            ui.label(semibold(heading).color(tk.ink));
            ui.add_space(SPACE_XS);
            add(ui);
        })
        .response
        .rect
}

/// A text in a row of options, as tall as the row's buttons and boxes, so a wrapped row keeps
/// everything on one baseline.
pub fn row_label(ui: &mut egui::Ui, text: RichText) -> egui::Response {
    let h = ui.spacing().interact_size.y;
    let galley = egui::WidgetText::from(text).into_galley(ui, Some(egui::TextWrapMode::Extend), f32::INFINITY, egui::TextStyle::Body);
    let w = galley.size().x;
    ui.add_sized([w + 1.0, h], egui::Label::new(egui::WidgetText::Galley(galley)))
}

/// Width of a text at a font size in the body font.
pub fn text_width(ui: &egui::Ui, text: &str, size: f32) -> f32 {
    ui.fonts(|f| f.layout_no_wrap(text.to_owned(), FontId::new(size, FontFamily::Proportional), Color32::WHITE).size().x)
}

/// Width of a small button in the status bar.
pub fn status_button_width(ui: &egui::Ui, label: &str) -> f32 {
    text_width(ui, label, TEXT_SMALL) + 2.0 * ui.spacing().button_padding.x + STATUS_BUTTON_SLACK
}

/// A label cut with "…" when it is wider than `max_w`; hover for the whole text is for the caller.
pub fn clipped_label(ui: &mut egui::Ui, text: RichText, max_w: f32) -> egui::Response {
    let galley = egui::WidgetText::from(text.clone()).into_galley(ui, Some(egui::TextWrapMode::Extend), f32::INFINITY, egui::TextStyle::Small);
    if galley.size().x <= max_w {
        return ui.label(text);
    }
    let h = galley.size().y;
    ui.allocate_ui_with_layout(egui::vec2(max_w.max(0.0), h), egui::Layout::left_to_right(egui::Align::Center), |ui| {
        ui.add(egui::Label::new(text).truncate())
    })
    .inner
}

/// A drop-down in a wrapped row: egui's combo box lays itself out in one line and so never
/// wraps; giving it a box of its own width lets the row wrap before it.
pub fn sized_combo(ui: &mut egui::Ui, width: f32, add: impl FnOnce(&mut egui::Ui)) {
    let h = ui.spacing().interact_size.y;
    ui.allocate_ui_with_layout(egui::vec2(width + SPACE_XS, h), egui::Layout::left_to_right(egui::Align::Center), add);
}

/// A muted dot between the items of a row of options.
pub fn dot(ui: &mut egui::Ui) {
    let muted = t(ui).muted;
    row_label(ui, RichText::new("\u{b7}").color(muted));
}

/// An "i" icon whose hover text explains something at length.
pub fn info_icon(ui: &mut egui::Ui, text: &str) {
    ui.label(RichText::new(ph::INFO).color(t(ui).muted)).on_hover_text(text);
}

/// Fills a heading cell's area with `header_bg` (a little wider than the cell, so the gap
/// between two cells is filled too). Nothing is painted outside `limit`, the clip rectangle of the
/// table the cell belongs to (the fill must never reach past the table's frame).
pub fn paint_header_bg(ui: &mut egui::Ui, rect: egui::Rect, color: Color32, limit: egui::Rect) {
    let gap = ui.spacing().item_spacing.x / 2.0;
    let wide = rect.expand2(egui::vec2(gap, 0.0));
    // a cell clips to itself; the fill goes into the gap, so widen the clip for this cell, but
    // only as far as the table's own clip rectangle
    let clip = ui.clip_rect().union(wide).intersect(limit);
    ui.set_clip_rect(clip);
    ui.painter().rect_filled(wide.intersect(limit), 0.0, color);
}

/// The text of a heading cell in a list: bold ink on `header_bg`.
pub fn header_cell(ui: &mut egui::Ui, text: &str, limit: egui::Rect) {
    let tk = *t(ui);
    paint_header_bg(ui, ui.max_rect(), tk.header_bg, limit);
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

/// Space kept free between a dialog and the edge of the area the panels leave.
pub const DIALOG_EDGE: f32 = 16.0;
/// Room kept for the fixed footer until it has been measured (separator and one row of buttons).
pub const DIALOG_FOOTER_H: f32 = 56.0;
/// A dialog never gets a scrolling middle shorter than this, however small the window.
pub const DIALOG_BODY_MIN_H: f32 = 80.0;
/// A dialog without a set width is about this wide.
pub const DIALOG_DEFAULT_W: f32 = 520.0;

/// The area dialogs may use: what is left of the window after the top bar, tab strip and status
/// bar (a modal dialog may cover the right panel), shrunk by `DIALOG_EDGE` on every side.
pub fn dialog_bounds(available: egui::Rect) -> egui::Rect {
    available.shrink(DIALOG_EDGE)
}

/// Remembers the area the panels leave (called by the frame once its panels are drawn; the
/// central panel then takes the rest, so dialogs cannot ask egui for it later).
pub fn set_dialog_area(ctx: &egui::Context, available: egui::Rect) {
    ctx.data_mut(|d| d.insert_temp(egui::Id::new("dialog_area"), available));
}

/// Remembers what is left after the right panel too: where the table and the filter bar live.
pub fn set_central_area(ctx: &egui::Context, available: egui::Rect) {
    ctx.data_mut(|d| d.insert_temp(egui::Id::new("central_area"), available));
}

/// The area of the central panel (the window less every panel).
pub fn central_area(ctx: &egui::Context) -> egui::Rect {
    ctx.data(|d| d.get_temp::<egui::Rect>(egui::Id::new("central_area"))).unwrap_or_else(|| ctx.screen_rect())
}

/// The area dialogs may use, or the whole window before the first frame has set it.
pub fn dialog_area(ctx: &egui::Context) -> egui::Rect {
    ctx.data(|d| d.get_temp::<egui::Rect>(egui::Id::new("dialog_area"))).filter(|r| r.width() > 1.0 && r.height() > 1.0).unwrap_or_else(|| ctx.screen_rect())
}

/// The heights of a dialog in an area of a given height.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DialogLayout {
    /// The tallest the whole dialog (title bar, middle, footer, frame) may be.
    pub max_total_h: f32,
    /// The tallest the content under the title bar may be (middle + footer).
    pub max_content_h: f32,
    /// The tallest the scrolling middle may be.
    pub body_max_h: f32,
}

/// What a dialog's frame and title bar add to its content, estimated from the title text height
/// (the frame has a margin above and below its content, egui puts one more margin pair between
/// the title bar and the content, and draws the title in the heading size). The real value is
/// measured after the dialog's first frame and replaces this.
pub fn dialog_overhead_estimate(title_h: f32) -> f32 {
    4.0 * SPACE_M + title_h + DIALOG_TITLE_SLACK
}
pub const DIALOG_TITLE_SLACK: f32 = 8.0;
/// Room egui keeps on each side of a window title for its buttons.
pub const DIALOG_TITLE_BUTTON_W: f32 = 28.0;

/// Pure layout of a dialog: `bounds_h` is the height of the area it may use (already shrunk by
/// `DIALOG_EDGE`), `overhead` what the frame and title bar add to the content, `footer_h` the
/// footer's measured height (its estimate before it has been drawn once).
pub fn dialog_layout(bounds_h: f32, overhead: f32, item_spacing_y: f32, footer_h: f32) -> DialogLayout {
    let max_total_h = bounds_h.max(overhead + footer_h + DIALOG_BODY_MIN_H);
    let max_content_h = max_total_h - overhead;
    let body_max_h = (max_content_h - footer_h - item_spacing_y).max(DIALOG_BODY_MIN_H);
    DialogLayout { max_total_h, max_content_h, body_max_h }
}

/// Where a dialog first opens: horizontally centred in `bounds`, at its top.
pub fn dialog_start_pos(bounds: egui::Rect, width: f32) -> egui::Pos2 {
    egui::pos2((bounds.center().x - width / 2.0).max(bounds.left()), bounds.top())
}

/// What a dialog drew in its last frame (tests read it).
#[derive(Clone, Copy, Debug)]
pub struct DialogReport {
    pub window: egui::Rect,
    /// The fixed buttons row under the title bar (`NOTHING` when the dialog has none).
    pub header: egui::Rect,
    /// The fixed footer (`NOTHING` when the dialog has none).
    pub footer: egui::Rect,
    pub body: egui::Rect,
    pub bounds: egui::Rect,
}

/// The dialogs drawn since the log was last cleared.
#[derive(Clone, Default)]
pub struct DialogLog(pub Vec<(String, DialogReport)>);

/// Forgets the logged dialogs (tests call it before the frame they look at).
pub fn clear_dialog_log(ctx: &egui::Context) {
    ctx.data_mut(|d| d.insert_temp(egui::Id::new("dialog_log"), DialogLog::default()));
}

/// The dialogs drawn since `clear_dialog_log`: (title, what they drew).
pub fn dialog_log(ctx: &egui::Context) -> Vec<(String, DialogReport)> {
    ctx.data(|d| d.get_temp::<DialogLog>(egui::Id::new("dialog_log"))).map(|l| l.0).unwrap_or_default()
}

/// The last frame's report of the dialog with this title.
pub fn dialog_report(ctx: &egui::Context, title: &str) -> Option<DialogReport> {
    ctx.data(|d| d.get_temp::<DialogReport>(egui::Id::new(("dialog_report", title))))
}

/// A dialog: the title bar (header) and the button footer stay in place, the middle scrolls.
/// Same title style everywhere; it fits in the area the panels leave, less 16 px on every side.
pub struct Dialog<'a> {
    title: String,
    width: f32,
    collapsible: bool,
    resizable: Option<bool>,
    open: Option<&'a mut bool>,
    min_width: f32,
    fill_height: bool,
}

/// The modal window every dialog uses.
pub fn modal<'a>(title: &str) -> Dialog<'a> {
    Dialog { title: title.to_string(), width: DIALOG_DEFAULT_W, collapsible: false, resizable: None, open: None, min_width: 0.0, fill_height: false }
}

impl<'a> Dialog<'a> {
    pub fn collapsible(mut self, v: bool) -> Self {
        self.collapsible = v;
        self
    }
    pub fn default_width(mut self, w: f32) -> Self {
        self.width = w;
        self
    }
    pub fn resizable(mut self, v: bool) -> Self {
        self.resizable = Some(v);
        self
    }
    /// The dialog opens as tall as the area allows (for a middle made with `body_scroll`, which
    /// always fills the height it is given: egui would otherwise open the window at its default
    /// height, as a resizable window never grows by itself).
    pub fn fill_height(mut self) -> Self {
        self.fill_height = true;
        self
    }
    /// The narrowest the dialog can be made (never more than the area allows).
    pub fn min_width(mut self, w: f32) -> Self {
        self.min_width = w;
        self
    }
    pub fn open(mut self, open: &'a mut bool) -> Self {
        self.open = Some(open);
        self
    }

    /// Shows the dialog. `add` calls `body` once for the scrolling middle and `footer` once for
    /// the buttons.
    pub fn show<R>(
        self,
        ctx: &egui::Context,
        add: impl FnOnce(&mut DialogUi<'_>) -> R,
    ) -> Option<egui::InnerResponse<Option<R>>> {
        let bounds = dialog_bounds(dialog_area(ctx));
        // (ctx.fonts holds the context's lock: read the style before, not inside)
        let spacing_y = ctx.style().spacing.item_spacing.y;
        let title_h = ctx.fonts(|f| {
            f.row_height(&FontId::new(TEXT_HEADING, family_semibold()))
                .max(f.row_height(&FontId::new(TEXT_BASE, family_semibold())))
        });
        // the height of what stays in place around the scrolling middle: the footer and the header
        let footer_id = egui::Id::new(("dialog_fixed_h", self.title.as_str()));
        let overhead_id = egui::Id::new(("dialog_overhead", self.title.as_str()));
        let footer_h = ctx.data(|d| d.get_temp::<f32>(footer_id)).unwrap_or(DIALOG_FOOTER_H);
        let overhead = ctx.data(|d| d.get_temp::<f32>(overhead_id)).unwrap_or_else(|| dialog_overhead_estimate(title_h));
        let lay = dialog_layout(bounds.height(), overhead, spacing_y, footer_h);
        // never wider than the area: wide content scrolls sideways inside the dialog
        // (egui's window width is the content's: the frame's two margins come on top)
        let max_inner_w = bounds.width() - 2.0 * SPACE_M;
        let width = self.width.min(max_inner_w);
        let title = self.title.clone();
        // A title longer than the dialog is cut with "…": egui makes a window at least as wide as
        // its title, which would push the dialog out of the area.
        let title_w = max_inner_w - 2.0 * DIALOG_TITLE_BUTTON_W;
        let mut job = egui::text::LayoutJob::single_section(
            self.title.clone(),
            egui::TextFormat { font_id: FontId::new(TEXT_HEADING, family_semibold()), color: Color32::PLACEHOLDER, ..Default::default() },
        );
        job.wrap = egui::text::TextWrapping { max_width: title_w.max(40.0), max_rows: 1, break_anywhere: true, overflow_character: Some('\u{2026}') };
        let galley = ctx.fonts(|f| f.layout_job(job));
        let mut window = egui::Window::new(egui::WidgetText::Galley(galley))
            .id(egui::Id::new(self.title.as_str()))
            .collapsible(self.collapsible);
        if let Some(r) = self.resizable {
            window = window.resizable(r);
        }
        if let Some(open) = self.open {
            window = window.open(open);
        }
        if self.fill_height {
            window = window.default_height(lay.max_content_h);
        }
        let out = window
            .default_width(width)
            .min_width(self.min_width.min(max_inner_w))
            .max_width(max_inner_w)
            .max_height(lay.max_content_h)
            .constrain_to(bounds)
            .default_pos(dialog_start_pos(bounds, width + 2.0 * SPACE_M))
            .show(ctx, |ui| {
                let mut d = DialogUi { ui, body_max_h: lay.body_max_h, footer: None, header: None, body: None };
                let r = add(&mut d);
                let (footer, header, body) = (d.footer, d.header, d.body);
                if footer.is_some() || header.is_some() {
                    let fixed = footer.map_or(0.0, |f| f.height()) + header.map_or(0.0, |h| h.height() + spacing_y);
                    ctx.data_mut(|data| data.insert_temp(footer_id, fixed));
                }
                (r, footer, header, body)
            });
        out.map(|o| {
            let (r, footer, header, body) = match o.inner {
                Some((r, f, h, b)) => (Some(r), f, h, b),
                None => (None, None, None, None),
            };
            let window = o.response.rect;
            // what the frame and title bar really add: the window less its content (the middle and
            // the footer; the header is counted with the fixed parts, not here)
            if let Some(b) = body {
                let last = footer.map_or(b.bottom(), |f| f.bottom());
                let first = header.map_or(b.top(), |h| h.top());
                let measured = window.height() - (last - first);
                if measured > 0.0 {
                    ctx.data_mut(|d| d.insert_temp(overhead_id, measured));
                }
            }
            let report = DialogReport { window, header: header.unwrap_or(egui::Rect::NOTHING), footer: footer.unwrap_or(egui::Rect::NOTHING), body: body.unwrap_or(egui::Rect::NOTHING), bounds };
            ctx.data_mut(|d| {
                d.insert_temp(egui::Id::new(("dialog_report", title.as_str())), report);
                let log = d.get_temp_mut_or_default::<DialogLog>(egui::Id::new("dialog_log"));
                log.0.retain(|(t, _)| t != &title);
                log.0.push((title.clone(), report));
            });
            egui::InnerResponse::new(r, o.response)
        })
    }
}

/// The inside of a dialog under construction.
pub struct DialogUi<'a> {
    pub ui: &'a mut egui::Ui,
    body_max_h: f32,
    footer: Option<egui::Rect>,
    header: Option<egui::Rect>,
    body: Option<egui::Rect>,
}

impl DialogUi<'_> {
    /// The scrolling middle, between the title bar and the footer: as tall as its content up to
    /// what the area leaves, with the vertical bar always shown. Content wider than the dialog
    /// scrolls sideways inside it; it never makes the dialog wider.
    pub fn body<R>(&mut self, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
        let top = self.ui.cursor().top();
        let out = egui::ScrollArea::vertical()
            .max_height(self.body_max_h)
            .auto_shrink([false, true])
            .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysVisible)
            .show(self.ui, |ui| {
                egui::ScrollArea::horizontal()
                    .id_salt("dialog_body_x")
                    .auto_shrink([false, true])
                    .show(ui, add)
                    .inner
            });
        self.body = Some(self.used_since(top));
        out.inner
    }

    /// The rectangle the content added since `top`.
    fn used_since(&self, top: f32) -> egui::Rect {
        let m = self.ui.min_rect();
        egui::Rect::from_min_max(egui::pos2(m.left(), top), egui::pos2(m.right(), m.bottom()))
    }

    /// The height the scrolling middle may have.
    pub fn body_max_h(&self) -> f32 {
        self.body_max_h
    }

    /// A middle that is always as tall as the dialog may be and scrolls as a whole (vertical bar
    /// always shown) when its content is taller; the content may use `body_max_h()` to fill what
    /// is left (the schema dialog's lists scroll on their own inside it).
    pub fn body_scroll<R>(&mut self, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
        let top = self.ui.cursor().top();
        let out = egui::ScrollArea::vertical()
            .max_height(self.body_max_h)
            .auto_shrink([false, false])
            .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysVisible)
            .show(self.ui, add);
        self.body = Some(self.used_since(top));
        out.inner
    }

    /// The fixed footer: a separator, then the buttons (which wrap to a second line when the
    /// dialog is narrow; the middle then gets less height).
    pub fn footer<R>(&mut self, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
        let top = self.ui.cursor().top();
        self.ui.separator();
        let r = add(self.ui);
        let bottom = self.ui.min_rect().bottom();
        self.footer = Some(egui::Rect::from_min_max(
            egui::pos2(self.ui.min_rect().left(), top),
            egui::pos2(self.ui.min_rect().right(), bottom),
        ));
        r
    }

    /// Content between the title bar and the scrolling middle that must stay in view (a row of
    /// buttons, a banner). Call it before `body`; it takes height from the middle.
    pub fn header<R>(&mut self, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
        let top = self.ui.cursor().top();
        let r = add(self.ui);
        let m = self.ui.min_rect();
        self.header = Some(egui::Rect::from_min_max(egui::pos2(m.left(), top), egui::pos2(m.right(), m.bottom())));
        r
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

    #[test]
    fn app_icon_is_a_256_square_with_transparent_corners() {
        let icon = super::app_icon();
        let side = super::APP_ICON_SIDE as usize;
        assert_eq!((icon.width, icon.height), (super::APP_ICON_SIDE, super::APP_ICON_SIDE));
        assert_eq!(icon.rgba.len(), side * side * 4);
        // the rounded tile leaves the corner transparent and fills the middle
        assert_eq!(icon.rgba[3], 0);
        let mid = (side / 2 * side + side / 2) * 4;
        assert_eq!(icon.rgba[mid + 3], 255);
    }
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

// ---- the dashboard view (Phase 10 D1)
/// The width of the pages list.
pub const DASH_LEFT_W: f32 = 168.0;
/// The width of the page and item properties.
pub const DASH_RIGHT_W: f32 = 216.0;
/// What a text box in the properties keeps clear of the panel's edge.
pub const DASH_PROPS_PAD: f32 = 24.0;
pub const DASH_LABEL_EDIT_W: f32 = 110.0;
pub const DASH_PER_ROW_W: f32 = 48.0;
/// The side of a drag handle of an item on a Free page.
pub const DASH_HANDLE: f32 = 10.0;
/// The smallest scale a page is drawn at.
pub const DASH_MIN_SCALE: f32 = 0.1;

// ---- KPI cards (Phase 10 D2)
pub const TEXT_KPI: f32 = 28.0;
pub const KPI_SPARK_W: f32 = 160.0;
pub const KPI_SPARK_H: f32 = 28.0;
pub const KPI_SPARK_DOT: f32 = 2.5;
/// The width the KPI settings keep for their labels.
pub const KPI_LABEL_W: f32 = 78.0;
