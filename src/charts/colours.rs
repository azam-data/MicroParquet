//! The colour map of a chart (`docs/phase7-spec.md`, "Colour follows the entity, never its rank").
//! The first time a category or series is seen it takes the next free slot of the eight categorical
//! colours, and the slot is stored in the `ChartSpec`. Filtering one out later never repaints the
//! others. A ninth name gets the de-emphasised "Other" colour; so does a series called "Other".

use super::tokens::{self, SLOTS};
use crate::chart::OTHER_LABEL;
use serde::{Deserialize, Serialize};

/// The colour of one name.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Colour {
    /// Slot 0..8 of the categorical set (blue, orange, aqua, yellow, magenta, green, violet, red).
    Slot(usize),
    /// The de-emphasised colour.
    Other,
    /// One of the two greys of older curves: 0 is the darker (more recent), 1 the fainter (older).
    Grey(u8),
    /// A colour picked by hand (Phase 8 W7: the Light and Deep shades, or a hex): the same in both themes.
    Fixed(u32),
}

/// The stored slot numbers of the two greys (a slot of `SLOTS` to 99 is "Other", as before).
const GREY_BASE: usize = 100;

impl Colour {
    /// `0xRRGGBB` in the light or the dark set.
    pub fn hex(&self, dark: bool) -> u32 {
        match self {
            Colour::Slot(i) => tokens::series_colour(*i, dark).unwrap_or_else(|| tokens::ink(dark).deemph),
            Colour::Other => tokens::ink(dark).deemph,
            // the older curves: muted ink, then the de-emphasised grey (nearest the surface in each theme)
            Colour::Grey(0) => tokens::ink(dark).muted,
            Colour::Grey(_) => tokens::ink(dark).deemph,
            Colour::Fixed(rgb) => *rgb & 0xFF_FFFF,
        }
    }
}

/// Name to slot, in the order the names were first seen. A stored slot of `SLOTS` or more means "Other".
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq, Eq)]
#[serde(default)]
pub struct ColourMap {
    pub slots: Vec<(String, usize)>,
    /// The colours picked by hand for single items (Phase 8 W7), by name. They come before the house colours and the slots,
    /// and a filter or Top N never changes them. Left out of the file when empty, so older files are written as they were.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub custom: Vec<(String, ColourPick)>,
}

impl ColourMap {
    /// The colour of a name already seen; `None` for a new name (use `assign`).
    pub fn get(&self, name: &str) -> Option<Colour> {
        if name == OTHER_LABEL {
            return Some(Colour::Other);
        }
        // 1. the chart's own pick, 2. the house colour, 3. the automatic slot
        if let Some((_, p)) = self.custom.iter().find(|(n, _)| n == name) {
            return Some(p.colour());
        }
        if let Some(p) = house::lookup(name) {
            return Some(p.colour());
        }
        self.slots.iter().find(|(n, _)| n == name).map(|(_, s)| {
            if *s < SLOTS {
                Colour::Slot(*s)
            } else if *s >= GREY_BASE {
                Colour::Grey((*s - GREY_BASE).min(1) as u8)
            } else {
                Colour::Other
            }
        })
    }

    /// The colour a name was picked or given a house colour: never an automatic slot.
    pub fn picked(&self, name: &str) -> Option<Colour> {
        self.pick_of(name).copied().or_else(|| house::lookup(name)).map(|p| p.colour())
    }

    /// The chart's own pick for a name, if it has one.
    pub fn pick_of(&self, name: &str) -> Option<&ColourPick> {
        self.custom.iter().find(|(n, _)| n == name).map(|(_, p)| p)
    }

    /// Picks a colour for a name; an earlier pick of that name is replaced.
    pub fn set_pick(&mut self, name: &str, pick: ColourPick) {
        self.custom.retain(|(n, _)| n != name);
        self.custom.push((name.to_string(), pick));
    }

    /// Takes the pick of a name back to automatic (or to its house colour).
    pub fn clear_pick(&mut self, name: &str) {
        self.custom.retain(|(n, _)| n != name);
    }

    /// Stores a grey for a name (step 0 or 1); an existing entry is replaced.
    pub fn set_grey(&mut self, name: &str, step: u8) {
        self.slots.retain(|(n, _)| n != name);
        self.slots.push((name.to_string(), GREY_BASE + step.min(1) as usize));
    }

