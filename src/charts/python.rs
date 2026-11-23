//! "Copy as Python" (Phase 7 C7): a self-contained script for the chart on the clipboard. The chart's data
//! is a pandas DataFrame literal (the numbers as the chart shows them, in the chart's scale) and the
//! drawing is matplotlib in the house style (fonts, palette, marks, titles). A Sankey and a treemap
//! have no matplotlib form: they use plotly, and the script says so in a comment.

use super::axis::ValueKind;
use super::colours::ColourMap;
use super::data::{heatmap, waterfall_from_data, ChartData, StepKind};
use super::layout::{chart_title, scaled, value_kind, LayoutOpts};
use super::spec::{ChartKind, Stack, Status, Transform};
use super::tokens as t;
use anyhow::{bail, Result};
use std::fmt::Write;

/// A Python string literal.
fn s(text: &str) -> String {
    let mut out = String::from("\"");
    for c in text.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => {}
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// A Python number: the shortest text that reads back as the same value.
fn f(v: f64) -> String {
    if v.is_finite() {
        format!("{:?}", v)
    } else {
        "None".to_string()
    }
}

fn fo(v: Option<f64>) -> String {
    v.map(f).unwrap_or_else(|| "None".to_string())
}

fn list_s(v: &[String]) -> String {
    format!("[{}]", v.iter().map(|x| s(x)).collect::<Vec<_>>().join(", "))
}

fn list_f(v: &[Option<f64>]) -> String {
    format!("[{}]", v.iter().map(|x| fo(*x)).collect::<Vec<_>>().join(", "))
}

fn hexs(c: u32) -> String {
    s(&t::hex_text(c))
}

/// The house style and the drawing helpers every script starts with.
fn preamble(w: &mut String, dark: bool, plotly: bool, decimals: usize, labels: &str) {
    let ink = t::ink(dark);
    let _ = writeln!(w, "import math\nimport pandas as pd");
    if plotly {
        let _ = writeln!(w, "import plotly.graph_objects as go");
    } else {
        let _ = writeln!(w, "import matplotlib.pyplot as plt\nfrom matplotlib.patches import Patch");
    }
    let _ = writeln!(w, "\nDARK = {}  # True for the dark theme", if dark { "True" } else { "False" });
    let _ = writeln!(w, "SURFACE, INK, INK2, MUTED, GRID, BASELINE, DEEMPH = {}, {}, {}, {}, {}, {}, {}", hexs(ink.surface), hexs(ink.ink), hexs(ink.ink2), hexs(ink.muted), hexs(ink.grid), hexs(ink.baseline), hexs(ink.deemph));
    let light = t::ink(false);
    let dk = t::ink(true);
    let _ = writeln!(
        w,
        "if not DARK:\n    SURFACE, INK, INK2, MUTED, GRID, BASELINE, DEEMPH = {}, {}, {}, {}, {}, {}, {}\nelse:\n    SURFACE, INK, INK2, MUTED, GRID, BASELINE, DEEMPH = {}, {}, {}, {}, {}, {}, {}",
        hexs(light.surface),
        hexs(light.ink),
        hexs(light.ink2),
        hexs(light.muted),
        hexs(light.grid),
        hexs(light.baseline),
        hexs(light.deemph),
        hexs(dk.surface),
        hexs(dk.ink),
        hexs(dk.ink2),
        hexs(dk.muted),
        hexs(dk.grid),
        hexs(dk.baseline),
        hexs(dk.deemph)
    );
    let ser = |d: bool| format!("[{}]", (0..t::SLOTS).filter_map(|i| t::series_colour(i, d)).map(hexs).collect::<Vec<_>>().join(", "));
    let _ = writeln!(w, "SERIES = {} if not DARK else {}  # blue, orange, aqua, yellow, magenta, green, violet, red", ser(false), ser(true));
    let _ = writeln!(w, "FONT = [\"Source Sans 3\", \"Segoe UI\", \"DejaVu Sans\"]  # the first one installed is used");
    if !plotly {
        let _ = writeln!(
            w,
            r#"
plt.rcParams.update({{
    "font.family": "sans-serif", "font.sans-serif": FONT, "font.size": {tick},
    "figure.facecolor": SURFACE, "axes.facecolor": SURFACE, "savefig.facecolor": SURFACE,
    "text.color": INK, "axes.labelcolor": INK2, "xtick.color": MUTED, "ytick.color": MUTED,
    "axes.spines.top": False, "axes.spines.right": False, "axes.spines.left": False, "axes.edgecolor": BASELINE,
    "axes.grid": False, "grid.color": GRID, "grid.linewidth": 1, "grid.linestyle": "-",
    "xtick.major.size": 0, "ytick.major.size": 0, "legend.frameon": False,
}})

DEC = {dec}  # the decimals of the view the chart was made in

def fmt(v, kind="amount", decimals=None):
    """The number as the chart writes it: thousands separators, the true minus sign, a % for shares
    (one decimal for a share, the view's decimals for an amount, none for a count)."""
    if v is None or (isinstance(v, float) and math.isnan(v)):
        return ""
    if decimals is None:
        decimals = 1 if kind == "percent" else 0 if kind == "count" else DEC
    text = f"{{abs(v):,.{{decimals}}f}}" + ("%" if kind == "percent" else "")
    return ("−" if v < 0 else "") + text

{labels}

def style_axes(ax, horizontal=False):
    """One zero baseline, light solid gridlines on the value axis only, no frame."""
    ax.spines["bottom" if not horizontal else "left"].set_visible(True)
    ax.spines["bottom" if not horizontal else "left"].set_color(BASELINE)
    ax.grid(axis="x" if horizontal else "y")
    ax.set_axisbelow(True)

def titles(fig, title, subtitle, footnote):
    fig.text(0.015, 0.975, title, fontsize={title_pt}, fontweight="semibold", ha="left", va="top")
    if subtitle:
        fig.text(0.015, 0.915, subtitle, fontsize={sub_pt}, color=INK2, ha="left", va="top")
    if footnote:
        fig.text(0.015, 0.012, footnote, fontsize={foot_pt}, color=MUTED, ha="left", va="bottom")
"#,
            tick = t::tick_pt(),
            dec = decimals,
            labels = labels,
            title_pt = t::TITLE_PT,
            sub_pt = t::SUBTITLE_PT,
            foot_pt = t::FOOTNOTE_PT
        );
    }
}

fn colours_of(data: &ChartData, opts: &LayoutOpts) -> (ColourMap, Vec<String>) {
    let mut cm = opts.colours.clone().unwrap_or_else(|| data.spec.colour_map.clone());
    let named: Vec<String> = data.series.iter().filter(|x| !x.is_empty()).cloned().collect();
    cm.assign_all(&named);
    (cm, named)
}

fn colour_dict(data: &ChartData, cm: &ColourMap) -> String {
    let item = |n: &str, c: Option<super::colours::Colour>| {
        let l = c.map(|c| c.hex(false)).unwrap_or(t::SERIES_LIGHT[0]);
        let d = c.map(|c| c.hex(true)).unwrap_or(t::SERIES_DARK[0]);
        format!("{}: {} if not DARK else {}", s(n), hexs(l), hexs(d))
    };
    let mut items: Vec<String> = data.series.iter().filter(|n| !n.is_empty()).map(|n| item(n, cm.get(n))).collect();
    // a chart of one series: its pick is on its measure's name (the column of its frame); a scatter's one series has no name
    if let Some(m) = super::colours::single_series_name(data) {
        let one = Some(super::colours::single_colour(data, cm));
        items.push(item(&m, one));
        if matches!(data.spec.kind, ChartKind::Scatter | ChartKind::Bubble) {
            items.push(item("", one));
        }
    }
    format!("{{{}}}", items.join(", "))
}

/// The Python expression of the one colour of a histogram or box plot (the first slot unless it was picked).
fn one_colour_expr(data: &ChartData, cm: &ColourMap) -> String {
    let c = super::colours::single_colour(data, cm);
    if c == super::colours::Colour::Slot(0) {
        "SERIES[0]".to_string()
    } else {
        format!("{} if not DARK else {}", hexs(c.hex(false)), hexs(c.hex(true)))
    }
}

fn status_colour(st: Status) -> String {
    hexs(st.hex())
}

fn kind_word(vk: ValueKind) -> &'static str {
    match vk {
        ValueKind::Percent => "percent",
        _ => "amount",
    }
}

