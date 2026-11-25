use crate::models::{
    parquet_native_type, parquet_needs_conversion, AggFunc, BlankRule, ColDataType, ColumnSchemaConfig, CondOp,
    CsvTextEncoding, FilterCondition, FilterOp, GroupJoin, IngestionConfig, MacroRecipe, PiiMaskMode,
    TransformStep,
};
use anyhow::{Context, Result};
use std::path::Path;

pub fn save_macro_to_file(recipe: &MacroRecipe, path: &Path) -> Result<()> {
    // The program version is written next to the recipe; older files without it still load.
    let mut value = serde_json::to_value(recipe).context("Failed to serialize macro recipe")?;
    if let Some(obj) = value.as_object_mut() {
        obj.insert("app_version".to_string(), env!("CARGO_PKG_VERSION").into());
    }
    let json = serde_json::to_string_pretty(&value).context("Failed to serialize macro recipe")?;
    std::fs::write(path, json).context("Failed to write .mpq.json file")?;
    Ok(())
}

/// The program version a macro file was saved with (`None` for files from before it was written).
pub fn macro_file_version(path: &Path) -> Option<String> {
    let raw = std::fs::read_to_string(path).ok()?;
    let v: serde_json::Value = serde_json::from_str(&raw).ok()?;
    v.get("app_version")?.as_str().map(str::to_string)
}

pub fn load_macro_from_file(path: &Path) -> Result<MacroRecipe> {
    let raw = std::fs::read_to_string(path).context("Failed to read .mpq.json file")?;
    let recipe: MacroRecipe =
        serde_json::from_str(&raw).context("Failed to parse .mpq.json structure")?;
    Ok(recipe)
}

/// True when the Python generator has a translation for this step.
fn has_python_translation(step: &TransformStep) -> bool {
    !matches!(
        step,
        TransformStep::RateShockSim { .. }
            | TransformStep::FtpCurveMap { .. }
            | TransformStep::BankingHygieneAudit { .. }
            | TransformStep::LoanClassificationProvision { .. }
            | TransformStep::LoanLimitUtilization { .. }
            | TransformStep::ClassifyOverdue { .. }
            | TransformStep::IfrsEcl { .. }
            | TransformStep::PivotTable { .. }
    )
}

/// "Step N '<label>'" for every step the Python export cannot reproduce, for UI warnings.
pub fn python_untranslated_steps(steps: &[TransformStep]) -> Vec<String> {
    steps
        .iter()
        .enumerate()
        .filter(|(_, s)| !has_python_translation(s))
        .map(|(i, s)| format!("Step {} '{}'", i + 1, s.short_label()))
        .collect()
}

/// Python mirror of `engine::natural_sort_key` and of the sort keys of a sort step.
const PY_NATURAL_KEY_HELPER: &str = r#"import re as _re


def _mp_natural_key(v):
    """Same order as MicroParquet: case-insensitive, numbers inside text by value."""
    if v is None:
        return None
    out = []
    for part in _re.split(r"([0-9]+)", v.lower()):
        if part[:1].isascii() and part[:1].isdigit():
            d = part.lstrip("0") or "0"
            out.append("%04d%s" % (min(len(d), 9999), d))
        else:
            out.append(part)
    return "".join(out)


def _mp_sort_key(e, dtype):
    if dtype != pl.Utf8:
        return e
    blank = e.str.strip_chars() == ""
    return pl.when(blank).then(None).otherwise(e).map_elements(_mp_natural_key, return_dtype=pl.Utf8)


"#;

/// Python mirror of the custom-list and Tenor sort orders (`sort_lists::order_sort_keys`, U3).
pub const PY_ORDER_HELPER: &str = r#"_MP_TENOR_DAYS = {"d": 1, "day": 1, "days": 1, "w": 7, "wk": 7, "wks": 7, "week": 7, "weeks": 7,
                  "m": 365 / 12, "mo": 365 / 12, "mos": 365 / 12, "mon": 365 / 12, "mons": 365 / 12,
                  "month": 365 / 12, "months": 365 / 12,
                  "y": 365, "yr": 365, "yrs": 365, "year": 365, "years": 365}


def _mp_tenor_days(v):
    """Days of a tenor such as 3y, 6 M, 100D or O/N; None when the text is not one."""
    t = v.strip().lower()
    if t in ("o/n", "on", "overnight"):
        return 1.0
    m = _re.fullmatch(r"([0-9.]+)\s*([a-z]*)", t)
    if not m or m.group(2) not in _MP_TENOR_DAYS:
        return None
    try:
        n = float(m.group(1))
    except ValueError:
        return None
    if n != n or n in (float("inf"), float("-inf")):
        return None  # the app refuses a number too big to hold
    return n * _MP_TENOR_DAYS[m.group(2)]


def _mp_rank_keys(e, rank_fn, sub_fn=None):
    """Three sort keys: listed (0) before the rest (1), the rank, then the natural key.
    With sub_fn (the Ranges order) a fourth key breaks ties of the rank: Below x, x, x to y, Above x."""
    blank = e.str.strip_chars() == ""
    t = pl.when(blank).then(None).otherwise(e)
    rank = t.map_elements(rank_fn, return_dtype=pl.Float64)
    group = pl.when(t.is_null()).then(None).when(rank.is_null()).then(1).otherwise(0)
    keys = [group, rank]
    if sub_fn is not None:
        keys.append(t.map_elements(sub_fn, return_dtype=pl.Float64))
    return keys + [t.map_elements(_mp_natural_key, return_dtype=pl.Utf8)]


"#;

/// Python mirror of `dates::DatePart::value` (Split date, the filter "By" choice; Phase 8 W1/W2): plain Python on a
/// `datetime.date` (a `datetime` drops its time), with the same labels and week rules. Keep the two in step.
pub const PY_DATE_PART_HELPER: &str = r#"import datetime as _dt
_MP_MON_ABBR = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"]
_MP_DAY_ABBR = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"]


def _mp_week_starting(d, week_start):
    """The first day of the week of d (on or before it); week_start is days since Monday."""
    return d - _dt.timedelta(days=(d.weekday() - week_start) % 7)


