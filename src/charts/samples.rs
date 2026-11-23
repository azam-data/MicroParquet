//! Synthetic example tables of the tools' guide examples, as the tools print them. They feed the template
//! tests and the guide screenshots; no real data.

use super::templates::CurveInput;
use polars::prelude::*;

pub fn dates(name: &str, values: &[&str]) -> Series {
    let days: Vec<i32> = values
        .iter()
        .map(|v| chrono::NaiveDate::parse_from_str(v, "%Y-%m-%d").map(|d| (d - chrono::NaiveDate::from_ymd_opt(1970, 1, 1).unwrap()).num_days() as i32).unwrap_or(0))
        .collect();
    Series::new(name.into(), days).cast(&DataType::Date).expect("dates")
}

/// The Phase 4 V3 gap ladder as its tool's table has it: three buckets, the totals row, a -10% limit on the first.
pub fn ladder_table() -> DataFrame {
    DataFrame::new(vec![
        Series::new("Bucket".into(), ["0\u{2013}1 month", "1\u{2013}3 months", "Over 3 months", "Total", "Overdue assets (not in gap)"]),
        Series::new("Inflows".into(), [Some(100.0), Some(200.0), Some(0.0), Some(300.0), Some(0.0)]),
        Series::new("Outflows".into(), [Some(150.0), Some(50.0), Some(0.0), Some(200.0), None]),
        Series::new("Net gap".into(), [Some(-50.0), Some(150.0), Some(0.0), Some(100.0), None]),
        Series::new("Cumulative gap".into(), [Some(-50.0), Some(100.0), Some(100.0), None, None]),
        Series::new("Net gap % of total assets".into(), [Some(-16.67), Some(50.0), Some(0.0), None, None]),
        Series::new("Cumulative gap % of total assets".into(), [Some(-16.666_666_666_666_668), Some(33.333_333_333_333_336), Some(33.333_333_333_333_336), None, None]),
        Series::new("Cumulative gap limit %".into(), [Some(-10.0), None, None, None, None]),
        Series::new("Breach".into(), [Some("BREACH"), None, None, None, None]),
    ])
    .expect("ladder")
}

/// The CL migration matrix of the Phase 4 V2 example as the tool prints it (A1 STD to SMA 100, A2 SS to DF 50, A3 STD closed 30, A4 new STD 20).
pub fn cl_matrix() -> DataFrame {
    DataFrame::new(vec![
        Series::new("Previous grade".into(), ["STD", "SMA", "SS", "DF", "BL", "Previous month total", "New", "Total"]),
        Series::new("STD".into(), [0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 20.0, 20.0]),
        Series::new("SMA".into(), [100.0, 0.0, 0.0, 0.0, 0.0, 100.0, 0.0, 100.0]),
        Series::new("SS".into(), [0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0]),
        Series::new("DF".into(), [0.0, 0.0, 50.0, 0.0, 0.0, 50.0, 0.0, 50.0]),
        Series::new("BL".into(), [0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0]),
        Series::new("Closed/Repaid".into(), [30.0, 0.0, 0.0, 0.0, 0.0, 30.0, 0.0, 30.0]),
        Series::new("Total".into(), [130.0, 0.0, 50.0, 0.0, 0.0, 180.0, 20.0, 200.0]),
    ])
    .expect("matrix")
}