/// The values of one measure as the chart shows them (in the chart's scale).
fn shown(data: &ChartData, measure: usize, opts: &LayoutOpts) -> Vec<Vec<Option<f64>>> {
    let vk = value_kind(&data.spec, measure);
    data.measure(measure).map(|m| m.values.iter().map(|r| r.iter().map(|v| v.map(|x| scaled(x, vk, opts))).collect()).collect()).unwrap_or_default()
}

/// A DataFrame literal of one measure: the categories, then one column per series.
fn frame_literal(data: &ChartData, measure: usize, opts: &LayoutOpts, name: &str) -> String {
    let cat = if data.spec.category.is_empty() { "Category".to_string() } else { data.spec.category.clone() };
    let vals = shown(data, measure, opts);
    let mname = data.measure_names.get(measure).cloned().unwrap_or_else(|| "Value".to_string());
    let mut cols = vec![format!("    {}: {}", s(&cat), list_s(&data.categories))];
    for (i, ser) in data.series.iter().enumerate() {
        let head = if ser.is_empty() { mname.clone() } else { ser.clone() };
        cols.push(format!("    {}: {}", s(&head), list_f(vals.get(i).map(|v| v.as_slice()).unwrap_or(&[]))));
    }
    format!("{} = pd.DataFrame({{\n{}\n}}).set_index({})", name, cols.join(",\n"), s(&cat))
}

