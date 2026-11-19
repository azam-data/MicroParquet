//! Phase 7 UI tests: the chart tokens in `theme.rs`, and (from C1) the headless render tests of the
//! chart renderer at 1219×686, 1600×900 and 1291×700 pt.

use crate::ui::theme;
use eframe::egui::Color32;

fn rgb(c: Color32) -> u32 {
    ((c.r() as u32) << 16) | ((c.g() as u32) << 8) | c.b() as u32
}

#[test]
fn c0_the_theme_tokens_carry_every_hex_value_of_the_design_system() {
    let l = theme::CHART_LIGHT;
    assert_eq!([l.surface, l.ink, l.ink2, l.muted, l.grid, l.baseline, l.deemph].map(rgb), [0xfcfcfb, 0x0b0b0b, 0x52514e, 0x898781, 0xe1e0d9, 0xc3c2b7, 0xc3c2b7]);
    let d = theme::CHART_DARK;
    assert_eq!([d.surface, d.ink, d.ink2, d.muted, d.grid, d.baseline, d.deemph].map(rgb), [0x1a1a19, 0xf0efec, 0xc3c2b7, 0x898781, 0x2c2c2a, 0x383835, 0x55544f]);
    assert_eq!(theme::CHART_SERIES_LIGHT.map(rgb), [0x2a78d6, 0xeb6834, 0x1baf7a, 0xeda100, 0xe87ba4, 0x008300, 0x6250d6, 0xe34948]);
    assert_eq!(theme::CHART_SERIES_DARK.map(rgb), [0x3987e5, 0xd95926, 0x199e70, 0xc98500, 0xd55181, 0x0ca30c, 0x9085e9, 0xe66767]);
    assert_eq!(
        theme::CHART_SEQUENTIAL.map(rgb),
        [0xcde2fb, 0xb7d3f6, 0x9ec5f4, 0x86b6ef, 0x6da7ec, 0x5598e7, 0x3987e5, 0x2a78d6, 0x256abf, 0x1c5cab, 0x184f95, 0x104281, 0x0d366b]
    );
    assert_eq!([theme::CHART_DIVERGING_POSITIVE, theme::CHART_DIVERGING_NEGATIVE, theme::CHART_DIVERGING_ZERO].map(rgb), [0x2a78d6, 0xe34948, 0xc3c2b7]);
    assert_eq!([theme::CHART_STATUS_GOOD, theme::CHART_STATUS_WARNING, theme::CHART_STATUS_SERIOUS, theme::CHART_STATUS_CRITICAL].map(rgb), [0x0ca30c, 0xfab219, 0xec835a, 0xd03b3b]);
    // the sizes
    assert_eq!((theme::CHART_TITLE_PT, theme::CHART_SUBTITLE_PT, theme::CHART_TICK_PT, theme::CHART_FOOTNOTE_PT), (15.0, 11.0, 10.0, 9.0));
    assert_eq!((theme::CHART_BAR_MAX_PX, theme::CHART_BAR_RADIUS_PX, theme::CHART_SEGMENT_GAP_PX, theme::CHART_MIN_SPACE_PX), (24.0, 4.0, 2.0, 8.0));
    // a slot beyond the eight is the de-emphasised colour
    assert_eq!(theme::chart_series_colour(8, false), theme::CHART_LIGHT.deemph);
    assert_eq!(theme::chart_series_colour(1, true), theme::CHART_SERIES_DARK[1]);
}
