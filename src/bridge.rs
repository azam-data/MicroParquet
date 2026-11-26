//! Phase 11 B1: the variance bridge. A period change in interest (income, or cost of funds) split into a **volume**, a **mix** and a
//! **rate** effect that add up to the change exactly, from two periods' balances and interest by product (or branch, wing, segment):
//!
//! - volume = (total balance now - total balance before) x the average rate before;
//! - mix = what the move of balance between products changes, at each product's own rate before:
//!   (sum of balance now x rate before) - (total balance now x the average rate before);
//! - rate = what each product's change of rate changes, on the balance now: sum of balance now x (rate now - rate before).
//!
//! volume + mix + rate = interest now - interest before, with nothing left over. A product that is new (no balance before) is taken to
//! have had the average rate before, so its whole rate gap is in the rate effect (decision B-2). A product that has closed has no rate
//! now and nothing in the rate effect. The effects are in the unit of the interest columns (annual interest, as the MoM table has it).
//!
//! The Phase 6 "Mix effect" column of the MoM table is the joint effect (change of balance x change of rate) and is left as it was;
//! the numbers here are a different split, and this tab says which formulas it uses.

use anyhow::{bail, Result};
use polars::prelude::*;

/// One product in the two periods.
#[derive(Clone, Debug, PartialEq)]
pub struct Item {
    pub key: String,
    pub b0: f64,
    pub i0: f64,
    pub b1: f64,
    pub i1: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Vrm {
    pub open: f64,
    pub volume: f64,
    pub mix: f64,
    pub rate: f64,
    pub close: f64,
}

pub fn volume_rate_mix(items: &[Item]) -> Result<Vrm> {
    for it in items {
        if [it.b0, it.i0, it.b1, it.i1].iter().any(|v| !v.is_finite()) {
            bail!("Bridge: '{}' has a number that is not a number. Fix the data first.", it.key);
        }
        if it.b0 < 0.0 || it.b1 < 0.0 {
            bail!("Bridge: '{}' has a negative balance. Volume, rate and mix need balances of one sign: reverse the sign of the column in the schema (Reverse sign) and run it again.", it.key);
        }
        if it.b0 == 0.0 && it.i0 != 0.0 {
            bail!("Bridge: '{}' has interest in the earlier period but no balance, so its rate is not defined. Fix the balance or the interest.", it.key);
        }
        if it.b1 == 0.0 && it.i1 != 0.0 {
            bail!("Bridge: '{}' has interest in the later period but no balance, so its rate is not defined. Fix the balance or the interest.", it.key);
        }
    }
    let s0: f64 = items.iter().map(|i| i.b0).sum();
    let s1: f64 = items.iter().map(|i| i.b1).sum();
    let i0: f64 = items.iter().map(|i| i.i0).sum();
    let i1: f64 = items.iter().map(|i| i.i1).sum();
    if s0 <= 0.0 {
        bail!("Bridge: there is no balance in the earlier period, so there is no average rate to start from.");
    }
    let avg0 = i0 / s0;
    let rate0 = |it: &Item| if it.b0 > 0.0 { it.i0 / it.b0 } else { avg0 };
    let volume = (s1 - s0) * avg0;
    let mix = items.iter().map(|it| it.b1 * rate0(it)).sum::<f64>() - s1 * avg0;
    let rate = items.iter().map(|it| it.i1 - it.b1 * rate0(it)).sum::<f64>();
    let v = Vrm { open: i0, volume, mix, rate, close: i1 };
    // the identity is exact; a gap bigger than rounding means a bug, never something to hide
    let gap = (v.volume + v.mix + v.rate) - (i1 - i0);
    if gap.abs() > 1e-6 * (1.0 + i1.abs() + i0.abs()) {
        bail!("Bridge: the effects do not add up to the change (off by {:.2}). Please report this.", gap);
    }
    Ok(v)
}

fn column(df: &DataFrame, name: &str) -> Result<Vec<Option<f64>>> {
    let c = df.column(name).map_err(|_| anyhow::anyhow!("The variance table has no column '{}'.", name))?;
    Ok(c.cast(&DataType::Float64)?.f64()?.into_iter().collect())
}

pub const STEP: &str = "Step";
pub const INTEREST: &str = "Interest (BDT)";
pub const BALANCE: &str = "Balance (BDT)";

/// The bridge table of a MoM variance table (`keys` are the label of each row). With the interest columns it is Prior interest, Volume,
/// Mix, Rate, Current interest; without a rate it is the balance bridge: the balance before, the change of each key (the `top` largest,
/// the rest as "Other keys"), the balance now.
pub fn bridge_frame(df: &DataFrame, keys: &[String], top: usize) -> Result<DataFrame> {
    let (b0, b1) = (column(df, "Prev balance")?, column(df, "Curr balance")?);
    let need = |v: &Option<f64>, what: &str, k: &str| v.ok_or_else(|| anyhow::anyhow!("Bridge: '{}' has no {}. Run the comparison again.", k, what));
    if df.get_column_names().iter().any(|c| c.as_str() == "Prev annual interest") {
        let (i0, i1) = (column(df, "Prev annual interest")?, column(df, "Curr annual interest")?);
        let mut items = Vec::new();
        for n in 0..df.height() {
            let k = keys.get(n).cloned().unwrap_or_default();
            items.push(Item { b0: need(&b0[n], "earlier balance", &k)?, b1: need(&b1[n], "later balance", &k)?, i0: need(&i0[n], "earlier interest", &k)?, i1: need(&i1[n], "later interest", &k)?, key: k });
        }
        let v = volume_rate_mix(&items)?;
        let steps = ["Prior interest", "Volume", "Mix", "Rate", "Current interest"].map(String::from).to_vec();
        return Ok(DataFrame::new(vec![Series::new(STEP.into(), steps), Series::new(INTEREST.into(), vec![v.open, v.volume, v.mix, v.rate, v.close])])?);
    }
    let mut rows: Vec<(String, f64)> = Vec::new();
    let (mut s0, mut s1) = (0.0, 0.0);
    for n in 0..df.height() {
        let k = keys.get(n).cloned().unwrap_or_default();
        let (p, c) = (need(&b0[n], "earlier balance", &k)?, need(&b1[n], "later balance", &k)?);
        s0 += p;
        s1 += c;
        rows.push((k, c - p));
    }
    rows.sort_by(|a, b| b.1.abs().partial_cmp(&a.1.abs()).unwrap_or(std::cmp::Ordering::Equal).then_with(|| a.0.cmp(&b.0)));
    let mut steps: Vec<String> = vec!["Balance before".to_string()];
    let mut amounts: Vec<f64> = vec![s0];
    let mut other = 0.0;
    for (i, (k, ch)) in rows.iter().enumerate() {
        if i < top {
            steps.push(k.clone());
            amounts.push(*ch);
        } else {
            other += ch;
        }
    }
    if rows.len() > top {
        steps.push("Other keys".to_string());
        amounts.push(other);
    }
    steps.push("Balance now".to_string());
    amounts.push(s1);
    Ok(DataFrame::new(vec![Series::new(STEP.into(), steps), Series::new(BALANCE.into(), amounts)])?)
}