const DRAW: &str = r##"
def draw(ax, df, kind, stack="grouped", horizontal=False, colours=None, stepped=False, label_values=True, vkind="amount", net=False, segments=False, named_end=False):
    """One panel: bars (grouped, stacked, 100% stacked), lines (stepped), areas. `df` has the categories as its
    index and one column per series."""
    colours = colours or {}
    names = list(df.columns)
    pick = lambda i, n: colours.get(n, SERIES[min(i, len(SERIES) - 1)])
    pos = list(range(len(df.index)))
    if kind == "bar":
        data = df.copy()
        if stack == "percent" and len(names) > 1:
            data = data.div(data.abs().sum(axis=1), axis=0) * 100
        width = 0.55 if len(names) == 1 or stack != "grouped" else 0.7 / len(names)
        bottoms_pos = [0.0] * len(pos)
        bottoms_neg = [0.0] * len(pos)
        for i, n in enumerate(names):
            vals = [0.0 if pd.isna(v) else v for v in data[n]]
            if stack == "grouped":
                offs = [p + (i - (len(names) - 1) / 2) * width for p in pos]
                base = [0.0] * len(pos)
            else:
                offs, base = pos, [bottoms_pos[k] if v >= 0 else bottoms_neg[k] for k, v in enumerate(vals)]
            colour = [pick(i, n)] * len(pos)
            if len(names) == 1 and any(v < 0 for v in vals):
                colour = ["#e34948" if v < 0 else "#2a78d6" if v > 0 else "#c3c2b7" for v in vals]  # diverging: red down, blue up, grey at zero
            if horizontal:
                bars = ax.barh(offs, vals, height=width, left=base, color=colour, edgecolor=SURFACE, linewidth=2 if stack != "grouped" else 0, label=n)
            else:
                bars = ax.bar(offs, vals, width=width, bottom=base, color=colour, edgecolor=SURFACE, linewidth=2 if stack != "grouped" else 0, label=n)
            if stack != "grouped":
                for k, v in enumerate(vals):
                    if v >= 0:
                        bottoms_pos[k] += v
                    else:
                        bottoms_neg[k] += v
            if label_values and LABELS != "none" and ((LABELS == "auto" and len(names) == 1 and len(pos) <= 12) or LABELS in ("all", "ends")):
                signed = len(names) == 1 and any(v < 0 for v in vals)
                texts = [lab(v, vkind, signed) if show(k, len(vals)) and (i, k) not in DROPPED else "" for k, v in enumerate(vals)]
                ax.bar_label(bars, labels=texts, padding=3, fontsize=9.5, color=INK, label_type="center" if stack != "grouped" else "edge")
        if segments and stack == "stacked" and not horizontal and LABELS != "none":
            # each segment beside its stack with its value and its share of the stack
            totals = df.fillna(0).abs().sum(axis=1)
            for k in range(len(pos)):
                base = 0.0
                for n in names:
                    v = 0.0 if pd.isna(df[n].iloc[k]) else df[n].iloc[k]
                    share = 0 if totals.iloc[k] == 0 else abs(v) / totals.iloc[k] * 100
                    ax.annotate((lab(v, vkind) if LDEC is not None else fmt(v * (LABEL_SCALE if vkind == "amount" else 1), vkind, 0 if float(v * (LABEL_SCALE if vkind == "amount" else 1)).is_integer() else DEC)) + " \u00b7 " + f"{share:.1f}%", (k + width / 2, base + v / 2), xytext=(8, 0), textcoords="offset points", va="center", fontsize=9.5, color=INK)
                    base += v
        if net and stack == "stacked" and not horizontal and LABELS != "none":
            # the net of each stack: a short dark tick across it and its signed value beside it
            for k, total in enumerate(df.fillna(0).sum(axis=1)):
                ax.hlines(total, k - width / 2 - 0.08, k + width / 2 + 0.08, color=INK, linewidth=2, zorder=5)
                ax.annotate(lab(total, vkind, True), (k + width / 2 + 0.1, total), xytext=(4, 0), textcoords="offset points", va="center", fontsize=9.5, color=INK)
        if horizontal:
            ax.set_yticks(pos, [str(i) for i in df.index])
            ax.invert_yaxis()
            ax.axvline(0, color=BASELINE, linewidth=1)
        else:
            cat_labels(ax, df.index)
            ax.axhline(0, color=BASELINE, linewidth=1)
        style_axes(ax, horizontal)
    elif kind in ("line", "area"):
        x = pos
        base = [0.0] * len(pos)
        for i, n in enumerate(names):
            vals = [float("nan") if v is None else v for v in df[n]]
            colour = pick(i, n)
            drawstyle = "steps-post" if stepped else "default"
            if kind == "area" and stack in ("stacked", "percent") and len(names) > 1:
                top = [b + (0 if math.isnan(v) else v) for b, v in zip(base, vals)]
                ax.fill_between(x, base, top, color=colour, alpha=0.35, linewidth=0)
                ax.plot(x, top, color=colour, linewidth=2, solid_capstyle="round", label=n)
                base = top
            else:
                if kind == "area":
                    ax.fill_between(x, 0, vals, color=colour, alpha=0.10, linewidth=0)
                ax.plot(x, vals, color=colour, linewidth=2, solid_capstyle="round", drawstyle=drawstyle, label=n,
                        marker="o" if len(pos) <= 24 and not (kind == "area" and len(names) > 1) else None, markersize=4,
                        markeredgecolor=SURFACE, markeredgewidth=2)
            last = max((k for k, v in enumerate(vals) if not math.isnan(v)), default=None)
            first = min((k for k, v in enumerate(vals) if not math.isnan(v)), default=None)
            if LABELS in ("all", "ends"):
                for k, v in enumerate(vals):
                    if math.isnan(v) or (LABELS == "ends" and k not in (first, last)) or (len(names) == 1 and k == last) or (i, k) in DROPPED:
                        continue
                    ax.annotate(lab(v, vkind), (x[k], v), xytext=(0, 7), textcoords="offset points", ha="center", fontsize=9.5)
            if last is not None and len(names) == 1:
                # one series says its value; a panel that is one named measure (the NPL % panel) says its name too
                value_text = lab(vals[last], vkind) if show(last, len(vals)) else ""
                end_text = (n + "  " if named_end and value_text else n if named_end else "") + value_text
                if end_text:
                    ax.annotate(end_text, (x[last], vals[last]), xytext=(8, 0), textcoords="offset points", va="center", fontsize=9.5)
            elif last is not None and len(names) <= 4 and kind == "line":
                ax.annotate(n, (x[last], vals[last]), xytext=(8, 0), textcoords="offset points", va="center", fontsize=10, color=INK2)
        cat_labels(ax, df.index)
        ax.margins(y=0.12)  # room for the end labels and for a breach ring below the lowest point
        style_axes(ax)
    return names
"##;

fn ref_lines(data: &ChartData, panel: usize, horizontal: bool, opts: &LayoutOpts, measure: usize, out: &mut String, indent: &str, axname: &str) {
    let vk = value_kind(&data.spec, measure);
    // the lines, then their labels: staggered above the plot (vertical lines) or at the right end (horizontal lines), so
    // neighbouring labels never meet and none stands on a mark
    for (k, r) in data.spec.ref_lines.iter().filter(|r| r.panel == panel).enumerate() {
        let v = scaled(r.value, vk, opts);
        let label = if r.label.is_empty() { r.status.label().to_string() } else { format!("{} ({})", r.label, r.status.label()) };
        let call = if horizontal { "axvline" } else { "axhline" };
        let _ = writeln!(out, "{indent}{axname}.{call}({}, color={}, linewidth={})", f(v), status_colour(r.status), t::REFERENCE_LINE_PX);
        if horizontal {
            let _ = writeln!(out, "{indent}{axname}.annotate({}, ({}, 1), xycoords=(\"data\", \"axes fraction\"), xytext=(4, {}), textcoords=\"offset points\", fontsize=10, color=INK2, annotation_clip=False)", s(&label), f(v), 4 + 13 * k);
        } else {
            let _ = writeln!(out, "{indent}{axname}.annotate({}, (1, {}), xycoords=(\"axes fraction\", \"data\"), xytext=(-4, {}), textcoords=\"offset points\", ha=\"right\", fontsize=10, color=INK2, annotation_clip=False)", s(&label), f(v), 4 + 13 * k);
        }
    }
    for b in data.spec.ref_bands.iter().filter(|b| b.panel == panel) {
        let (a, c) = (scaled(b.from, vk, opts), scaled(b.to, vk, opts));
        let call = if horizontal { "axvspan" } else { "axhspan" };
        let _ = writeln!(out, "{indent}{axname}.{call}({}, {}, color=DEEMPH, alpha=0.35, linewidth=0, label={})", f(a.min(c)), f(a.max(c)), s(&b.label));
        if !b.label.is_empty() {
            let _ = writeln!(out, "{indent}{axname}.annotate({}, (0, {}), xycoords=(\"axes fraction\", \"data\"), xytext=(4, 3), textcoords=\"offset points\", fontsize=10, color=INK2)", s(&b.label), f(a.max(c)));
        }
    }
}