def _mp_date_part(d, part, fy_start, week_start):
    """One part of a date, as MicroParquet writes it. Week of year: W01 is the week that contains 1 January."""
    if d is None:
        return None
    if isinstance(d, _dt.datetime):
        d = d.date()
    q = (d.month - 1) // 3 + 1
    fm = (d.month - fy_start) % 12 + 1
    if part == "Year":
        return d.year
    if part == "Quarter":
        return "Q%d" % q
    if part == "Year-Quarter":
        return "%d Q%d" % (d.year, q)
    if part == "Month":
        return _MP_MON_ABBR[d.month - 1]
    if part == "Month number":
        return d.month
    if part == "Month-Year":
        return "%s %d" % (_MP_MON_ABBR[d.month - 1], d.year)
    if part == "Week of year":
        first = _mp_week_starting(_dt.date(d.year, 1, 1), week_start)
        return "W%02d" % ((_mp_week_starting(d, week_start) - first).days // 7 + 1)
    if part == "Week starting":
        return _mp_week_starting(d, week_start)
    if part == "Day name":
        return _MP_DAY_ABBR[d.weekday()]
    if part == "Day of month":
        return d.day
    if part == "Date":
        return d
    if part == "Fiscal year":
        y = d.year if d.month >= fy_start else d.year - 1
        return "FY%d" % y if fy_start == 1 else "FY%d-%02d" % (y, (y + 1) % 100)
    if part == "Fiscal quarter":
        return "FQ%d" % ((fm - 1) // 3 + 1)
    if part == "Fiscal month":
        return fm
    raise ValueError("Unknown date part: %s" % part)


def _mp_date_key(d, part, fy_start, week_start):
    """Where the part of a date stands in the part's own order, as a whole number (filters compare by it, never by
    the text): the year; months and quarters counted from year 0; the month 1-12; the quarter 1-4; the day's place in
    the week from the week start (0 = the first day). A blank date has no key."""
    if d is None:
        return None
    if isinstance(d, _dt.datetime):
        d = d.date()
    q = (d.month - 1) // 3 + 1
    if part == "Year":
        return d.year
    if part == "Year-Quarter":
        return d.year * 4 + q - 1
    if part == "Month-Year":
        return d.year * 12 + d.month - 1
    if part == "Month":
        return d.month
    if part == "Quarter":
        return q
    if part == "Day name":
        return (d.weekday() - week_start) % 7
    raise ValueError("No order to compare by: %s" % part)


"#;

/// The Python expression of one date part of a column (through `_mp_date_part`).
pub fn py_date_part_expr(column: &str, part: crate::dates::DatePart, cal: &crate::dates::CalendarSettings) -> String {
    let dtype = match part.part_type() {
        crate::dates::PartType::Int => "pl.Int32",
        crate::dates::PartType::Text => "pl.Utf8",
        crate::dates::PartType::Date => "pl.Date",
    };
    format!(
        "pl.col({}).map_elements(lambda d: _mp_date_part(d, {}, {}, {}), return_dtype={})",
        serde_json::to_string(column).unwrap_or_default(),
        serde_json::to_string(part.label()).unwrap_or_default(),
        cal.fy_start_month,
        cal.week_start.from_monday(),
        dtype
    )
}

/// The Python expression of the order key of a date part of a column (through `_mp_date_key`; Phase 8 W9).
pub fn py_date_key_expr(column: &str, part: crate::dates::DatePart, cal: &crate::dates::CalendarSettings) -> String {
    format!(
        "pl.col({}).map_elements(lambda d: _mp_date_key(d, {}, {}, {}), return_dtype=pl.Int64)",
        serde_json::to_string(column).unwrap_or_default(),
        serde_json::to_string(part.label()).unwrap_or_default(),
        cal.fy_start_month,
        cal.week_start.from_monday()
    )
}

/// Python mirror of `dates::calendar_rank` and `dates::parse_range` (the Calendar and Ranges orders, Phase 8
/// W0). Plain Python, so the tests run it without Polars. Keep the two in step.
pub const PY_CALENDAR_RANGES_HELPER: &str = r#"_MP_MONTHS = ["jan", "feb", "mar", "apr", "may", "jun", "jul", "aug", "sep", "oct", "nov", "dec"]
_MP_MONTHS_LONG = ["january", "february", "march", "april", "may", "june", "july", "august", "september",
                   "october", "november", "december"]
_MP_DAYS = ["mon", "tue", "wed", "thu", "fri", "sat", "sun"]
_MP_DAYS_LONG = ["monday", "tuesday", "wednesday", "thursday", "friday", "saturday", "sunday"]


def _mp_month(t):
    if t == "sept":
        return 9
    for i in range(12):
        if t == _MP_MONTHS[i] or t == _MP_MONTHS_LONG[i]:
            return i + 1
    return None


def _mp_day(t):
    """Days since Monday of a day name, or None."""
    for i in range(7):
        if t == _MP_DAYS[i] or t == _MP_DAYS_LONG[i]:
            return i
    return None


def _mp_small(t):
    t = t.strip()
    if 1 <= len(t) <= 2 and t.isascii() and t.isdigit():
        return int(t)
    return None


def _mp_year4(t):
    t = t.strip()
    if len(t) == 4 and t.isascii() and t.isdigit():
        return int(t)
    return None


def _mp_calendar_rank(v, week_start):
    """Calendar order: month names, day names from the week start (days since Monday), Q1-Q4, FQ1-FQ4,
    W01-W54, 'Aug 2026', '2026 Q1', 'FY2026', 'FY2026-27'. None for anything else."""
    t = v.strip().lower()
    if not t:
        return None
    m = _mp_month(t)
    if m is not None:
        return float(m)
    d = _mp_day(t)
    if d is not None:
        return 100.0 + (d + 7 - week_start) % 7
    if t.startswith("fq") and _mp_small(t[2:]) is not None:
        n = _mp_small(t[2:])
        return 300.0 + n if 1 <= n <= 4 else None
    if t.startswith("q") and _mp_small(t[1:]) is not None:
        n = _mp_small(t[1:])
        return 200.0 + n if 1 <= n <= 4 else None
    if t.startswith("w") and _mp_small(t[1:]) is not None:
        n = _mp_small(t[1:])
        return 400.0 + n if 1 <= n <= 54 else None
    if t.startswith("fy"):
        rest = t[2:].strip()
        i = 0
        while i < len(rest) and rest[i].isascii() and rest[i].isdigit():
            i += 1
        y = _mp_year4(rest[:i])
        if y is None:
            return None
        tail = rest[i:].strip()
        if not tail:
            return 3000000.0 + y
        if tail[:1] not in ("-", "–", "/"):
            return None
        end = tail[1:].strip()
        ok = (len(end) == 2 and end.isascii() and end.isdigit() and int(end) == (y + 1) % 100) or \
             (len(end) == 4 and end.isascii() and end.isdigit() and int(end) == y + 1)
        return 3000000.0 + y if ok else None
    w = t.split()
    if len(w) == 2:
        m, y = _mp_month(w[0]), _mp_year4(w[1])
        if m is not None and y is not None:
            return 1000000.0 + y * 100 + m
        y = _mp_year4(w[0])
        q = _mp_small(w[1][1:]) if w[1].startswith("q") else None
        if y is not None and q is not None and 1 <= q <= 4:
            return 2000000.0 + y * 10 + q
    return None


_MP_UNITS = {"%": 1.0, "k": 1e3, "thousand": 1e3, "thousands": 1e3, "l": 1e5, "lakh": 1e5, "lakhs": 1e5,
             "lac": 1e5, "lacs": 1e5, "m": 1e6, "mn": 1e6, "million": 1e6, "millions": 1e6, "cr": 1e7,
             "crore": 1e7, "crores": 1e7, "bn": 1e9, "billion": 1e9, "billions": 1e9, "d": 1.0, "day": 1.0,
             "days": 1.0, "w": 7.0, "wk": 7.0, "wks": 7.0, "week": 7.0, "weeks": 7.0, "mo": 365.0 / 12.0,
             "mos": 365.0 / 12.0, "mon": 365.0 / 12.0, "month": 365.0 / 12.0, "months": 365.0 / 12.0,
             "y": 365.0, "yr": 365.0, "yrs": 365.0, "year": 365.0, "years": 365.0}
_MP_RANGE_PREFIX = [("below", 0), ("under", 0), ("less than", 0), ("up to", 0), ("upto", 0), ("<=", 0),
                    ("≤", 0), ("<", 0), ("above", 3), ("over", 3), ("more than", 3), (">=", 3),
                    ("≥", 3), (">", 3)]
_MP_RANGE_TAIL = [("and above", 3), ("& above", 3), ("or more", 3), ("and over", 3), ("+", 3),
                  ("and below", 0), ("& below", 0), ("or less", 0)]


def _mp_take_number(t):
    t = t.lstrip()
    neg = False
    if t[:1] in ("-", "−"):
        neg, t = True, t[1:]
    elif t[:1] == "+":
        t = t[1:]
    i = 0
    while i < len(t) and ((t[i].isascii() and t[i].isdigit()) or t[i] in ".,"):
        i += 1
    digits = t[:i]
    if not (digits[:1].isascii() and digits[:1].isdigit()):
        return None
    try:
        n = float(digits.replace(",", ""))
    except ValueError:
        return None
    if n != n or n in (float("inf"), float("-inf")):
        return None
    return (-n if neg else n, t[i:])


def _mp_take_unit(t):
    if t.startswith("%"):
        return (1.0, t[1:])
    s = t.lstrip()
    i = 0
    while i < len(s) and s[i].isalpha():
        i += 1
    if i == 0:
        return (None, t)
    u = _MP_UNITS.get(s[:i])
    return (u, s[i:]) if u is not None else (None, t)


def _mp_range_key(v):
    """Ranges order: (first number scaled by its unit, 0 Below / 1 exact / 2 range / 3 Above), or None."""
    t = v.strip().lower()
    if not t:
        return None
    kind = None
    for p, k in _MP_RANGE_PREFIX:
        if t.startswith(p):
            r = t[len(p):]
            if all(c.isalpha() or c == " " for c in p) and r[:1].isalpha():
                continue
            t, kind = r, k
            break
    got = _mp_take_number(t)
    if got is None:
        return None
    first, rest = got
    unit, rest = _mp_take_unit(rest)
    rest = rest.lstrip()
    second = False
    if kind is None:
        after = None
        for sep in ("–", "—", "-", "to "):
            if rest.startswith(sep):
                after = rest[len(sep):]
                break
        if after is None and rest.startswith("to") and (len(rest) == 2 or rest[2] == " "):
            after = rest[2:]
        if after is not None:
            got2 = _mp_take_number(after)
            if got2 is not None:
                unit2, r3 = _mp_take_unit(got2[1])
                if unit is None:
                    unit = unit2
                rest = r3.lstrip()
                second = True
    tail = None
    if not second and kind is None:
        for suffix, k in _MP_RANGE_TAIL:
            if rest == suffix:
                tail, rest = k, ""
                break
    if rest.strip():
        return None
    kv = kind if kind is not None else (tail if tail is not None else (2 if second else 1))
    return (first * (unit if unit is not None else 1.0), float(kv))


def _mp_range_rank(v):
    k = _mp_range_key(v)
    return None if k is None else k[0]


def _mp_range_sub(v):
    k = _mp_range_key(v)
    return None if k is None else k[1]


"#;

/// Python mirror of `engine::parse_banking_amount` (P1 rules), emitted when a column is typed
/// Float64 or Int64. Keep the two in step.
const PY_AMOUNT_HELPER: &str = r#"import re as _re
_MP_CURRENCY = ("\u09f3", "bdt", "tk.", "tk")
_MP_MARKERS = (("dr.", "Dr"), ("cr.", "Cr"), ("dr", "Dr"), ("cr", "Cr"))


def _mp_strip_currency(s):
    low = s.lower()
    for p in _MP_CURRENCY:
        if low.startswith(p):
            return s[len(p):].lstrip()
    return s


def _mp_amount(v, col, sign, acc, as_int=False):
    """Same rules as MicroParquet: currency prefix, Dr/Cr per sign convention, commas,
    (brackets) and trailing minus, "-" = 0, blank/NA/N/A/NULL = null. Anything else -> null."""
    if v is None:
        return None
    s = v.strip()
    if s.startswith("'"):
        s = s[1:].strip()
    if s == "" or s.lower() in ("na", "n/a", "null"):
        return None
    s = _mp_strip_currency(s)
    marker = None
    low = s.lower()
    for m, kind in _MP_MARKERS:
        if low.endswith(m):
            marker, s = kind, s[: len(s) - len(m)].strip()
            break
    else:
        for m, kind in _MP_MARKERS:
            if low.startswith(m):
                marker, s = kind, s[len(m):].strip()
                break
    if marker is not None and sign == "Unset":
        raise ValueError(
            f"Column '{col}' has Dr/Cr amounts. Choose 'Dr is +' or 'Cr is +' for it in Schema & Ingestion."
        )
    s = _mp_strip_currency(s.strip())
    if s == "-":
        return 0 if as_int else 0.0
    body = s.replace(",", "").strip()
    neg = False
    if acc:
        if body.startswith("(") and body.endswith(")"):
            body, neg = _mp_strip_currency(body[1:-1].strip()), True
        elif len(body) > 1 and body.endswith("-"):
            body, neg = body[:-1].strip(), True
    if body.startswith("-"):
        if neg:
            return None
        neg, body = True, body[1:]
    elif body.startswith("+"):
        body = body[1:]
    if not _re.fullmatch(r"[0-9]*\.?[0-9]*", body) or not _re.search(r"[0-9]", body):
        return None
    if marker is not None:
        if neg:
            return None
        neg = marker != ("Dr" if sign == "DrPositive" else "Cr")
    if as_int:
        if "." in body:
            return None
        x = int(body)
    else:
        x = float(body)
    return -x if neg else x


"#;

/// Python mirror of `engine::parse_flexible_banking_date` (P3 rules), emitted when a column is
/// typed as a date. Keep the two in step.
const PY_DATE_HELPER: &str = r#"import datetime as _dt
import re as _re
_MP_MONTHS = {
    "jan": 1, "january": 1, "feb": 2, "february": 2, "mar": 3, "march": 3,
    "apr": 4, "april": 4, "may": 5, "jun": 6, "june": 6, "jul": 7, "july": 7,
    "aug": 8, "august": 8, "sep": 9, "sept": 9, "september": 9, "oct": 10,
    "october": 10, "nov": 11, "november": 11, "dec": 12, "december": 12,
}


def _mp_digits(t):
    return t != "" and all("0" <= ch <= "9" for ch in t)


def _mp_ymd(y, m, d):
    try:
        return _dt.date(y, m, d)
    except (ValueError, OverflowError):
        return None


def _mp_date(v, mode):
    """Same rules as MicroParquet: D/M/Y or M/D/Y (by mode, swapping when the other reading
    is the only valid one), YYYY-MM-DD, month names (31-Oct-26), "." separators, trailing
    times, two-digit years 00-69 -> 20xx and 70-99 -> 19xx. Blank, "-", "0", NA -> null."""
    if v is None:
        return None
    s = v.strip().strip('"').strip("'").strip()
    if s in ("", "-", "0") or s.lower() in ("null", "none", "na", "n/a"):
        return None
    s = s.split(" ")[0]
    for i in range(1, len(s) - 1):
        if s[i] in "Tt" and _mp_digits(s[i - 1]) and _mp_digits(s[i + 1]):
            s = s[:i]
            break
    parts = _re.split(r"[/.\-]", s)
    if len(parts) < 3:
        return None
    a, b, c = (p.strip() for p in parts[:3])
    if not _mp_digits(a) or not _mp_digits(c):
        return None
    p1 = int(a)
    month = _MP_MONTHS.get(b.lower())
    if month is not None:
        p2 = month
    elif _mp_digits(b):
        p2 = int(b)
    else:
        return None
    p3 = int(c)
    if len(a) == 4:
        if not 1900 <= p1 <= 2200:
            return None
        m, d = (p2, p3) if p2 <= 12 else (p3, p2)
        return _mp_ymd(p1, m, d)
    y = int(c)
    if len(c) == 2:
        year = 2000 + y if y < 70 else 1900 + y
    elif len(c) == 4 and 1900 <= y <= 2200:
        year = y
    else:
        return None
    if month is not None:
        return _mp_ymd(year, p2, p1)
    if mode == "DateDMY":
        return _mp_ymd(year, p2, p1) or _mp_ymd(year, p1, p2)
    return _mp_ymd(year, p1, p2) or _mp_ymd(year, p2, p1)


"#;

/// Escapes text for a double-quoted Python string literal.
fn py_escape(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

/// Why the Python export is refused when columns are PII-masked.
pub fn python_pii_refusal(masked_columns: &[String]) -> String {
    format!(
        "Python export refused: column(s) {} are PII-masked. The app masks them with this install's secret key, which a Python script cannot reproduce, so the script would write the real names and account numbers. Remove the masks in Schema & Ingestion to export a script, or export the masked data from the app instead.",
        masked_columns.join(", ")
    )
}

/// The Python script for a single-file pipeline. Refused (error) when any column is PII-masked.
pub fn generate_python_polars_code(
    source_path: &Path,
    is_parquet: bool,
    ingestion: &IngestionConfig,
    steps: &[TransformStep],
    control_col: Option<&str>,
) -> Result<String> {
    let masked: Vec<String> = ingestion
        .columns
        .iter()
        .filter(|c| c.pii_mask != PiiMaskMode::None)
        .map(|c| c.name.clone())
        .collect();
    if !masked.is_empty() {
        anyhow::bail!(python_pii_refusal(&masked));
    }
    // a lookup field needs the model of a project (Phase 9 M1): a script has no links to follow
    if steps.iter().any(|s| !crate::model::step_links(s).is_empty()) {
        anyhow::bail!("{}", crate::model::PYTHON_REFUSAL);
    }
    let mut py = String::new();
    py.push_str("# ==========================================================================\n");
    py.push_str(&format!(
        "# Auto-Generated by MicroParquet v{} — Polars Out-of-Core Streaming Recipe\n",
        env!("CARGO_PKG_VERSION")
    ));
    py.push_str("# ==========================================================================\n");
    py.push_str("import polars as pl\nfrom pathlib import Path\n\n");

    let path_str = source_path.to_string_lossy().replace('\\', "/");
    py.push_str(&format!("source_path = Path(r\"{}\")\n\n", path_str));

    if is_parquet {
        py.push_str("lf = pl.scan_parquet(source_path)\n\n");
    } else {
        py.push_str("# 1. Lock Strict Schema Overrides (Preserves Leading Zeros on Banking IDs)\n");
        py.push_str("custom_schema = {\n");
        for c in &ingestion.columns {
            py.push_str(&format!("    \"{}\": pl.Utf8,\n", c.name));
        }
        py.push_str("}\n\n");
        // Same read settings as the app: separator, title lines above the header, encoding.
        let read_args = format!(
            "schema_overrides=custom_schema, try_parse_dates=False, separator={:?}, skip_rows={}",
            ingestion.separator, ingestion.skip_rows
        );
        match ingestion.encoding {
            CsvTextEncoding::Utf8 => py.push_str(&format!(
                "lf = pl.scan_csv(source_path, {}, encoding=\"utf8-lossy\")\n\n",
                read_args
            )),
            CsvTextEncoding::Windows1252 => py.push_str(&format!(
                "# Windows-1252 file: decoded while reading (scan_csv reads UTF-8 only)\n\
                 lf = pl.read_csv(source_path, {}, encoding=\"cp1252\", infer_schema_length=0).lazy()\n\n",
                read_args
            )),
        }

        if ingestion.sanitize_linebreaks {
            py.push_str("# 2. Sanitize Linebreaks (\\r, \\n) & Trim Whitespace Across All Text Columns\n");
            py.push_str("lf = lf.with_columns(\n    pl.col(pl.Utf8).str.replace_all(r\"[\\r\\n]+\", \" \").str.strip_chars()\n)\n\n");
        }
    }

    // Columns to type, each with the expression giving its text. CSV: every column (read as
    // text). Parquet: only columns whose type was changed from the file's own; non-text
    // columns are cast to text first, as in the app.
    let typed: Vec<(&ColumnSchemaConfig, String)> = ingestion
        .columns
        .iter()
        .filter_map(|c| {
            let name = py_escape(&c.name);
            if !is_parquet {
                return Some((c, format!("pl.col(\"{}\")", name)));
            }
            if !parquet_needs_conversion(c) || parquet_native_type(&c.source_dtype).is_none() {
                return None;
            }
            if c.source_dtype == "String" {
                Some((c, format!("pl.col(\"{}\")", name)))
            } else {
                Some((c, format!("pl.col(\"{}\").cast(pl.Utf8)", name)))
            }
        })
        .collect();
    {
        // Same typing as MicroParquet's ingestion: casts are not strict, so a value that cannot
        // be converted becomes null (as in the app), and the script prints how many per column.
        let mut casts: Vec<String> = Vec::new();
        let mut cast_cols: Vec<String> = Vec::new();
        if typed
            .iter()
            .any(|(c, _)| matches!(c.dtype, ColDataType::Float64 | ColDataType::Int64))
        {
            py.push_str("# Amount parsing rules (same as MicroParquet)\n");
            py.push_str(PY_AMOUNT_HELPER);
        }
        if typed.iter().any(|(c, _)| {
            matches!(
                c.dtype,
                ColDataType::DateDMY | ColDataType::DateMDY | ColDataType::DateISO
            )
        }) {
            py.push_str("# Date parsing rules (same as MicroParquet)\n");
            py.push_str(PY_DATE_HELPER);
        }
        let acc = if ingestion.parse_accounting_negatives { "True" } else { "False" };
        for (c, base) in &typed {
            let name = py_escape(&c.name);
            let sign = format!("{:?}", c.sign_convention);
            if c.dtype != ColDataType::Utf8 {
                cast_cols.push(format!("\"{}\"", name));
            }
            match c.dtype {
                // CSV text stays as read; a Parquet column changed to Text is cast.
                ColDataType::Utf8 => {
                    if is_parquet && c.source_dtype != "String" {
                        casts.push(base.clone());
                    }
                }
                ColDataType::Float64 => casts.push(format!(
                    "{b}.map_elements(lambda v: _mp_amount(v, \"{n}\", \"{s}\", {a}), return_dtype=pl.Float64)",
                    b = base,
                    n = name,
                    s = sign,
                    a = acc
                )),
                ColDataType::Int64 => casts.push(format!(
                    "{b}.map_elements(lambda v: _mp_amount(v, \"{n}\", \"{s}\", {a}, as_int=True), return_dtype=pl.Int64)",
                    b = base,
                    n = name,
                    s = sign,
                    a = acc
                )),
                ColDataType::DateDMY | ColDataType::DateMDY | ColDataType::DateISO => {
                    casts.push(format!(
                        "{}.map_elements(lambda v: _mp_date(v, \"{:?}\"), return_dtype=pl.Date)",
                        base, c.dtype
                    ))
                }
            }
        }
        if !casts.is_empty() {
            py.push_str("# 2b. Apply Ingestion Column Types (amounts parsed by _mp_amount)\n");
            py.push_str("# Casts are not strict: a value that cannot be converted becomes null, as in MicroParquet.\n");
            py.push_str(&format!("_cast_cols = [{}]\n", cast_cols.join(", ")));
            py.push_str("_nulls_before = lf.select([pl.col(c).is_null().sum() for c in _cast_cols]).collect().row(0)\n");
            py.push_str(&format!(
                "lf = lf.with_columns([\n    {},\n])\n",
                casts.join(",\n    ")
            ));
            py.push_str("_nulls_after = lf.select([pl.col(c).is_null().sum() for c in _cast_cols]).collect().row(0)\n");
            py.push_str("for _c, _b, _a in zip(_cast_cols, _nulls_before, _nulls_after):\n    print(f\"Cast {_c}: {_a - _b} value(s) could not be converted and became null\")\n\n");
        }
    }

    // The schema's "Rename to" names, applied after typing and before any step (Phase 6 U1).
    let renames = ingestion.rename_pairs();
    if !renames.is_empty() {
        py.push_str("# 2c. Rename columns (Schema & Ingestion, \"Rename to\")\n");
        py.push_str("lf = lf.rename({\n");
        for (old, new) in &renames {
            py.push_str(&format!("    \"{}\": \"{}\",\n", py_escape(old), py_escape(new)));
        }
        py.push_str("})\n\n");
    }

    // the helper of conditions on picked values (U7), when a step has one
    fn uses_values(step: &TransformStep) -> bool {
        match step.inner() {
            TransformStep::FilterGroup { conditions, .. } => conditions.iter().any(|c| c.uses_values()),
            TransformStep::ConditionalColumn { rules, .. } => rules.iter().any(|r| !r.values.is_empty() && !r.compare_is_col),
            _ => false,
        }
    }
    if steps.iter().any(uses_values) {
        py.push_str(PY_VALUES_HELPER);
    }

    py.push_str("# 3. Applied Transformation Pipeline Steps\n");
    for (idx, step) in steps.iter().enumerate() {
        py.push_str(&format!("# Step {}: {}\n", idx + 1, step.short_label()));
        match step {
            TransformStep::SelectAndReorderColumns { cols } => {
                let quoted: Vec<String> = cols.iter().map(|c| format!("\"{}\"", c)).collect();
                py.push_str(&format!("lf = lf.select([{}])\n\n", quoted.join(", ")));
            }
            TransformStep::RenameColumn { old_name, new_name } => {
                py.push_str(&format!(
                    "lf = lf.rename({{\"{}\": \"{}\"}})\n\n",
                    old_name, new_name
                ));
            }
            TransformStep::ConvertToNumber { columns } => {
                py.push_str("# Amount parsing rules (same as MicroParquet)\n");
                py.push_str(PY_AMOUNT_HELPER);
                let casts: Vec<String> = columns
                    .iter()
                    .map(|c| {
                        format!(
                            "pl.col(\"{c}\").cast(pl.Utf8).map_elements(lambda v: _mp_amount(v, \"{c}\", \"Unset\", True), return_dtype=pl.Float64)"
                        )
                    })
                    .collect();
                py.push_str(&format!("lf = lf.with_columns([\n    {},\n])\n\n", casts.join(",\n    ")));
            }
            TransformStep::FilterRows {
                column,
                operator,
                value,
                keep_matching,
            } => {
                let cond_py = py_filter_expr(column, *operator, value);
                if *keep_matching {
                    py.push_str(&format!("lf = lf.filter({})\n\n", cond_py));
                } else {
                    py.push_str(&format!("lf = lf.filter(~({}))\n\n", cond_py));
                }
            }
            TransformStep::Skipped { .. } => {
                py.push_str("# (turned off in MicroParquet: this step is skipped)\n\n");
            }
            TransformStep::SortRows { levels } => {
                if !levels.is_empty() {
                    if !py.contains("def _mp_natural_key") {
                        py.push_str(PY_NATURAL_KEY_HELPER);
                    }
                    // Blank text counts as blank (last), like the app; other text sorts by its
                    // natural key (case-insensitive, numbers by value). Other types sort as they are.
                    if levels.iter().any(|l| !l.order.is_natural()) && !py.contains("def _mp_rank_keys") {
                        py.push_str(PY_ORDER_HELPER);
                    }
                    if levels.iter().any(|l| matches!(l.order, crate::sort_lists::SortOrder::Calendar { .. } | crate::sort_lists::SortOrder::Ranges))
                        && !py.contains("def _mp_calendar_rank")
                    {
                        py.push_str(PY_CALENDAR_RANGES_HELPER);
                    }
                    let mut keys: Vec<String> = Vec::new();
                    let mut desc: Vec<&str> = Vec::new();
                    for l in levels {
                        match &l.order {
                            crate::sort_lists::SortOrder::Natural => {
                                keys.push(format!(
                                    "_mp_sort_key(pl.col(\"{c}\"), lf.collect_schema()[\"{c}\"])",
                                    c = l.column
                                ));
                                desc.push(if l.descending { "True" } else { "False" });
                            }
                            order => {
                                let rank_fn = match order {
                                    crate::sort_lists::SortOrder::Tenor => "_mp_tenor_days".to_string(),
                                    crate::sort_lists::SortOrder::List { values, case_insensitive, .. } => {
                                        let norm = |v: &str| if *case_insensitive { v.trim().to_lowercase() } else { v.trim().to_string() };
                                        let pairs: Vec<String> = values
                                            .iter()
                                            .enumerate()
                                            .map(|(i, v)| format!("{}: {}.0", serde_json::to_string(&norm(v)).unwrap_or_default(), i))
                                            .collect();
                                        let look = if *case_insensitive { "v.strip().lower()" } else { "v.strip()" };
                                        format!("lambda v, _r={{{}}}: _r.get({})", pairs.join(", "), look)
                                    }
                                    crate::sort_lists::SortOrder::Calendar { week_start } => {
                                        format!("lambda v: _mp_calendar_rank(v, {})", week_start.from_monday())
                                    }
                                    crate::sort_lists::SortOrder::Ranges => "_mp_range_rank, _mp_range_sub".to_string(),
                                    crate::sort_lists::SortOrder::Natural => String::new(),
                                };
                                keys.push(format!("*_mp_rank_keys(pl.col(\"{}\"), {})", l.column, rank_fn));
                                desc.push("False");
                                desc.push(if l.descending { "True" } else { "False" });
                                if matches!(order, crate::sort_lists::SortOrder::Ranges) {
                                    desc.push(if l.descending { "True" } else { "False" });
                                }
                                desc.push("False");
                            }
                        }
                    }
                    py.push_str(&format!(
                        "lf = lf.sort([{}], descending=[{}], nulls_last=True, maintain_order=True)\n\n",
                        keys.join(", "),
                        desc.join(", ")
                    ));
                }
            }
            TransformStep::FilterGroup { conditions, join, keep_matching } => {
                if !conditions.is_empty() {
                    // a condition on a date part (W2) tests a column of that part, made first and dropped after
                    let mut temps: Vec<String> = Vec::new();
                    let mut tested: Vec<FilterCondition> = Vec::new();
                    let mut refused: Option<String> = None;
                    for (i, c) in conditions.iter().enumerate() {
                        match &c.part {
                            Some(p) => {
                                if !py.contains("def _mp_date_part") {
                                    py.push_str(PY_DATE_PART_HELPER);
                                }
                                let name = format!("__mp_part{}", i);
                                if p.part.is_ordered() {
                                    // compared by the part's order (W9): a column of keys, and the values written as keys
                                    match crate::dates::keyed_condition(c, &name) {
                                        Ok(keyed) => {
                                            py.push_str(&format!("lf = lf.with_columns({}.alias(\"{}\"))\n", py_date_key_expr(&c.column, p.part, &p.calendar), name));
                                            tested.push(keyed);
                                        }
                                        Err(e) => {
                                            refused = Some(format!("Filter: {}", e));
                                            break;
                                        }
                                    }
                                } else if let Some(why) = crate::dates::unordered_refusal(c) {
                                    refused = Some(format!("Filter: {}", why));
                                    break;
                                } else {
                                    py.push_str(&format!("lf = lf.with_columns({}.alias(\"{}\"))\n", py_date_part_expr(&c.column, p.part, &p.calendar), name));
                                    tested.push(FilterCondition { column: name.clone(), part: None, ..c.clone() });
                                }
                                temps.push(name);
                            }
                            None => tested.push(c.clone()),
                        }
                    }
                    if let Some(why) = refused {
                        // the same refusal as the app, never a guessed comparison
                        py.push_str(&format!("raise ValueError({:?})\n\n", why));
                        continue;
                    }
                    let parts: Vec<String> = tested.iter().map(py_condition_expr).collect();
                    let sep = match join {
                        GroupJoin::And => " & ",
                        GroupJoin::Or => " | ",
                    };
                    let combined = parts.iter().map(|p| format!("({})", p)).collect::<Vec<_>>().join(sep);
                    // Unknown tests (null against a value) do not match.
                    let matches = format!("({}).fill_null(False)", combined);
                    if *keep_matching {
                        py.push_str(&format!("lf = lf.filter({})\n\n", matches));
                    } else {
                        py.push_str(&format!("lf = lf.filter(~{})\n\n", matches));
                    }
                    if !temps.is_empty() {
                        let quoted: Vec<String> = temps.iter().map(|t| format!("\"{}\"", t)).collect();
                        py.push_str(&format!("lf = lf.drop([{}])\n\n", quoted.join(", ")));
                    }
                }
            }
            TransformStep::ConditionalColumn {
                new_col,
                rules,
                fallback,
                output_as_float,
                blank_result,
            } => {
                if let Some(r) = rules.iter().find(|r| r.compare_is_col && r.op == FilterOp::InList) {
                    // the same refusal as the app; never a guessed comparison
                    py.push_str(&format!(
                        "raise ValueError({:?})\n\n",
                        format!("'is in list' cannot compare column '{}' with column '{}'.", r.col_name, r.compare_val.trim())
                    ));
                    continue;
                }
                if !rules.is_empty() {
                    let blank_value = match blank_result {
                        BlankRule::FallThrough => None,
                        BlankRule::Result(t) if t.trim().is_empty() => Some("pl.lit(None)".to_string()),
                        BlankRule::Result(t) if *output_as_float => Some(t.trim().to_string()),
                        BlankRule::Result(t) => Some(format!("pl.lit(\"{}\")", t.trim())),
                    };
                    let mut first = true;
                    let mut chain = String::new();
                    let mut push = |chain: &mut String, cond: String, out: String| {
                        if first {
                            chain.push_str(&format!("pl.when({}).then({})", cond, out));
                            first = false;
                        } else {
                            chain.push_str(&format!("\n    .when({}).then({})", cond, out));
                        }
                    };
                    for r in rules.iter() {
                        let col_blank = |c: &str| format!("pl.col(\"{}\").is_null() | (pl.col(\"{}\").cast(pl.Utf8).str.strip_chars() == \"\")", c, c);
                        if let Some(bv) = &blank_value {
                            if !matches!(r.op, FilterOp::IsNull | FilterOp::IsNotNull) {
                                let mut b = format!("({})", col_blank(&r.col_name));
                                if r.compare_is_col {
                                    b.push_str(&format!(" | ({})", col_blank(r.compare_val.trim())));
                                }
                                push(&mut chain, b, bv.clone());
                            }
                        }
                        let cond = if r.compare_is_col && !matches!(r.op, FilterOp::IsNull | FilterOp::IsNotNull) {
                            let sym = match r.op {
                                FilterOp::Eq => "==",
                                FilterOp::NotEq => "!=",
                                FilterOp::Gt => ">",
                                FilterOp::Gte => ">=",
                                FilterOp::Lt => "<",
                                FilterOp::Lte => "<=",
                                _ => "",
                            };
                            let (a, b) = (&r.col_name, r.compare_val.trim());
                            match r.op {
                                FilterOp::Contains => format!("pl.col(\"{a}\").str.contains(pl.col(\"{b}\"), literal=True)"),
                                FilterOp::StartsWith => format!("pl.col(\"{a}\").str.starts_with(pl.col(\"{b}\"))"),
                                _ => format!("pl.col(\"{a}\") {sym} pl.col(\"{b}\")"),
                            }
                        } else if !r.values.is_empty() && matches!(r.op, FilterOp::Eq | FilterOp::NotEq | FilterOp::InList | FilterOp::NotInList) {
                            py_values_expr(&r.col_name, &r.values, false, matches!(r.op, FilterOp::NotEq | FilterOp::NotInList))
                        } else {
                            py_filter_expr(&r.col_name, r.op, &r.compare_val)
                        };
                        let out = if *output_as_float {
                            r.output_val.clone()
                        } else {
                            format!("pl.lit(\"{}\")", r.output_val)
                        };
                        push(&mut chain, cond, out);
                    }
                    let fb = if *output_as_float {
                        fallback.clone()
                    } else {
                        format!("pl.lit(\"{}\")", fallback)
                    };
                    py.push_str(&format!(
                        "lf = lf.with_columns(\n    {}\n    .otherwise({})\n    .alias(\"{}\")\n)\n\n",
                        chain, fb, new_col
                    ));
                }
            }
            TransformStep::MathFormula {
                new_col,
                left_col,
                op,
                right_is_col,
                right_val_or_col,
            } => {
                let rhs = if *right_is_col {
                    format!("pl.col(\"{}\").cast(pl.Float64)", right_val_or_col)
                } else {
                    match crate::engine::parse_math_scalar(new_col, right_val_or_col) {
                        Ok(v) => format!("{:?}", v),
                        Err(e) => {
                            // Same error as the app; never a guessed number.
                            py.push_str(&format!(
                                "raise ValueError({:?})\n\n",
                                format!("{:#}", e)
                            ));
                            continue;
                        }
                    }
                };
                py.push_str(&format!(
                    "lf = lf.with_columns(\n    (pl.col(\"{}\").cast(pl.Float64) {} {}).alias(\"{}\")\n)\n\n",
                    left_col,
                    op.symbol(),
                    rhs,
                    new_col
                ));
            }
            TransformStep::NormalizeEntity {
                source_col,
                new_col,
            } => {
                py.push_str(&format!(
                    "lf = lf.with_columns(\n    pl.col(\"{}\").str.to_uppercase()\n      .str.replace_all(r\"[.,\\-()/]+\", \" \")\n      .str.replace_all(r\"\\bLIMITED\\b\", \"LTD\")\n      .str.replace_all(r\"\\bBANGLADESH\\b\", \"BD\")\n      .str.strip_chars()\n      .alias(\"{}\")\n)\n\n",
                    source_col, new_col
                ));
            }
            TransformStep::CleanLineBreaks { columns, trim_all } => {
                let list = if columns.is_empty() {
                    "[c for c, t in lf.collect_schema().items() if t == pl.String]".to_string()
                } else {
                    format!("[{}]", columns.iter().map(|c| format!("\"{}\"", py_escape(c))).collect::<Vec<_>>().join(", "))
                };
                if *trim_all {
                    // every value: line breaks to one space, spaces at the ends trimmed
                    py.push_str(&format!(
                        "for _c in {}:\n    lf = lf.with_columns(\n        pl.col(_c).str.replace_all(r\"[\\r\\n]+\", \" \").str.strip_chars(\" \").alias(_c)\n    )\n\n",
                        list
                    ));
                } else {
                    py.push_str(&format!(
                        "for _c in {}:\n    lf = lf.with_columns(\n        pl.when(pl.col(_c).str.contains(r\"[\\r\\n]\"))\n          .then(pl.col(_c).str.replace_all(r\"[\\r\\n]+\", \" \").str.strip_chars())\n          .otherwise(pl.col(_c)).alias(_c)\n    )\n\n",
                        list
                    ));
                }
            }
            TransformStep::MaturityBucket {
                mat_col,
                cutoff_date,
                bucket_def,
                new_col,
                ..
            } => {
                py.push_str(&format!(
                    "# Custom Maturity Bucketing ({}) relative to {}\n",
                    bucket_def.name, cutoff_date
                ));
                py.push_str(&format!(
                    "rem_days = (pl.col(\"{}\") - pl.lit(\"{}\").str.to_date(\"%Y-%m-%d\")).dt.total_days()\n",
                    mat_col, cutoff_date
                ));
                py.push_str(&format!(
                    "lf = lf.with_columns(\n    pl.when(pl.col(\"{}\").is_null()).then(pl.lit(\"{}\"))\n      .when(rem_days < 0).then(pl.lit(\"{}\"))",
                    mat_col, bucket_def.null_sight_label, bucket_def.overdue_label
                ));
                for r in &bucket_def.rules {
                    py.push_str(&format!(
                        "\n      .when(rem_days <= {}).then(pl.lit(\"{}\"))",
                        r.max_days, r.label
                    ));
                }
                py.push_str(&format!(
                    "\n      .otherwise(pl.lit(\"{}\"))\n      .alias(\"{}\")\n)\n\n",
                    bucket_def.overflow_label, new_col
                ));
            }
            TransformStep::RateBand { rate_col, band_def, new_col } => {
                let labels = match band_def.labels() {
                    Ok(l) => l,
                    Err(e) => {
                        // Same error as the app; never a guessed band.
                        py.push_str(&format!("raise ValueError({:?})\n\n", format!("{:#}", e)));
                        continue;
                    }
                };
                // labels are plain ASCII in the script: "\u2013" escapes for the en dash
                let lit = |t: &str| {
                    let esc: String = t.chars().map(|c| if c.is_ascii() { c.to_string() } else { format!("\\u{:04x}", c as u32) }).collect();
                    format!("pl.lit(\"{}\")", esc)
                };
                py.push_str(&format!("# Rate profile ({}): bands of the rate column, {}\n", band_def.name, if band_def.lower_inclusive { "[a, b)" } else { "(a, b]" }));
                py.push_str(&format!("_r = pl.col(\"{}\").cast(pl.Float64)\n", rate_col));
                py.push_str(&format!(
                    "lf = lf.with_columns(\n    pl.when(_r.is_null() | _r.is_nan()).then({})\n      .when(_r == 0).then({})\n      .when(_r < 0).then({})",
                    lit(&labels.blank),
                    lit(&labels.zero),
                    lit(&labels.below)
                ));
                for k in 1..labels.edges.len() {
                    let op = if band_def.lower_inclusive { "<" } else { "<=" };
                    py.push_str(&format!("\n      .when(_r {} {:?}).then({})", op, labels.edges[k], lit(&labels.bands[k - 1])));
                }
                py.push_str(&format!("\n      .otherwise({})\n      .alias(\"{}\")\n)\n\n", lit(&labels.above), new_col));
            }
            TransformStep::DurationAnalysis {
                bal_col,
                rate_col,
                mat_col,
                cutoff_date,
                compounding_freq,
                shock_bps,
                default_sight_years,
                side_col,
            } => {
                // Asset/liability sign for Delta EVE (same rule as banking::direction_sign)
                let side = side_col.as_deref().map(str::trim).filter(|s| !s.is_empty());
                let (sign_py, assumed_py) = match side {
                    Some(s) => {
                        py.push_str(&format!(
                            "_side = pl.col(\"{s}\").cast(pl.Utf8).str.strip_chars().str.to_uppercase()\n_sign = pl.when(_side.str.starts_with(\"A\")).then(1.0).when(_side.str.starts_with(\"L\") | _side.is_in([\"DEPOSIT\", \"BORROWING\"])).then(-1.0).otherwise(None)\n_bad = lf.select(pl.col(\"{s}\")).filter(_sign.is_null()).unique(maintain_order=True).head(10).collect()\nif _bad.height > 0:\n    raise ValueError(f\"Side column '{s}' has values that are neither asset nor liability: {{_bad.to_series().to_list()}}\")\n"
                        ));
                        ("_sign".to_string(), String::new())
                    }
                    None => (
                        "1.0".to_string(),
                        ",\n    pl.lit(\"ASSET (unspecified)\").alias(\"Direction_Assumed\")".to_string(),
                    ),
                };
                py.push_str(&format!(
                    "# IRRBB Duration & Delta EVE Analysis ({:+.0} bps)\nrem_days = (pl.col(\"{}\") - pl.lit(\"{}\").str.to_date(\"%Y-%m-%d\")).dt.total_days()\nT = pl.when(pl.col(\"{}\").is_null()).then({}).otherwise((rem_days / 365.0).clip(0.0027, 100.0))\ny = (pl.col(\"{}\").cast(pl.Float64).fill_null(0.0) / 100.0).clip(0.0001, 1.0)\nk = {:.1}\nmac_dur = pl.when(T <= 0.5).then(T).otherwise(((1.0 + y/k)/y) * (1.0 - (1.0 + y/k).pow(-T*k)))\nmod_dur = mac_dur / (1.0 + y/k)\nlf = lf.with_columns([\n    T.alias(\"Residual_Years_T\"),\n    mac_dur.round(4).alias(\"Macaulay_Duration_Yrs\"),\n    mod_dur.round(4).alias(\"Modified_Duration\"),\n    (pl.col(\"{}\") * mod_dur * 0.0001).round(2).alias(\"DV01_BPV_BDT\"),\n    (-pl.col(\"{}\") * mod_dur * ({} / 10000.0) * {}).round(2).alias(\"Delta_EVE_Impact_BDT\"){}\n])\n\n",
                    shock_bps, mat_col, cutoff_date, mat_col, default_sight_years, rate_col, *compounding_freq as f64, bal_col, bal_col, shock_bps, sign_py, assumed_py
                ));
            }
            TransformStep::GroupByPivot {
                group_cols, aggs, ..
            } => {
                let g_str: Vec<String> = group_cols.iter().map(|c| format!("\"{}\"", c)).collect();
                for a in aggs {
                    if a.func == AggFunc::WeightedAvg
                        && a.weight_col.as_deref().map_or(true, |w| w.trim().is_empty())
                    {
                        py.push_str(&format!(
                            "raise ValueError(\"Weighted average needs a weight column (metric '{}')\")\n",
                            a.alias
                        ));
                    }
                }
                let mut a_strs = Vec::new();
                for a in aggs {
                    let expr = match a.func {
                        AggFunc::Sum => {
                            format!("pl.col(\"{}\").sum().alias(\"{}\")", a.target_col, a.alias)
                        }
                        AggFunc::Count => {
                            format!("pl.col(\"{}\").count().alias(\"{}\")", a.target_col, a.alias)
                        }
                        AggFunc::Mean => {
                            format!("pl.col(\"{}\").mean().alias(\"{}\")", a.target_col, a.alias)
                        }
                        AggFunc::Min => {
                            format!("pl.col(\"{}\").min().alias(\"{}\")", a.target_col, a.alias)
                        }
                        AggFunc::Max => {
                            format!("pl.col(\"{}\").max().alias(\"{}\")", a.target_col, a.alias)
                        }
                        AggFunc::WeightedAvg => {
                            // Null-safe: only rows with both value and weight count; zero weight -> null;
                            // weights of mixed sign (positive and negative) -> null
                            let w = a.weight_col.as_deref().unwrap_or("").trim();
                            let v_py = format!("pl.col(\"{}\").cast(pl.Float64)", a.target_col);
                            let w_py = format!("pl.col(\"{}\").cast(pl.Float64)", w);
                            let mask = format!("({v_py}.is_not_null() & {w_py}.is_not_null())");
                            let den = format!("{w_py}.filter({mask}).sum()");
                            let pos = format!("(({w_py} > 0) & {mask}).cast(pl.Int64).sum()");
                            let neg = format!("(({w_py} < 0) & {mask}).cast(pl.Int64).sum()");
                            format!(
                                "pl.when(({pos} > 0) & ({neg} > 0)).then(None).when({den} == 0).then(None).otherwise(({v_py} * {w_py}).filter({mask}).sum() / {den}).alias(\"{}\")",
                                a.alias
                            )
                        }
                        AggFunc::PctOfTotal => {
                            format!("pl.col(\"{}\").sum().alias(\"{}\")", a.target_col, a.alias)
                        }
                    };
                    a_strs.push(expr);
                }
                py.push_str(&format!(
                    "lf = lf.group_by([{}]).agg([\n    {}\n])\n\n",
                    g_str.join(", "),
                    a_strs.join(",\n    ")
                ));
            }
            TransformStep::SplitDate { calendar, .. } => {
                if !py.contains("def _mp_date_part") {
                    py.push_str(PY_DATE_PART_HELPER);
                }
                let column = match step {
                    TransformStep::SplitDate { column, .. } => column.clone(),
                    _ => String::new(),
                };
                let exprs: Vec<String> = step
                    .split_date_names()
                    .into_iter()
                    .map(|(part, name)| format!("{}.alias({})", py_date_part_expr(&column, part, calendar), serde_json::to_string(&name).unwrap_or_default()))
                    .collect();
                py.push_str(&format!("lf = lf.with_columns([\n    {},\n])\n\n", exprs.join(",\n    ")));
            }
            TransformStep::SqlQuery { query } => {
                py.push_str(&format!(
                    "ctx = pl.SQLContext(self=lf)\nlf = ctx.execute(\"\"\"{}\"\"\")\n\n",
                    query
                ));
            }
            TransformStep::RateShockSim { .. }
            | TransformStep::FtpCurveMap { .. }
            | TransformStep::BankingHygieneAudit { .. }
            | TransformStep::LoanClassificationProvision { .. }
            | TransformStep::LoanLimitUtilization { .. }
            | TransformStep::ClassifyOverdue { .. }
            | TransformStep::IfrsEcl { .. }
            | TransformStep::PivotTable { .. } => {
                py.push_str(&format!(
                    "raise NotImplementedError(\"Step {} '{}' has no Python translation in this version; run it in MicroParquet\")\n\n",
                    idx + 1,
                    py_escape(&step.short_label())
                ));
            }
        }
    }

    if let Some(ctrl) = control_col {
        py.push_str(&format!(
            "# 4. Verify Reconciliation Control Total on '{}'\nprint(lf.select([pl.len().alias(\"rows\"), (pl.col(\"{}\").sum() / 1e7).alias(\"Crore\")]).collect())\n\n",
            ctrl, ctrl
        ));
    }

    py.push_str("# 5. Stream Output to Zstd-Compressed Parquet\n");
    py.push_str("lf.sink_parquet(source_path.with_name(f\"{source_path.stem}_output.parquet\"), compression=\"zstd\")\n");
    Ok(py)
}

/// Python for one filter-group condition. Like the single filter, the script cannot know the
/// column types: a value that is a number is compared as a number, anything else as text.
fn py_condition_expr(c: &FilterCondition) -> String {
    let name = &c.column;
    if c.uses_values() {
        let negate = matches!(c.op, CondOp::NotEquals | CondOp::NotInList);
        return py_values_expr(name, &c.values, c.or_blank, negate);
    }
    let lit_of = |v: &str| {
        let t = v.trim();
        if t.parse::<f64>().is_ok() {
            t.to_string()
        } else {
            format!("\"{}\"", t)
        }
    };
    match c.op {
        CondOp::Equals => py_filter_expr(name, FilterOp::Eq, &c.value),
        CondOp::NotEquals => py_filter_expr(name, FilterOp::NotEq, &c.value),
        CondOp::Gt => py_filter_expr(name, FilterOp::Gt, &c.value),
        CondOp::Gte => py_filter_expr(name, FilterOp::Gte, &c.value),
        CondOp::Lt => py_filter_expr(name, FilterOp::Lt, &c.value),
        CondOp::Lte => py_filter_expr(name, FilterOp::Lte, &c.value),
        CondOp::Contains => py_filter_expr(name, FilterOp::Contains, &c.value),
        CondOp::StartsWith => py_filter_expr(name, FilterOp::StartsWith, &c.value),
        CondOp::InList => py_filter_expr(name, FilterOp::InList, &c.value),
        CondOp::NotInList => py_filter_expr(name, FilterOp::NotInList, &c.value),
        CondOp::EndsWith => format!("pl.col(\"{}\").str.ends_with(\"{}\")", name, c.value.trim()),
        CondOp::Between => format!(
            "pl.col(\"{}\").is_between({}, {})",
            name,
            lit_of(&c.value),
            lit_of(&c.value2)
        ),
        CondOp::IsBlank => format!(
            "pl.col(\"{n}\").is_null() | (pl.col(\"{n}\").cast(pl.Utf8).str.strip_chars() == \"\")",
            n = name
        ),
        CondOp::IsNotBlank => format!(
            "~(pl.col(\"{n}\").is_null() | (pl.col(\"{n}\").cast(pl.Utf8).str.strip_chars() == \"\"))",
            n = name
        ),
    }
}

/// Python for a condition on picked values (the helper `_mp_in_values` reads the column's type).
fn py_values_expr(name: &str, values: &[String], or_blank: bool, negate: bool) -> String {
    let items: Vec<String> = values.iter().map(|v| serde_json::to_string(v).unwrap_or_default()).collect();
    format!(
        "_mp_in_values(\"{n}\", [{items}], lf.collect_schema()[\"{n}\"], {b}, {neg})",
        n = py_escape(name),
        items = items.join(", "),
        b = if or_blank { "True" } else { "False" },
        neg = if negate { "True" } else { "False" }
    )
}

/// Python mirror of the picked-values conditions (Phase 6 U7): numbers as numbers, dates as dates,
/// the rest as text; a blank never matches "in", and is unknown for "not in" unless blank was picked.
const PY_VALUES_HELPER: &str = r#"import datetime as _dt


def _mp_in_values(name, values, dtype, or_blank=False, negate=False):
    c = pl.col(name)
    if dtype.is_numeric():
        picked = c.cast(pl.Float64).is_in([float(v.replace(",", "")) for v in values])
    elif dtype == pl.Date:
        picked = c.is_in([_dt.date.fromisoformat(v.strip()) for v in values])
    else:
        picked = c.cast(pl.Utf8).is_in(values)
    blank = c.is_null() | (c.cast(pl.Utf8).str.strip_chars() == "") if dtype == pl.Utf8 else c.is_null()
    positive = picked.fill_null(False)
    if or_blank:
        positive = positive | blank
    if not negate:
        return positive
    if or_blank:
        return ~positive
    return pl.when(blank).then(None).otherwise(~positive)


"#;

fn py_filter_expr(col_name: &str, op: FilterOp, val: &str) -> String {
    let trimmed = val.trim();
    match op {
        FilterOp::NotInList => {
            let items: Vec<String> = trimmed.split(',').map(|s| format!("\"{}\"", s.trim())).collect();
            format!("~pl.col(\"{}\").is_in([{}])", col_name, items.join(", "))
        }
        FilterOp::IsNull => format!("pl.col(\"{}\").is_null()", col_name),
        FilterOp::IsNotNull => format!("pl.col(\"{}\").is_not_null()", col_name),
        FilterOp::Contains => format!("pl.col(\"{}\").str.contains(\"{}\")", col_name, trimmed),
        FilterOp::StartsWith => {
            format!("pl.col(\"{}\").str.starts_with(\"{}\")", col_name, trimmed)
        }
        FilterOp::InList => {
            let items: Vec<String> = trimmed
                .split(',')
                .map(|s| format!("\"{}\"", s.trim()))
                .collect();
            format!("pl.col(\"{}\").is_in([{}])", col_name, items.join(", "))
        }
        _ => {
            let rhs = if trimmed.parse::<f64>().is_ok() {
                trimmed.to_string()
            } else {
                format!("\"{}\"", trimmed)
            };
            let sym = match op {
                FilterOp::Eq => "==",
                FilterOp::NotEq => "!=",
                FilterOp::Gt => ">",
                FilterOp::Gte => ">=",
                FilterOp::Lt => "<",
                FilterOp::Lte => "<=",
                _ => "==",
            };
            format!("pl.col(\"{}\") {} {}", col_name, sym, rhs)
        }
    }
}