    /// The colour of a name that is not the highlighted one: its grey when it has one, else the common de-emphasised colour.
    pub fn dimmed(&self, name: &str, dark: bool) -> u32 {
        match self.get(name) {
            Some(c @ Colour::Grey(_)) => c.hex(dark),
            _ => tokens::ink(dark).deemph,
        }
    }

    /// Whether the name has one of the greys (an older curve: no markers).
    pub fn is_grey(&self, name: &str) -> bool {
        matches!(self.get(name), Some(Colour::Grey(_)))
    }

    /// The colour of a name; a new name takes the next free slot (and is stored), or "Other" once the eight are taken.
    pub fn assign(&mut self, name: &str) -> Colour {
        if let Some(c) = self.get(name) {
            return c;
        }
        // the next free slot: the lowest one no stored name holds (a slot is never given to a second name)
        let slot = (0..SLOTS).find(|s| !self.slots.iter().any(|(_, u)| u == s)).unwrap_or(SLOTS);
        self.slots.push((name.to_string(), slot));
        if slot < SLOTS {
            Colour::Slot(slot)
        } else {
            Colour::Other
        }
    }

    /// Gives every name a colour, in the order of the list (names already seen keep theirs).
    pub fn assign_all<S: AsRef<str>>(&mut self, names: &[S]) {
        for n in names {
            self.assign(n.as_ref());
        }
    }
}

// ------------------------------------------------------------------------------------ picks (Phase 8 W7)

/// A colour typed or picked as "#rrggbb" (stored as that text, so a file can be read and edited by hand).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HexColour(pub u32);

impl HexColour {
    /// "#rrggbb" (the # is optional, any case). Anything else is refused in words.
    pub fn parse(text: &str) -> Result<HexColour, String> {
        let t = text.trim().trim_start_matches('#');
        if t.len() != 6 || !t.chars().all(|c| c.is_ascii_hexdigit()) {
            return Err(format!("'{}' is not a colour. Write it as #rrggbb, for example #037952.", text.trim()));
        }
        u32::from_str_radix(t, 16).map(HexColour).map_err(|_| format!("'{}' is not a colour. Write it as #rrggbb, for example #037952.", text.trim()))
    }

    pub fn text(&self) -> String {
        tokens::hex_text(self.0)
    }
}

impl Serialize for HexColour {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&self.text())
    }
}

impl<'de> Deserialize<'de> for HexColour {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let t = String::deserialize(d)?;
        HexColour::parse(&t).map_err(serde::de::Error::custom)
    }
}

/// How the colour of one item was chosen (Phase 8 W7).
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ColourPick {
    /// The Base shade of a hue: pinned to a palette slot (0..8), so it keeps its validated light and dark pair.
    Slot { slot: usize },
    /// A fixed colour, the same in both themes: the Light or Deep shade of a hue, or any hex.
    Hex { hex: HexColour },
}

impl ColourPick {
    pub fn colour(&self) -> Colour {
        match self {
            ColourPick::Slot { slot } if *slot < SLOTS => Colour::Slot(*slot),
            ColourPick::Slot { .. } => Colour::Other,
            ColourPick::Hex { hex } => Colour::Fixed(hex.0),
        }
    }

    /// A Base pick is the palette itself: it is checked for being hard to tell from another item, not for contrast or status.
    pub fn is_base(&self) -> bool {
        matches!(self, ColourPick::Slot { .. })
    }

    /// The pick for the swatch of a hue (0..8) in a shade: 0 Light, 1 Base, 2 Deep.
    pub fn swatch(hue: usize, shade: usize) -> ColourPick {
        let h = hue.min(SLOTS - 1);
        match shade {
            0 => ColourPick::Hex { hex: HexColour(tokens::HUE_LIGHT[h]) },
            1 => ColourPick::Slot { slot: h },
            _ => ColourPick::Hex { hex: HexColour(tokens::HUE_DEEP[h]) },
        }
    }
}

// --------------------------------------------------------------------------- house colours (Phase 8 W7)

/// `portable_data/house_colours.json`: a colour for a name that every chart, template, pack chart and chart book uses unless the
/// chart has its own pick. A missing file means none; a broken one is refused with its line and the charts use the palette.
pub mod house {
    use super::*;
    use std::cell::RefCell;
    use std::path::{Path, PathBuf};
    use std::time::{Duration, Instant, SystemTime};