/// The Data labels setting as Python: which labels, their unit and decimals, and the ones left out.
fn label_helpers(plan: &super::labels::LabelPlan, opts: &LayoutOpts, dropped: &[super::labels::LabelKey], data: &ChartData) -> String {
    use super::spec::LabelMode;
    let mode = match plan.mode {
        LabelMode::Auto => "auto",
        LabelMode::All => "all",
        LabelMode::EndsOnly => "ends",
        LabelMode::None => "none",
    };
    let scale = if plan.opts.divisor > 0.0 { opts.divisor / plan.opts.divisor } else { 1.0 };
    let cats = &data.categories;
    let (fit, slot_chars) = if cats.is_empty() {
        ("full", 0)
    } else {
        let m = super::layout::ApproxMeasure;
        let wf = |s: &str| -> f32 { <super::layout::ApproxMeasure as super::layout::Measure>::size(&m, s, t::tick_pt(), false).x };
        let band = (720.0 - 90.0) / cats.len() as f32;
        let pl = super::catlabels::plan(cats, band, &wf, t::tick_pt() * 1.3);
        (
            match pl.fit {
                super::catlabels::CatFit::Full => "full",
                super::catlabels::CatFit::Wrapped => "wrapped",
                super::catlabels::CatFit::Angled => "angled",
            },
            ((band - 4.0) / (t::tick_pt() * 0.5)).floor().max(1.0) as usize,
        )
    };
    let mut cells: Vec<String> = dropped.iter().map(|(_, si, ci)| format!("({}, {})", si, ci)).collect();
    cells.sort();
    cells.dedup();
    format!(
        r#"LABELS = "{mode}"  # which data labels: auto, all, ends (first and last) or none
LABEL_SCALE = {scale}  # the labels' unit against the numbers in the tables below
LDEC = {ldec}  # decimals of the labels (None = the view's)
LPCT = {lpct}  # labels on percentages
DROPPED = {{{dropped}}}  # labels of "all" left out because they would overlap: (series, category)
CAT_FIT = "{fit}"  # category labels: full, wrapped (two lines) or angled (35 degrees); none is cut or left out
CAT_SLOT = {slot_chars}  # characters of a label that fit its slot

def cat_labels(ax, names):
    """Every category keeps its label: full, else on two lines, else angled 35 degrees (never cut, never left out)."""
    names = [str(n) for n in names]
    if CAT_FIT == "wrapped":
        out = []
        for n in names:
            if len(n) <= CAT_SLOT or " " not in n:
                out.append(n)
                continue
            cuts = [i for i, c in enumerate(n) if c == " "]
            i = min(cuts, key=lambda k: max(k, len(n) - k - 1))
            out.append(n[:i] + "\n" + n[i + 1:])
        names = out
    if CAT_FIT == "angled":
        ax.set_xticks(range(len(names)), names, rotation=35, ha="right", rotation_mode="anchor")
    else:
        ax.set_xticks(range(len(names)), names)

def ring_labels(ax, wedges, parts):
    """Donut labels outside the ring with leader lines ("HC  64%"); a part under 1% has none; labels are nudged apart."""
    import numpy as np
    total = float(parts["Value"].sum())
    items = []
    for w, name, v in zip(wedges, parts["Part"], parts["Value"]):
        share = v / total * 100 if total else 0
        if share < 1:
            continue
        ang = np.deg2rad((w.theta2 + w.theta1) / 2)
        items.append([np.cos(ang), np.sin(ang), str(name) + "  " + str(int(round(share))) + "%" if LABELS != "none" and LPCT else str(name)])
    for side in (1, -1):
        col = sorted((i for i in items if (1 if i[0] >= 0 else -1) == side), key=lambda i: i[1])
        ys = [1.25 * i[1] for i in col]
        for k in range(1, len(ys)):
            ys[k] = max(ys[k], ys[k - 1] + 0.14)
        for i, y in zip(col, ys):
            ax.annotate(i[2], xy=(i[0] * 0.98, i[1] * 0.98), xytext=(side * 1.3, y), ha="left" if side == 1 else "right", va="center", fontsize=9.5, color=INK,
                        arrowprops=dict(arrowstyle="-", color=MUTED, linewidth=1, connectionstyle="angle,angleA=0,angleB=90,rad=4"))

def show(i, n, auto=True):
    """Whether the label at position i of n is printed."""
    return auto if LABELS == "auto" else True if LABELS == "all" else i in (0, n - 1) if LABELS == "ends" else False

def lab(v, kind="amount", signed=False):
    """A data label as the chart writes it: the label unit and decimals, a sign for a signed bar."""
    if LABELS == "none" or (kind == "percent" and not LPCT) or v is None or (isinstance(v, float) and math.isnan(v)):
        return ""
    if kind == "amount":
        v = v * LABEL_SCALE
    d = LDEC if LDEC is not None else (2 if (signed and kind == "percent") else None)
    text = fmt(v, kind, d)
    return ("+" + text) if signed and v > 0 else text"#,
        mode = mode,
        scale = scale,
        ldec = plan.decimals.map(|d| d.to_string()).unwrap_or_else(|| "None".to_string()),
        lpct = if plan.percent { "True" } else { "False" },
        dropped = cells.join(", "),
        fit = fit,
        slot_chars = slot_chars,
    )
}

/// The script for a chart.
pub fn chart_to_python(data: &ChartData, opts: &LayoutOpts) -> Result<String> {
    t::with_text_scale(data.spec.text_size.factor(), || chart_to_python_scaled(data, opts))
}

fn chart_to_python_scaled(data: &ChartData, opts: &LayoutOpts) -> Result<String> {
    if let Some(m) = data.non_finite() {
        anyhow::bail!("Chart: {}. Fix those rows or filter them out, then chart again.", m);
    }
    if let Some(r) = data.empty_reason() {
        bail!("No data to chart: {}.", r);
    }
    let spec = &data.spec;
    let kind = spec.kind;
    let mut w = String::new();
    let title = chart_title(data);
    let _ = writeln!(w, "# {}", title.replace('\n', " "));
    let _ = writeln!(w, "# Made by MicroParquet, Charts > Copy as Python. The numbers below are the chart's own table, in the chart's scale{}.", if super::layout::shows_unit(data, opts) { format!(" ({})", opts.unit) } else { String::new() });
    let plotly = matches!(kind, ChartKind::Sankey | ChartKind::Treemap);
    if plotly {
        let _ = writeln!(w, "# A Sankey and a treemap have no matplotlib form: this script uses plotly (pip install plotly pandas).");
    } else {
        let _ = writeln!(w, "# Needs: pip install pandas matplotlib");
    }
    let plan = super::labels::LabelPlan::new(spec, opts);
    let dropped = super::labels::dropped_labels(data, opts, 1000.0, 520.0);
    let label_code = label_helpers(&plan, opts, &dropped, data);
    preamble(&mut w, opts.dark, plotly, opts.decimals, &label_code);
    let unit = if opts.unit.is_empty() { String::new() } else { opts.unit.clone() };
    let subtitle = super::layout::subtitle_text(data, opts);
    let mut foot_parts: Vec<String> = Vec::new();
    if !spec.footnote.trim().is_empty() {
        foot_parts.push(spec.footnote.trim().to_string());
    }
    foot_parts.extend(super::labels::footnote_notes(data, opts, dropped.len()));
    let _ = writeln!(w, "\nTITLE = {}\nSUBTITLE = {}\nFOOTNOTE = {}", s(&super::tokens::tidy(&title)), s(&super::tokens::tidy(&subtitle)), s(&super::tokens::tidy(&foot_parts.join(" \u{b7} "))));
    let (cm, _) = colours_of(data, opts);
    let vk0 = value_kind(spec, 0);
    match kind {
        ChartKind::Sankey => sankey(&mut w, data, opts),
        ChartKind::Treemap => treemap(&mut w, data, opts),
        ChartKind::Scatter | ChartKind::Bubble => scatter(&mut w, data, &cm),
        ChartKind::BoxPlot => boxes(&mut w, data, &cm),
        ChartKind::Heatmap => heat(&mut w, data, opts)?,
        ChartKind::Donut => donut(&mut w, data, opts, &cm)?,
        ChartKind::Waterfall => waterfall(&mut w, data, opts)?,
        ChartKind::Histogram => histogram(&mut w, data, &cm),
        _ => cartesian(&mut w, data, opts, &cm, vk0),
    }
    Ok(w)
}