/// A bigger, synthetic migration for the guide screenshot: five grades by six columns.
pub fn cl_matrix_large() -> DataFrame {
    let rows = [
        ("STD", [930.0, 32.0, 9.0, 2.0, 1.0, 25.0]),
        ("SMA", [180.0, 615.0, 143.0, 21.0, 4.0, 37.0]),
        ("SS", [42.0, 30.0, 708.0, 169.0, 26.0, 25.0]),
        ("DF", [11.0, 5.0, 20.0, 794.0, 158.0, 12.0]),
        ("BL", [3.0, 0.0, 2.0, 6.0, 979.0, 10.0]),
    ];
    let col = |i: usize| rows.iter().map(|r| r.1[i]).collect::<Vec<f64>>();
    DataFrame::new(vec![
        Series::new("Previous grade".into(), rows.iter().map(|r| r.0).collect::<Vec<_>>()),
        Series::new("STD".into(), col(0)),
        Series::new("SMA".into(), col(1)),
        Series::new("SS".into(), col(2)),
        Series::new("DF".into(), col(3)),
        Series::new("BL".into(), col(4)),
        Series::new("Closed/Repaid".into(), col(5)),
        Series::new("Total".into(), rows.iter().map(|r| r.1.iter().sum::<f64>()).collect::<Vec<_>>()),
    ])
    .expect("matrix")
}

/// The IRRBB results of the guide (Tier 1 of 1,000,000), as the tool's table has them.
pub fn irrbb_results() -> DataFrame {
    let sc = ["Parallel up", "Parallel down", "Steepener", "Flattener", "Short up", "Short down", "Maximum loss", "Outlier (max loss \u{2265} 15% of Tier 1)", "Tier 1 capital", "Rule source"];
    let eve: Vec<Option<f64>> = vec![Some(-104_969.17), Some(125_684.16), Some(-21_723.32), Some(-2_309.54), Some(-44_832.32), Some(48_236.10), None, None, None, None];
    let loss: Vec<Option<f64>> = vec![Some(104_969.17), Some(0.0), Some(21_723.32), Some(2_309.54), Some(44_832.32), Some(0.0), Some(104_969.17), None, None, None];
    let pct: Vec<Option<f64>> = vec![Some(10.5), Some(0.0), Some(2.17), Some(0.23), Some(4.48), Some(0.0), Some(10.5), None, None, None];
    let nii: Vec<Option<f64>> = vec![Some(-24_000.0), Some(24_000.0), Some(19_500.0), Some(-24_000.0), Some(-30_000.0), Some(30_000.0), None, None, None, None];
    let note = ["", "", "", "", "", "", "Parallel up", "No", "1000000.00", "BRPD IRRBB guideline 2021, Table 2 shocks"];
    DataFrame::new(vec![
        Series::new("Scenario".into(), sc),
        Series::new("EVE change".into(), eve),
        Series::new("Loss".into(), loss),
        Series::new("Loss % of Tier 1".into(), pct),
        Series::new("\u{394}NII".into(), nii),
        Series::new("Note".into(), note),
    ])
    .unwrap()
}

/// The MoM acceptance numbers: prior interest 80.00, volume +8.00, rate +5.00, mix +0.50, current 93.50.
pub fn mom_table() -> DataFrame {
    DataFrame::new(vec![
        Series::new("Product".into(), ["TD"]),
        Series::new("Prev annual interest".into(), [80.0]),
        Series::new("Curr annual interest".into(), [93.5]),
        Series::new("Volume effect".into(), [8.0]),
        Series::new("Rate effect".into(), [5.0]),
        Series::new("Mix effect".into(), [0.5]),
    ])
    .unwrap()
}

/// A month-on-month variance with three products and a Total row (the NII bridge's second chart).
pub fn mom_table_keys() -> DataFrame {
    DataFrame::new(vec![
        Series::new("Product".into(), ["TD", "Savings", "Current", "Total"]),
        Series::new("Prev annual interest".into(), [80.0, 30.0, 5.0, 115.0]),
        Series::new("Curr annual interest".into(), [93.5, 28.0, 5.5, 127.0]),
        Series::new("Volume effect".into(), [8.0, -3.0, 0.4, 5.4]),
        Series::new("Rate effect".into(), [5.0, 1.0, 0.1, 6.1]),
        Series::new("Mix effect".into(), [0.5, 0.0, 0.0, 0.5]),
    ])
    .unwrap()
}