    pub const FILE_NAME: &str = "house_colours.json";

    /// One house colour: `{ "name": "TD", "pick": { "kind": "slot", "slot": 2 } }` or `{ "kind": "hex", "hex": "#037952" }`.
    #[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
    pub struct HouseColour {
        pub name: String,
        pub pick: ColourPick,
    }

    /// The file this thread reads (`portable_data` or the folder a test put in its place).
    pub fn path() -> PathBuf {
        crate::rules::data_dir().join(FILE_NAME)
    }

    /// Names are matched trimmed and case-insensitive.
    fn key(name: &str) -> String {
        name.trim().to_lowercase()
    }

    /// Reads a house colours file. A missing file is no colours; a broken one is an error naming the line.
    pub fn load_from(path: &Path) -> Result<Vec<HouseColour>, String> {
        let text = match std::fs::read_to_string(path) {
            Ok(t) => t,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(e) => return Err(format!("The house colours file {} could not be read: {}.", path.display(), e)),
        };
        if text.trim().is_empty() {
            return Ok(Vec::new());
        }
        let list: Vec<HouseColour> = serde_json::from_str(&text).map_err(|e| format!("The house colours file {} is broken at line {}: {}. The charts use the palette until it is fixed.", path.display(), e.line(), e))?;
        let mut seen: Vec<String> = Vec::new();
        for (i, c) in list.iter().enumerate() {
            if c.name.trim().is_empty() {
                return Err(format!("The house colours file {} has a colour without a name (number {}).", path.display(), i + 1));
            }
            if let ColourPick::Slot { slot } = c.pick {
                if slot >= SLOTS {
                    return Err(format!("The house colours file {}: '{}' is pinned to slot {}, and there are only {} slots.", path.display(), c.name, slot + 1, SLOTS));
                }
            }
            if seen.contains(&key(&c.name)) {
                return Err(format!("The house colours file {} lists '{}' twice.", path.display(), c.name.trim()));
            }
            seen.push(key(&c.name));
        }
        Ok(list)
    }

    /// Writes the list (a temp file, then a rename), and makes this thread read it at once.
    pub fn save_to(path: &Path, list: &[HouseColour]) -> Result<(), String> {
        let text = serde_json::to_string_pretty(list).map_err(|e| format!("The house colours could not be written: {e}."))?;
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| format!("The folder {} could not be made: {}.", dir.display(), e))?;
        }
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, text).map_err(|e| format!("The house colours could not be written to {}: {}.", tmp.display(), e))?;
        std::fs::rename(&tmp, path).map_err(|e| format!("The house colours could not be saved to {}: {}.", path.display(), e))?;
        invalidate();
        Ok(())
    }

    struct Cache {
        path: Option<PathBuf>,
        stamp: Option<(SystemTime, u64)>,
        checked: Option<Instant>,
        list: Vec<HouseColour>,
        error: Option<String>,
    }

    thread_local! {
        static CACHE: RefCell<Cache> = const { RefCell::new(Cache { path: None, stamp: None, checked: None, list: Vec::new(), error: None }) };
    }

    /// How often a thread looks at the file's size and time again.
    const RECHECK: Duration = Duration::from_millis(500);

    fn stamp_of(path: &Path) -> Option<(SystemTime, u64)> {
        let m = std::fs::metadata(path).ok()?;
        Some((m.modified().ok()?, m.len()))
    }

    /// Forgets what this thread read: the next lookup reads the file again (after a save, or a change of the data folder).
    pub fn invalidate() {
        CACHE.with(|c| {
            let mut c = c.borrow_mut();
            c.path = None;
            c.checked = None;
        });
    }

    fn refreshed<R>(f: impl FnOnce(&Cache) -> R) -> R {
        CACHE.with(|c| {
            let mut c = c.borrow_mut();
            let due = c.path.is_none() || c.checked.map_or(true, |t| t.elapsed() >= RECHECK);
            if due {
                let path = c.path.clone().unwrap_or_else(path_now);
                let stamp = stamp_of(&path);
                if c.path.as_ref() != Some(&path) || c.stamp != stamp {
                    match load_from(&path) {
                        Ok(l) => {
                            c.list = l;
                            c.error = None;
                        }
                        Err(e) => {
                            c.list = Vec::new();
                            c.error = Some(e);
                        }
                    }
                    c.stamp = stamp;
                    c.path = Some(path);
                }
                c.checked = Some(Instant::now());
            }
            f(&c)
        })
    }

    fn path_now() -> PathBuf {
        path()
    }

    /// The house colour of a name (trimmed, case-insensitive), if there is one.
    pub fn lookup(name: &str) -> Option<ColourPick> {
        let k = key(name);
        refreshed(|c| c.list.iter().find(|h| key(&h.name) == k).map(|h| h.pick))
    }

    /// The house colours this thread reads now (none when the file is missing or broken).
    pub fn list() -> Vec<HouseColour> {
        refreshed(|c| c.list.clone())
    }

    /// Why the file is not used, when it is broken (shown at start-up).
    pub fn error() -> Option<String> {
        refreshed(|c| c.error.clone())
    }

    /// Adds or replaces the house colour of a name and saves the file.
    pub fn set(name: &str, pick: ColourPick) -> Result<(), String> {
        let mut l = load_from(&path())?;
        l.retain(|h| key(&h.name) != key(name));
        l.push(HouseColour { name: name.trim().to_string(), pick });
        save_to(&path(), &l)
    }

    /// Removes the house colour of a name and saves the file.
    pub fn remove(name: &str) -> Result<(), String> {
        let mut l = load_from(&path())?;
        l.retain(|h| key(&h.name) != key(name));
        save_to(&path(), &l)
    }
}