fn cartesian(w: &mut String, data: &ChartData, opts: &LayoutOpts, cm: &ColourMap, vk0: ValueKind) {
    let spec = &data.spec;
    let horizontal = spec.horizontal && spec.kind == ChartKind::Bar;
    let stack = match spec.stack {
        Stack::Stacked => "stacked",
        Stack::Percent => "percent",
        _ => "grouped",
    };
    w.push_str(DRAW);
    let kind_of = |k: ChartKind| match k {
        ChartKind::Line => "line",
        ChartKind::Area => "area",
        _ => "bar",
    };
    let _ = writeln!(w, "\nCOLOURS = {}", colour_dict(data, cm));
    if !data.facets.is_empty() {
        // small multiples: one panel per facet value, one scale for all
        let _ = writeln!(w, "\nfacets = {{}}");
        for f_ in &data.facets {
            let name = format!("facets[{}]", s(&f_.name));
            let _ = writeln!(w, "{}", frame_literal(&f_.data, 0, opts, &name));
        }
        let _ = writeln!(w, "\ncols = min(len(facets), 4)\nrows = -(-len(facets) // cols)\nfig, axes = plt.subplots(rows, cols, sharey=True, figsize=(4.2 * cols, 3.2 * rows + 1.4), squeeze=False)\nfig.subplots_adjust(top=0.80, bottom=0.10, hspace=0.45)");
        let _ = writeln!(w, "for ax, (name, df) in zip(axes.flat, facets.items()):\n    draw(ax, df, {}, stack={}, horizontal={}, colours=COLOURS, vkind={})\n    ax.set_title(name, loc=\"left\", fontsize=10, color=INK2)\nfor ax in list(axes.flat)[len(facets):]:\n    ax.set_visible(False)", s(kind_of(spec.kind)), s(stack), if horizontal { "True" } else { "False" }, s(kind_word(vk0)));
        let _ = writeln!(w, "titles(fig, TITLE, SUBTITLE, FOOTNOTE)\nplt.show()");
        return;
    }
    // panels: the linked panels of the spec, else one panel per measure, else one
    let panels: Vec<(ChartKind, usize, String, bool)> = if !spec.panels.is_empty() {
        spec.panels.iter().take(t::MAX_PANELS).map(|p| (p.kind, p.measures.first().copied().unwrap_or(0).min(data.measure_count().saturating_sub(1)), p.title.clone(), p.single)).collect()
    } else if data.measure_count() > 1 {
        (0..data.measure_count().min(t::MAX_PANELS)).map(|m| (spec.kind, m, data.measure_names.get(m).cloned().unwrap_or_default(), false)).collect()
    } else {
        vec![(spec.kind, 0, String::new(), false)]
    };
    let mut done: Vec<(usize, bool)> = Vec::new();
    for (_, m, _, single) in &panels {
        if !done.contains(&(*m, *single)) {
            done.push((*m, *single));
            if *single {
                // a ratio that is the same in every series: one column named after the measure
                let cat = if spec.category.is_empty() { "Category".to_string() } else { spec.category.clone() };
                let vk = value_kind(spec, *m);
                let md = data.measure(*m);
                let (vals, _) = md.as_ref().map(|x| super::data::collapse_series(&x.values, &x.rows, data.categories.len())).unwrap_or_default();
                let vals: Vec<Option<f64>> = vals.into_iter().map(|v| v.map(|x| scaled(x, vk, opts))).collect();
                let name = data.measure_names.get(*m).cloned().unwrap_or_else(|| "Value".to_string());
                let _ = writeln!(w, "\npanel{}_one = pd.DataFrame({{\n    {}: {},\n    {}: {}\n}}).set_index({})", m, s(&cat), list_s(&data.categories), s(&name), list_f(&vals), s(&cat));
            } else {
                let _ = writeln!(w, "\n{}", frame_literal(data, *m, opts, &format!("panel{}", m)));
            }
        }
    }
    let n = panels.len();
    let _ = writeln!(w, "\nfig, axes = plt.subplots({n}, 1, sharex=True, figsize=(10, {}), squeeze=False)\nfig.subplots_adjust(top={}, bottom=0.12, hspace=0.35)", 2.6 * n as f32 + 2.2, if n == 1 { "0.80" } else { "0.78" });
    for (i, (k, m, title, single)) in panels.iter().enumerate() {
        let vk = value_kind(spec, *m);
        let frame = if *single { format!("panel{}_one", m) } else { format!("panel{}", m) };
        let lead = if *single { "_ = " } else { "names = " };
        let _ = writeln!(w, "ax = axes[{i}][0]\n{lead}draw(ax, {frame}, {}, stack={}, horizontal={}, colours=COLOURS, stepped={}, vkind={}{}{})", s(kind_of(*k)), s(stack), if horizontal { "True" } else { "False" }, if spec.stepped { "True" } else { "False" }, s(kind_word(vk)), if *single { ", named_end=True" } else { "" }, if spec.net_marker { ", label_values=False, net=True" } else if spec.segment_labels { ", segments=True" } else { "" });
        if !title.is_empty() {
            let _ = writeln!(w, "ax.set_title({}, loc=\"left\", fontsize=10, color=INK2)", s(title));
        }
        ref_lines(data, i, horizontal, opts, *m, w, "", "ax");
        if i == 0 && !spec.bar_status.is_empty() && *k == ChartKind::Bar && data.series.len() <= 1 {
            // bars in a status colour, with the status in words and an icon (the colour is never alone)
            let items: Vec<String> = spec
                .bar_status
                .iter()
                .filter_map(|b| data.categories.iter().position(|c| *c == b.category).map(|ci| format!("({}, {}, {}, {})", ci, status_colour(b.status), s(&super::tokens::tidy(&b.label)), if b.status == Status::Critical { "\"white\"" } else { "\"#0b0b0b\"" })))
                .collect();
            let _ = writeln!(w, "for k, colour, text, ink in [{}]:\n    ax.patches[k].set_facecolor(colour)\n    ax.annotate(\"\\u25b2 \" + text, ({}, k), xytext=({}, {}), textcoords=\"offset points\", va=\"center\", fontsize=9.5, color=ink)", items.join(", "), if horizontal { "0" } else { "k" }, 8, 0);
        }
        // limit ticks (per-category limits) as short dashes in their status colour
        for lt in spec.limit_ticks.iter().filter(|l| l.panel == i) {
            if let Some(ci) = data.categories.iter().position(|c| *c == lt.category) {
                let v = f(scaled(lt.value, vk, opts));
                if horizontal {
                    // a tick across the bar at its limit; the breach written to the right of it
                    let _ = writeln!(w, "ax.plot([{v}, {v}], [{ci} - 0.3, {ci} + 0.3], color={}, linewidth=1.5)  # limit of {}", status_colour(lt.status), s(&lt.category));
                    if !lt.breach.is_empty() {
                        let _ = writeln!(w, "ax.annotate({}, ({v}, {ci}), xytext=(8, 0), textcoords=\"offset points\", va=\"center\", fontsize=10, color=INK)", s(&super::tokens::tidy(&lt.breach)));
                    }
                } else {
                    let _ = writeln!(w, "ax.plot([{ci} - 0.3, {ci} + 0.3], [{v}, {v}], color={}, linewidth=1.5)  # limit of {}", status_colour(lt.status), s(&lt.category));
                    if !lt.breach.is_empty() {
                        let _ = writeln!(w, "ax.annotate({}, ({ci}, {v}), xytext=(0, -16), textcoords=\"offset points\", ha=\"center\", fontsize=10, color=INK)", s(&super::tokens::tidy(&lt.breach)));
                    }
                }
            }
        }
    }
    let _ = writeln!(w, "if len(names) > 1:\n    fig.legend(handles=[Patch(color=COLOURS.get(n, SERIES[i % len(SERIES)]), label=n) for i, n in enumerate(names)], loc=\"upper left\", bbox_to_anchor=(0.015, 0.89), ncol=min(len(names), 6))\ntitles(fig, TITLE, SUBTITLE, FOOTNOTE)\nplt.show()");
}