/// A month-on-month variance with a New key (FX) and a Closed key (Old): their whole change is "New / closed".
pub fn mom_table_new_closed() -> DataFrame {
    let f = |p: &[&str], b: &[f64], r: &[f64]| DataFrame::new(vec![Series::new("Product".into(), p), Series::new("Balance".into(), b), Series::new("Rate".into(), r)]).unwrap();
    let prev = f(&["TD", "Savings", "Old scheme"], &[100.0, 50.0, 20.0], &[8.0, 4.0, 6.0]);
    let curr = f(&["TD", "Savings", "FX"], &[120.0, 50.0, 30.0], &[8.5, 4.0, 5.0]);
    let spec = crate::mom::MomSpec { key_cols: vec!["Product".into()], bal_col: "Balance".into(), rate_col: Some("Rate".into()), ..Default::default() };
    crate::mom::build_mom(prev.lazy(), curr.lazy(), &spec).unwrap()
}

pub fn lcr_summary() -> DataFrame {
    DataFrame::new(vec![
        Series::new(
            "Item".into(),
            [
                "Level 1 assets (after factors)",
                "Level 2A assets (after factors)",
                "Level 2B assets (after factors)",
                "Adjustment for the Level 2B cap",
                "Adjustment for the Level 2 cap",
                "HQLA after caps",
                "LCR",
                "LCR minimum",
                "LCR status",
                "NSFR",
                "NSFR minimum",
                "NSFR status",
            ],
        ),
        Series::new("Value".into(), [Some(3_500.0), Some(1_000.0), Some(500.0), Some(0.0), Some(0.0), Some(5_000.0), Some(133.68), Some(100.0), None, Some(108.2), Some(100.0), None]),
        Series::new("Unit".into(), ["BDT", "BDT", "BDT", "BDT", "BDT", "BDT", "%", "%", "", "%", "%", ""]),
        Series::new("Note".into(), ["", "", "", "", "", "", "", "at least", "Meets the minimum", "", "at least", "Meets the minimum"]),
    ])
    .unwrap()
}

pub fn ecl_summary() -> DataFrame {
    DataFrame::new(vec![
        Series::new("Section".into(), ["Grade group", "Grade group", "Stage", "Stage", "Stage", "Stage"]),
        Series::new("Group".into(), ["Standard", "Total", "Stage 1", "Stage 2", "Stage 3", "Total"]),
        Series::new("Loans".into(), [8.0, 10.0, 6.0, 2.0, 2.0, 10.0]),
        Series::new("EAD".into(), [900.0, 1000.0, 700.0, 200.0, 100.0, 1000.0]),
        Series::new("ECL".into(), [10.0, 60.0, 7.0, 13.0, 40.0, 60.0]),
    ])
    .unwrap()
}

pub fn scorecard() -> DataFrame {
    DataFrame::new(vec![
        Series::new("Rule".into(), ["No blank Actnum", "Balance in range", "Unique key", "Control total", "Overall score"]),
        Series::new("Rows checked".into(), [Some(100i64), Some(100), Some(100), None, None]),
        Series::new("Rows failing".into(), [Some(0i64), Some(8), Some(3), None, None]),
        Series::new("Pass %".into(), [Some(100.0), Some(92.0), Some(97.0), None, Some(96.3)]),
        Series::new("Result".into(), ["", "", "", "Pass", "Amber"]),
    ])
    .unwrap()
}

pub fn classified() -> DataFrame {
    let (mut m, mut g, mut b) = (Vec::new(), Vec::new(), Vec::new());
    for (month, vals) in [("2026-08-31", [900.0, 50.0, 20.0, 20.0, 10.0]), ("2026-09-30", [890.0, 50.0, 25.0, 20.0, 15.0]), ("2026-10-31", [880.0, 50.0, 30.0, 22.0, 18.0])] {
        for (i, gr) in ["STD", "SMA", "SS", "DF", "BL"].iter().enumerate() {
            m.push(month);
            g.push(*gr);
            b.push(vals[i]);
        }
    }
    DataFrame::new(vec![dates("Month", &m), Series::new("Grade".into(), g), Series::new("Balance".into(), b)]).unwrap()
}

