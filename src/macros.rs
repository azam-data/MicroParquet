use crate::models::{
    parquet_native_type, parquet_needs_conversion, AggFunc, ColDataType, ColumnSchemaConfig,
    CsvTextEncoding, FilterOp, IngestionConfig, MacroRecipe, PiiMaskMode, TransformStep,
};
use anyhow::{Context, Result};
use std::path::Path;

pub fn save_macro_to_file(recipe: &MacroRecipe, path: &Path) -> Result<()> {
    let json = serde_json::to_string_pretty(recipe).context("Failed to serialize macro recipe")?;
    std::fs::write(path, json).context("Failed to write .mpq.json file")?;
    Ok(())
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

pub fn generate_python_polars_code(
    source_path: &Path,
    is_parquet: bool,
    ingestion: &IngestionConfig,
    steps: &[TransformStep],
    control_col: Option<&str>,
) -> String {
    let mut py = String::new();
    py.push_str("# ==========================================================================\n");
    py.push_str("# Auto-Generated by MicroParquet v1.2 — Polars Out-of-Core Streaming Recipe\n");
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

    if ingestion.columns.iter().any(|c| c.pii_mask != PiiMaskMode::None) {
        py.push_str("# WARNING: PII masking not reproduced\n");
        py.push_str("# (the app masks with a per-install key; this script keeps the original values)\n");
        py.push_str("print(\"WARNING: PII masking is not reproduced by this script; masked columns hold the original values.\")\n\n");
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
            TransformStep::ConditionalColumn {
                new_col,
                rules,
                fallback,
                output_as_float,
            } => {
                if !rules.is_empty() {
                    py.push_str("lf = lf.with_columns(\n    ");
                    for (i, r) in rules.iter().enumerate() {
                        let cond = py_filter_expr(&r.col_name, r.op, &r.compare_val);
                        let out = if *output_as_float {
                            r.output_val.clone()
                        } else {
                            format!("pl.lit(\"{}\")", r.output_val)
                        };
                        if i == 0 {
                            py.push_str(&format!("pl.when({}).then({})", cond, out));
                        } else {
                            py.push_str(&format!("\n    .when({}).then({})", cond, out));
                        }
                    }
                    let fb = if *output_as_float {
                        fallback.clone()
                    } else {
                        format!("pl.lit(\"{}\")", fallback)
                    };
                    py.push_str(&format!(
                        "\n    .otherwise({})\n    .alias(\"{}\")\n)\n\n",
                        fb, new_col
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
                            // Null-safe: only rows with both value and weight count; zero weight -> null
                            let w = a.weight_col.as_deref().unwrap_or("").trim();
                            let v_py = format!("pl.col(\"{}\").cast(pl.Float64)", a.target_col);
                            let w_py = format!("pl.col(\"{}\").cast(pl.Float64)", w);
                            let mask = format!("({v_py}.is_not_null() & {w_py}.is_not_null())");
                            let den = format!("{w_py}.filter({mask}).sum()");
                            format!(
                                "pl.when({den} == 0).then(None).otherwise(({v_py} * {w_py}).filter({mask}).sum() / {den}).alias(\"{}\")",
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
            | TransformStep::LoanLimitUtilization { .. } => {
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
    py
}

fn py_filter_expr(col_name: &str, op: FilterOp, val: &str) -> String {
    let trimmed = val.trim();
    match op {
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