fn histogram(w: &mut String, data: &ChartData, cm: &ColourMap) {
    let one = format!("({})", one_colour_expr(data, cm));
    let counts: Vec<Option<f64>> = data.values.first().map(|r| r.clone()).unwrap_or_default();
    let _ = writeln!(w, "\nhist = pd.DataFrame({{\"Bin\": {}, \"Count\": {}}})", list_s(&data.categories), list_f(&counts));
    let _ = writeln!(
        w,
        "\nfig, ax = plt.subplots(figsize=(10, 4.8))\nfig.subplots_adjust(top=0.80, bottom=0.2)\nax.bar(range(len(hist)), hist[\"Count\"], width=0.9, color={one}, edgecolor=SURFACE, linewidth=2)\nax.set_xticks(range(len(hist)), hist[\"Bin\"], rotation=45, ha=\"right\")\nstyle_axes(ax)\ntitles(fig, TITLE, SUBTITLE, FOOTNOTE)\nplt.show()"
    );
}

fn scatter(w: &mut String, data: &ChartData, cm: &ColourMap) {
    let spec = &data.spec;
    let (mut x, mut y, mut z, mut g): (Vec<Option<f64>>, Vec<Option<f64>>, Vec<Option<f64>>, Vec<String>) = Default::default();
    for p in &data.points {
        x.push(Some(p.x));
        y.push(Some(p.y));
        z.push(p.size);
        g.push(data.series.get(p.series).cloned().unwrap_or_default());
    }
    let xname = if spec.category.is_empty() { "x".to_string() } else { spec.category.clone() };
    let yname = data.measure_names.first().cloned().unwrap_or_else(|| "y".to_string());
    let _ = writeln!(w, "\npoints = pd.DataFrame({{\n    {}: {},\n    {}: {},\n    \"Size\": {},\n    \"Series\": {}\n}})", s(&xname), list_f(&x), s(&yname), list_f(&y), list_f(&z), list_s(&g));
    let _ = writeln!(w, "COLOURS = {}", colour_dict(data, cm));
    let _ = writeln!(
        w,
        "\nfig, ax = plt.subplots(figsize=(10, 5.4))\nfig.subplots_adjust(top=0.80, bottom=0.12)\nfor i, (name, part) in enumerate(points.groupby(\"Series\", sort=False)):\n    size = 40 if part[\"Size\"].isna().all() else 20 + 380 * part[\"Size\"] / points[\"Size\"].max()\n    ax.scatter(part[{}], part[{}], s=size, color=COLOURS.get(name, SERIES[i % len(SERIES)]), alpha=0.85, edgecolors=SURFACE, linewidths=1.5, label=name or None)\nax.set_xlabel({})\nax.set_ylabel({})\nax.grid(axis=\"y\")\nax.set_axisbelow(True)\nax.spines[\"bottom\"].set_color(BASELINE)\nif points[\"Series\"].nunique() > 1:\n    ax.legend(loc=\"upper left\", bbox_to_anchor=(0, 1.12), ncol=3)\ntitles(fig, TITLE, SUBTITLE, FOOTNOTE)\nplt.show()",
        s(&xname),
        s(&yname),
        s(&xname),
        s(&yname)
    );
}