/// The classified loans of three months, the middle one with no loans at all (its NPL % is blank, not 0%).
pub fn classified_empty_month() -> DataFrame {
    let (mut m, mut g, mut b) = (Vec::new(), Vec::new(), Vec::new());
    for (month, vals) in [("2026-08-31", [900.0, 50.0, 20.0, 20.0, 10.0]), ("2026-09-30", [0.0; 5]), ("2026-10-31", [880.0, 50.0, 30.0, 22.0, 18.0])] {
        for (i, gr) in ["STD", "SMA", "SS", "DF", "BL"].iter().enumerate() {
            m.push(month);
            g.push(*gr);
            b.push(vals[i]);
        }
    }
    DataFrame::new(vec![dates("Month", &m), Series::new("Grade".into(), g), Series::new("Balance".into(), b)]).unwrap()
}

pub fn mix() -> DataFrame {
    let (mut m, mut p, mut b) = (Vec::new(), Vec::new(), Vec::new());
    for (month, vals) in [("2026-08", [500.0, 300.0, 200.0]), ("2026-09", [540.0, 305.0, 198.0]), ("2026-10", [600.0, 310.0, 195.0])] {
        for (i, pr) in ["Term deposit", "Savings", "Current"].iter().enumerate() {
            m.push(month);
            p.push(*pr);
            b.push(vals[i]);
        }
    }
    DataFrame::new(vec![Series::new("Month".into(), m), Series::new("Product".into(), p), Series::new("Balance".into(), b)]).unwrap()
}

pub fn rate_bands() -> DataFrame {
    DataFrame::new(vec![
        Series::new("Actnum".into(), ["L1", "L2", "L3", "L4", "L5"]),
        Series::new("Rate_Band".into(), ["09. 8%\u{2013}9%", "09. 8%\u{2013}9%", "08. 7%\u{2013}8%", "10. 9%\u{2013}10%", "No rate"]),
        Series::new("Balance".into(), [500.0, 250.0, 300.0, 100.0, 50.0]),
    ])
    .unwrap()
}

pub fn history(n: usize) -> DataFrame {
    let ends = ["2026-01-31", "2026-02-28", "2026-03-31", "2026-04-30", "2026-05-31", "2026-06-30", "2026-07-31", "2026-08-31"];
    let bal = [100.0, 104.0, 99.0, 108.0, 103.0, 111.0, 106.0, 115.0];
    DataFrame::new(vec![dates("Date", &ends[..n]), Series::new("Balance".into(), &bal[..n])]).unwrap()
}

pub fn curves() -> Vec<CurveInput> {
    let d = |s: &str| chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap();
    vec![
        CurveInput { date: d("2026-06-30"), points: vec![(30, 7.5), (90, 7.9), (365, 8.6), (1825, 9.4)] },
        CurveInput { date: d("2026-09-30"), points: vec![(30, 7.6), (90, 8.0), (365, 9.0), (1825, 9.9)] },
        CurveInput { date: d("2026-10-31"), points: vec![(30, 7.6), (90, 8.1), (365, 9.1), (1825, 10.2)] },
    ]
}

pub fn repricing() -> DataFrame {
    DataFrame::new(vec![
        Series::new("Repricing bucket".into(), ["0\u{2013}1m", "1\u{2013}3m", "3\u{2013}6m", "6\u{2013}12m", ">12m", "Total"]),
        Series::new("Assets".into(), [100.0, 200.0, 0.0, 0.0, 300.0, 600.0]),
        Series::new("Liabilities".into(), [-90.0, 0.0, 0.0, -80.0, 0.0, -170.0]),
        Series::new("Gap".into(), [10.0, 200.0, 0.0, -80.0, 300.0, 430.0]),
        Series::new("Cumulative gap".into(), [Some(10.0), Some(210.0), Some(210.0), Some(130.0), Some(430.0), None]),
        Series::new("\u{394}NII (BDT)".into(), [1.0, 2.0, 0.0, -3.0, 0.0, 0.0]),
    ])
    .unwrap()
}