// ------------------------------------------------------------------------------- warnings (Phase 8 W7)

/// The linear value of an sRGB channel (0..255).
fn linear(c: u32) -> f64 {
    let c = c as f64 / 255.0;
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

/// `0xRRGGBB` in OKLab (L, a, b).
fn oklab(rgb: u32) -> (f64, f64, f64) {
    let (r, g, b) = (linear((rgb >> 16) & 255), linear((rgb >> 8) & 255), linear(rgb & 255));
    let l = (0.4122214708 * r + 0.5363325363 * g + 0.0514459929 * b).cbrt();
    let m = (0.2119034982 * r + 0.6806995451 * g + 0.1073969566 * b).cbrt();
    let s = (0.0883024619 * r + 0.2817188376 * g + 0.6299787005 * b).cbrt();
    (0.2104542553 * l + 0.7936177850 * m - 0.0040720468 * s, 1.9779984951 * l - 2.4285922050 * m + 0.4505937099 * s, 0.0259040371 * l + 0.7827717662 * m - 0.8086757660 * s)
}

/// The distance of two colours in OKLab, times 100 ("ΔE×100": 10 and above is easy to tell apart, below is not).
pub fn delta_e100(a: u32, b: u32) -> f64 {
    let (x, y) = (oklab(a), oklab(b));
    100.0 * ((x.0 - y.0).powi(2) + (x.1 - y.1).powi(2) + (x.2 - y.2).powi(2)).sqrt()
}

fn luminance(rgb: u32) -> f64 {
    0.2126 * linear((rgb >> 16) & 255) + 0.7152 * linear((rgb >> 8) & 255) + 0.0722 * linear(rgb & 255)
}

/// The contrast ratio of two colours (1 to 21).
pub fn contrast_ratio(a: u32, b: u32) -> f64 {
    let (la, lb) = (luminance(a), luminance(b));
    let (hi, lo) = if la >= lb { (la, lb) } else { (lb, la) };
    (hi + 0.05) / (lo + 0.05)
}

/// Below this ΔE×100 two items of a chart are "hard to tell" apart.
pub const HARD_TO_TELL_BELOW: f64 = 10.0;
/// Below this contrast ratio a colour is "low contrast" on the background of a theme.
pub const LOW_CONTRAST_BELOW: f64 = 3.0;
/// Within this ΔE×100 of a status colour, a colour reads as that status.
pub const STATUS_WITHIN: f64 = 5.0;

/// What is wrong with colouring `name` with `pick` in a chart whose other items are `others` (name and colour): the warnings
/// the picker shows under it and the chart prints in its notes. A warning is not a refusal, the pick is the owner's call.
/// A Base pick is the palette itself, so it is checked only for being hard to tell from another item.
pub fn pick_warnings(pick: &ColourPick, others: &[(String, Colour)]) -> Vec<String> {
    let mine = pick.colour();
    let mut out: Vec<String> = Vec::new();
    // hard to tell from another item, in either theme
    let mut worst: Option<(f64, &str)> = None;
    for dark in [false, true] {
        for (n, c) in others {
            let d = delta_e100(mine.hex(dark), c.hex(dark));
            if d < HARD_TO_TELL_BELOW && worst.map_or(true, |(w, _)| d < w) {
                worst = Some((d, n.as_str()));
            }
        }
    }
    if let Some((_, n)) = worst {
        out.push(format!("Hard to tell from '{}'", n));
    }
    if pick.is_base() {
        return out;
    }
    for (dark, word) in [(true, "dark"), (false, "light")] {
        if contrast_ratio(mine.hex(dark), tokens::ink(dark).surface) < LOW_CONTRAST_BELOW {
            out.push(format!("Low contrast on the {word} background"));
        }
    }
    let rgb = mine.hex(false);
    if let Some((name, _)) = tokens::STATUS_COLOURS.iter().find(|(_, s)| delta_e100(rgb, *s) < STATUS_WITHIN) {
        out.push(format!("This is a status colour ({name}): status colours mean a state and shouldn't name an item"));
    }
    out
}

/// The name under which a chart of one series lists its one colour: its measure ("Sum of Balance"). `None` for a chart that has a
/// Series, several measures or panels, or a kind that colours by step or by value (a waterfall, a heatmap).
pub fn single_series_name(data: &super::data::ChartData) -> Option<String> {
    use super::spec::ChartKind as K;
    let one_colour = matches!(data.spec.kind, K::Bar | K::Line | K::Area | K::Scatter | K::Bubble | K::Histogram | K::BoxPlot);
    if !one_colour || data.series.iter().any(|s| !s.is_empty()) || data.measure_count() != 1 || !data.spec.panels.is_empty() {
        return None;
    }
    data.measure_names.first().filter(|n| !n.is_empty()).cloned()
}

/// The colour of the one series of a chart that has no Series: the pick (or house colour) on its measure's name, else the first slot.
pub fn single_colour(data: &super::data::ChartData, cm: &ColourMap) -> Colour {
    single_series_name(data).and_then(|n| cm.picked(&n)).unwrap_or(Colour::Slot(0))
}

/// The names a chart colours: its series, its categories when it has none and each category is a mark of its own colour (a
/// donut), or its one measure when it is a chart of one series ("Sum of Balance").
pub fn colour_names_of(data: &super::data::ChartData) -> Vec<String> {
    let series: Vec<String> = data.series.iter().filter(|s| !s.is_empty()).cloned().collect();
    if !series.is_empty() {
        series
    } else if data.spec.kind == super::spec::ChartKind::Donut {
        data.categories.clone()
    } else {
        single_series_name(data).into_iter().collect()
    }
}

/// The colour each name of a chart has now (a name without a stored colour has the slot of its place).
pub fn resolved(cm: &ColourMap, names: &[String]) -> Vec<(String, Colour)> {
    names.iter().enumerate().map(|(i, n)| (n.clone(), cm.get(n).unwrap_or(Colour::Slot(i.min(SLOTS - 1))))).collect()
}

/// The warnings of every item that has a pick of its own or a house colour, for the chart's notes: "Colour of 'TD': hard to tell
/// from 'Savings'". An item with the automatic colour has none.
pub fn colour_notes(data: &super::data::ChartData, cm: &ColourMap) -> Vec<String> {
    let names = colour_names_of(data);
    let all = resolved(cm, &names);
    let mut out = Vec::new();
    for (i, n) in names.iter().enumerate() {
        let pick = match cm.pick_of(n).copied().or_else(|| house::lookup(n)) {
            Some(p) => p,
            None => continue,
        };
        let others: Vec<(String, Colour)> = all.iter().enumerate().filter(|(j, _)| *j != i).map(|(_, x)| x.clone()).collect();
        for w in pick_warnings(&pick, &others) {
            out.push(format!("Colour of '{}': {}.", n, w[..1].to_lowercase() + &w[1..]));
        }
    }
    out
}