fn boxes(w: &mut String, data: &ChartData, cm: &ColourMap) {
    let one = format!("({})", one_colour_expr(data, cm));
    let mut rows = Vec::new();
    for b in &data.boxes {
        rows.push(format!(
            "    {{\"label\": {}, \"q1\": {}, \"med\": {}, \"q3\": {}, \"whislo\": {}, \"whishi\": {}, \"fliers\": [{}], \"n\": {}}}",
            s(&b.category),
            f(b.q1),
            f(b.median),
            f(b.q3),
            f(b.low_whisker),
            f(b.high_whisker),
            b.outliers.iter().map(|o| f(*o)).collect::<Vec<_>>().join(", "),
            b.n
        ));
    }
    let _ = writeln!(w, "\nbox_table = pd.DataFrame([\n{}\n])  # q1, median, q3, the whiskers (1.5 x the interquartile range) and the outliers\nstats = box_table.to_dict(\"records\")", rows.join(",\n"));
    let _ = writeln!(
        w,
        "\nfig, ax = plt.subplots(figsize=(10, 5.2))\nfig.subplots_adjust(top=0.80, bottom=0.12)\nax.bxp(stats, patch_artist=True, widths=0.4, showfliers=True,\n       boxprops=dict(facecolor={one}, edgecolor=SURFACE), medianprops=dict(color=SURFACE, linewidth=2),\n       whiskerprops=dict(color=INK2, linewidth=1.5), capprops=dict(color=INK2, linewidth=1.5),\n       flierprops=dict(marker=\"o\", markerfacecolor={one}, markeredgecolor=SURFACE, markersize=6))\nstyle_axes(ax)\ntitles(fig, TITLE, SUBTITLE, FOOTNOTE)\nplt.show()"
    );
}

fn waterfall(w: &mut String, data: &ChartData, opts: &LayoutOpts) -> Result<()> {
    let steps = waterfall_from_data(data)?;
    let vk = value_kind(&data.spec, 0);
    let sc = |v: f64| Some(scaled(v, vk, opts));
    let _ = writeln!(
        w,
        "\nsteps = pd.DataFrame({{\n    \"Step\": {},\n    \"Kind\": {},\n    \"Value\": {},\n    \"Base\": {},\n    \"Top\": {},\n    \"Level\": {}\n}})",
        list_s(&steps.iter().map(|x| x.label.clone()).collect::<Vec<_>>()),
        list_s(&steps.iter().map(|x| match x.kind { StepKind::Start => "start", StepKind::End => "end", StepKind::Delta => "delta" }.to_string()).collect::<Vec<_>>()),
        list_f(&steps.iter().map(|x| sc(x.value)).collect::<Vec<_>>()),
        list_f(&steps.iter().map(|x| sc(x.base)).collect::<Vec<_>>()),
        list_f(&steps.iter().map(|x| sc(x.top)).collect::<Vec<_>>()),
        list_f(&steps.iter().map(|x| sc(x.level)).collect::<Vec<_>>())
    );
    let _ = writeln!(
        w,
        "\nTOTAL_START, TOTAL_END = {}, {}\nfig, ax = plt.subplots(figsize=(10, 5.2))\nfig.subplots_adjust(top=0.80, bottom=0.14)\nfor i, r in steps.reset_index().iterrows():\n    colour = TOTAL_START if r.Kind == \"start\" else TOTAL_END if r.Kind == \"end\" else (\"#2a78d6\" if r.Value > 0 else \"#e34948\" if r.Value < 0 else \"#c3c2b7\")\n    ax.bar(r['index'], max(r.Top - r.Base, 0.002 * max(abs(steps.Top).max(), 1)), bottom=r.Base, width=0.55, color=colour, edgecolor=SURFACE)\n    if show(r['index'], len(steps)):\n        ax.annotate(lab(r.Value, {}, r.Kind == \"delta\"), (r['index'], r.Top), xytext=(0, 4), textcoords=\"offset points\", ha=\"center\", fontsize=9.5)\n    if r['index'] < len(steps) - 1:\n        ax.plot([r['index'] + 0.275, r['index'] + 1 - 0.275], [r.Level, r.Level], color=BASELINE, linewidth=1)\nax.set_xticks(range(len(steps)), steps[\"Step\"])\nax.axhline(0, color=BASELINE, linewidth=1)\nstyle_axes(ax)\ntitles(fig, TITLE, SUBTITLE, FOOTNOTE)\nplt.show()",
        hexs(t::WATERFALL_START),
        hexs(t::WATERFALL_END),
        s(kind_word(vk))
    );
    Ok(())
}

fn heat(w: &mut String, data: &ChartData, opts: &LayoutOpts) -> Result<()> {
    let transform = data.spec.measure_list().first().map(|m| m.transform).unwrap_or(Transform::None);
    let h = heatmap(data, Transform::None);
    let _ = transform;
    let vk = value_kind(&data.spec, 0);
    let cells: Vec<String> = h.cells.iter().map(|r| list_f(&r.iter().map(|v| v.map(|x| scaled(x, vk, opts))).collect::<Vec<_>>())).collect();
    let _ = writeln!(w, "\nmatrix = pd.DataFrame([\n    {}\n], index={}, columns={})", cells.join(",\n    "), list_s(&h.rows), list_s(&h.cols));
    let ramp = format!("[{}]", t::SEQUENTIAL.iter().map(|c| hexs(*c)).collect::<Vec<_>>().join(", "));
    let code = r##"
RAMP = @RAMP@  # one hue, 13 steps; in the dark theme low values use the darkest step
if DARK:
    RAMP = RAMP[::-1]
NEUTRAL, NEUTRAL_TEXT = GRID, MUTED  # zero and missing cells: near the surface, muted text; the ramp is for values above zero
KIND = @KIND@
fig, ax = plt.subplots(figsize=(10, 0.9 * len(matrix) + 2.4))
fig.subplots_adjust(top=0.78, bottom=0.08, left=0.14)
positive = [v for v in matrix.to_numpy().flatten() if not pd.isna(v) and v > 0]
lo, hi = (min(positive), max(positive)) if positive else (0, 0)
for r in range(len(matrix)):
    for c in range(len(matrix.columns)):
        v = matrix.iat[r, c]
        if pd.isna(v) or v <= 0:
            fill, text_colour = NEUTRAL, NEUTRAL_TEXT
        else:
            step = 6 if hi == lo else round((v - lo) / (hi - lo) * 12)
            fill = RAMP[step]
            text_colour = "white" if (step if not DARK else 12 - step) >= 6 else "#0b0b0b"
        ax.add_patch(plt.Rectangle((c + 0.02, r + 0.02), 0.96, 0.96, color=fill, linewidth=0))
        label = "" if pd.isna(v) else lab(v, KIND).rstrip("%" if KIND == "percent" else "")
        ax.text(c + 0.5, r + 0.5, label, ha="center", va="center", fontsize=9.5, color=text_colour)
ax.set_xlim(0, len(matrix.columns))
ax.set_ylim(len(matrix), 0)
ax.set_xticks([c + 0.5 for c in range(len(matrix.columns))], matrix.columns)
ax.xaxis.tick_top()
ax.set_yticks([r + 0.5 for r in range(len(matrix))], matrix.index)
for sp in ax.spines.values():
    sp.set_visible(False)
titles(fig, TITLE, SUBTITLE, FOOTNOTE)
plt.show()"##;
    let _ = writeln!(w, "{}", code.replace("@RAMP@", &ramp).replace("@KIND@", &s(kind_word(vk))));
    Ok(())
}

fn donut(w: &mut String, data: &ChartData, opts: &LayoutOpts, cm: &ColourMap) -> Result<()> {
    let vals: Vec<Option<f64>> = shown(data, 0, opts).first().cloned().unwrap_or_default();
    if vals.iter().flatten().count() < 2 {
        bail!("A donut needs at least 2 parts, but this chart has {}. Choose another form.", vals.iter().flatten().count());
    }
    let mut cm = cm.clone();
    cm.assign_all(&data.categories);
    let colours: Vec<String> = data.categories.iter().enumerate().map(|(i, c)| {
        let l = cm.get(c).map(|x| x.hex(false)).unwrap_or(t::SERIES_LIGHT[i.min(t::SLOTS - 1)]);
        let d = cm.get(c).map(|x| x.hex(true)).unwrap_or(t::SERIES_DARK[i.min(t::SLOTS - 1)]);
        format!("{} if not DARK else {}", hexs(l), hexs(d))
    }).collect();
    let _ = writeln!(w, "\nparts = pd.DataFrame({{\"Part\": {}, \"Value\": {}}})", list_s(&data.categories), list_f(&vals));
    let _ = writeln!(
        w,
        "\nCOLOURS = [{}]\nfig, ax = plt.subplots(figsize=(8, 6))\nfig.subplots_adjust(top=0.80, right=0.66)\nwedges, _ = ax.pie(parts[\"Value\"], colors=COLOURS, startangle=90, counterclock=False, wedgeprops=dict(width=0.42, edgecolor=SURFACE, linewidth=2))\nring_labels(ax, wedges, parts)\nax.text(0, 0, fmt(parts[\"Value\"].sum()), ha=\"center\", va=\"center\", fontsize=15, fontweight=\"semibold\")\nax.legend([f\"{{p}}  {{lab(v)}}\" if LABELS != \"none\" else p for p, v in zip(parts[\"Part\"], parts[\"Value\"])], loc=\"center left\", bbox_to_anchor=(1, 0.5))\ntitles(fig, TITLE, SUBTITLE, FOOTNOTE)\nplt.show()",
        colours.join(", ")
    );
    Ok(())
}

fn sankey(w: &mut String, data: &ChartData, opts: &LayoutOpts) {
    let vals = shown(data, 0, opts);
    let (mut src, mut dst, mut val): (Vec<String>, Vec<String>, Vec<Option<f64>>) = Default::default();
    for (si, row) in vals.iter().enumerate() {
        for (ci, v) in row.iter().enumerate() {
            if let Some(v) = v {
                if *v > 0.0 {
                    src.push(data.series.get(si).cloned().unwrap_or_default());
                    dst.push(data.categories.get(ci).cloned().unwrap_or_default());
                    val.push(Some(*v));
                }
            }
        }
    }
    let _ = writeln!(w, "\nflows = pd.DataFrame({{\"From\": {}, \"To\": {}, \"Value\": {}}})", list_s(&src), list_s(&dst), list_f(&val));
    let _ = writeln!(
        w,
        "\nfrom_nodes = list(dict.fromkeys(flows[\"From\"]))\nto_nodes = list(dict.fromkeys(flows[\"To\"]))\nlabels = [f\"From {{n}}\" for n in from_nodes] + [f\"To {{n}}\" for n in to_nodes]\nindex = {{n: i for i, n in enumerate(labels)}}\nfig = go.Figure(go.Sankey(\n    node=dict(label=from_nodes + to_nodes, color=[SERIES[i % len(SERIES)] for i in range(len(labels))], pad=18, thickness=14),\n    link=dict(source=[index[f\"From {{n}}\"] for n in flows[\"From\"]], target=[index[f\"To {{n}}\"] for n in flows[\"To\"]], value=flows[\"Value\"]),\n))\nfig.update_layout(title=dict(text=TITLE + \"<br><sup>\" + SUBTITLE + \"</sup>\", x=0.01), paper_bgcolor=SURFACE, font=dict(family=\", \".join(FONT), color=INK))\nfig.show()"
    );
}

fn treemap(w: &mut String, data: &ChartData, opts: &LayoutOpts) {
    let vals = shown(data, 0, opts);
    let (mut lab, mut par, mut val): (Vec<String>, Vec<String>, Vec<Option<f64>>) = Default::default();
    let grouped = data.series.iter().any(|n| !n.is_empty());
    for (si, row) in vals.iter().enumerate() {
        for (ci, v) in row.iter().enumerate() {
            if let Some(v) = v {
                if *v > 0.0 {
                    lab.push(data.categories.get(ci).cloned().unwrap_or_default());
                    par.push(if grouped { data.series.get(si).cloned().unwrap_or_default() } else { String::new() });
                    val.push(Some(*v));
                }
            }
        }
    }
    let _ = writeln!(w, "\nparts = pd.DataFrame({{\"Label\": {}, \"Group\": {}, \"Value\": {}}})", list_s(&lab), list_s(&par), list_f(&val));
    let _ = writeln!(
        w,
        "\nlabels = list(parts[\"Label\"]) + [g for g in dict.fromkeys(parts[\"Group\"]) if g]\nparents = list(parts[\"Group\"]) + [\"\"] * len([g for g in dict.fromkeys(parts[\"Group\"]) if g])\nvalues = list(parts[\"Value\"]) + [0] * len([g for g in dict.fromkeys(parts[\"Group\"]) if g])\nfig = go.Figure(go.Treemap(labels=labels, parents=parents, values=values, branchvalues=\"remainder\", marker=dict(colors=[SERIES[i % len(SERIES)] for i in range(len(labels))])))\nfig.update_layout(title=dict(text=TITLE + \"<br><sup>\" + SUBTITLE + \"</sup>\", x=0.01), paper_bgcolor=SURFACE, font=dict(family=\", \".join(FONT), color=INK))\nfig.show()"
    );
}
