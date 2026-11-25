//! M6: the guide of Phase 9 and the worked example. The nine new notes have the eight sections of `<guide_notes>` in order, every table
//! of an "Example" is the table the code gives (and names a test that exists), every picture a note names exists, the README lists
//! the notes, and the notes that M6 changed say what the code now does.
//!
//! The worked example is `tests/fixtures/phase9_model/`: five synthetic tabs (month-end accounts, a daily trial balance, a branch
//! master, a product master keyed by Wing + Product code, a GL map), a Calendar and five pivot tabs, as one project that points only
//! inside the fixture folder. Its tables are read from the headless export of that project, so they are what the window writes.

use crate::common::{dir, fixtures_dir, floats, strings};
use crate::p9_m1 as m1;
use crate::p9_m3::{cal_field, month_year, names as build_names, settings};
use chrono::Datelike as _;
use crate::p9_m5::{measure_value, plain_measure, pct};
use chrono::{Datelike, NaiveDate, Weekday};
use micro_parquet::dates::DatePart;
use micro_parquet::measures::{ColumnKind, ColumnTag, DayCount, NamedMeasure, RatioFormat, Sign, Term};
use micro_parquet::model::*;
use micro_parquet::model_calendar::date_link;
use micro_parquet::models::*;
use micro_parquet::project::*;
use polars::prelude::*;
use std::path::{Path, PathBuf};

// ------------------------------------------------------------------------------------------------ the worked example's files

pub fn fixture_root() -> PathBuf {
    fixtures_dir().join("phase9_model")
}

fn day(y: i32, m: u32, d: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(y, m, d).expect("a date")
}

/// The last day of each month from Jan 2025 to Oct 2026 (22 snapshots).
fn month_ends() -> Vec<NaiveDate> {
    (0..22)
        .map(|t| {
            let (y, m) = (2025 + t / 12, (t % 12) as u32 + 1);
            let next = if m == 12 { day(y + 1, 1, 1) } else { day(y, m + 1, 1) };
            next.pred_opt().expect("the day before")
        })
        .collect()
}

/// Branch code, name, region, size factor, and the factor of its current and savings accounts (so the CASA ratio differs by branch).
const BRANCHES: [(&str, &str, &str, f64, f64); 4] = [("BR01", "Gulshan", "Dhaka", 1.20, 1.30), ("BR02", "Motijheel", "Dhaka", 1.00, 1.00), ("BR03", "Agrabad", "Chattogram", 0.90, 0.80), ("BR04", "Khulna Main", "Khulna", 0.60, 0.50)];

/// Wing, Product code, product name, group, class, balance of the branch with factor 1 in Jan 2025 (Crore), growth a month (percent).
const PRODUCTS: [(&str, &str, &str, &str, &str, f64, f64); 8] = [
    ("Conventional", "CA01", "Current account", "CASA", "Deposit", 40.0, 1.0),
    ("Conventional", "SB01", "Savings account", "CASA", "Deposit", 60.0, 1.0),
    ("Conventional", "FD01", "Fixed deposit", "Term deposit", "Deposit", 120.0, 2.0),
    ("Conventional", "LN01", "Term loan", "Term loan", "Loan", 90.0, 3.0),
    ("Conventional", "LN02", "Working capital", "Working capital", "Loan", 50.0, 2.0),
    ("Islami", "CA01", "Al-Wadiah current", "CASA", "Deposit", 20.0, 1.0),
    ("Islami", "MD01", "Mudaraba term deposit", "Term deposit", "Deposit", 45.0, 2.0),
    ("Islami", "IN01", "Bai-Muajjal investment", "Investment (Islami)", "Loan", 35.0, 4.0),
];

fn accounts_csv() -> String {
    let mut out = String::from("Date,Account,Branch,Wing,Product code,Balance\n");
    for (t, d) in month_ends().iter().enumerate() {
        for (code, _, _, factor, casa) in BRANCHES {
            for (wing, pc, _, group, _, base, growth) in PRODUCTS {
                let mix = if group == "CASA" { casa } else { 1.0 };
                let balance = base * factor * mix * (100.0 + growth * t as f64) / 100.0;
                out.push_str(&format!("{},{}-{}-{},{},{},{},{:.2}\n", d.format("%Y-%m-%d"), code, &wing[..1], pc, code, wing, pc, balance));
            }
        }
    }
    out
}

/// No rows on Friday and Saturday, except the last day of a month: the closing is posted whatever the weekday.
fn is_weekend(d: NaiveDate) -> bool {
    matches!(d.weekday(), Weekday::Fri | Weekday::Sat) && d.succ_opt().is_some_and(|n| n.month() == d.month())
}

/// A trial balance a day (no rows on Friday and Saturday, but the month-end): loans, investments and deposits are balances; the income and expense
/// GLs hold the amount since 1 January (income as a credit, so negative) and start again each year.
fn gl_tb_csv() -> String {
    let mut out = String::from("Date,GL,Balance\n");
    let (mut ytd_loan, mut ytd_inv, mut ytd_dep) = (0.0f64, 0.0f64, 0.0f64);
    let mut d = day(2025, 1, 1);
    let epoch = d;
    while d <= day(2026, 10, 31) {
        let n = (d - epoch).num_days() as f64;
        let (loans, inv, dep) = (1000.0 + 0.8 * n, 400.0 + 0.2 * n, 1500.0 + n);
        if d.month() == 1 && d.day() == 1 {
            (ytd_loan, ytd_inv, ytd_dep) = (0.0, 0.0, 0.0);
        }
        // the rates of the year: loans 10.5% then 11%, investments 8% then 8.5%, deposits 5.5% then 6%
        let (rl, ri, rd) = if d.year() == 2025 { (0.105, 0.08, 0.055) } else { (0.11, 0.085, 0.06) };
        ytd_loan += loans * rl / 365.0;
        ytd_inv += inv * ri / 365.0;
        ytd_dep += dep * rd / 365.0;
        if !is_weekend(d) {
            let t = d.format("%Y-%m-%d");
            for (gl, v) in [("1101", loans), ("1201", inv), ("2101", dep), ("4101", -ytd_loan), ("4201", -ytd_inv), ("5101", ytd_dep)] {
                out.push_str(&format!("{t},{gl},{v:.2}\n"));
            }
        }
        d = d.succ_opt().expect("next day");
    }
    out
}

fn branch_csv() -> String {
    let mut out = String::from("Branch code,Branch name,Region\n");
    for (c, n, r, _, _) in BRANCHES {
        out.push_str(&format!("{c},{n},{r}\n"));
    }
    out
}

fn product_csv() -> String {
    let mut out = String::from("Wing,Code,Product name,Group,Class\n");
    for (w, c, n, g, k, _, _) in PRODUCTS {
        out.push_str(&format!("{w},{c},{n},{g},{k}\n"));
    }
    out
}

fn glmap_csv() -> String {
    "GL,Head\n1101,Loans\n1201,Investments\n2101,Deposits\n4101,Loan interest income\n4201,Investment interest income\n5101,Deposit interest expense\n".to_string()
}

fn data_files() -> Vec<(&'static str, String)> {
    vec![("Accounts.csv", accounts_csv()), ("GL_TB.csv", gl_tb_csv()), ("Branch_master.csv", branch_csv()), ("Product_master.csv", product_csv()), ("GL_map.csv", glmap_csv())]
}

// ------------------------------------------------------------------------------------------------ the model

fn link_branch() -> Link {
    Link { data_tab: "t1".into(), lookup_tab: "t3".into(), keys: vec![KeyPair { data: "Branch".into(), lookup: "Branch code".into() }], unmatched_value: None }
}

fn link_product() -> Link {
    Link { data_tab: "t1".into(), lookup_tab: "t4".into(), keys: vec![KeyPair { data: "Wing".into(), lookup: "Wing".into() }, KeyPair { data: "Product code".into(), lookup: "Code".into() }], unmatched_value: None }
}

fn link_gl() -> Link {
    Link { data_tab: "t2".into(), lookup_tab: "t5".into(), keys: vec![KeyPair { data: "GL".into(), lookup: "GL".into() }], unmatched_value: None }
}

fn branch_field(column: &str) -> ModelFieldRef {
    ModelFieldRef::new(&link_branch(), "Accounts \u{2192} Branch master", "Branch master", column)
}

fn product_field(column: &str) -> ModelFieldRef {
    ModelFieldRef::new(&link_product(), "Accounts \u{2192} Product master", "Product master", column)
}

fn gl_field(column: &str) -> ModelFieldRef {
    ModelFieldRef::new(&link_gl(), "GL TB \u{2192} GL map", "GL map", column)
}

fn calendar_field(tab: &str, title: &str, column: &str) -> ModelFieldRef {
    ModelFieldRef::new(&date_link(tab, "Date", "t6"), &format!("{title} \u{2192} Calendar"), "Calendar", column)
}

fn is(field: &ModelFieldRef, value: &str) -> FilterCondition {
    FilterCondition { column: field.name.clone(), op: CondOp::Equals, value: value.into(), lookup: Some(field.clone()), ..Default::default() }
}

fn base(name: &str, tab: &str, agg: TimeAgg, filter: FilterCondition, sign: Sign) -> NamedMeasure {
    NamedMeasure::Base { name: name.into(), tab: tab.into(), column: "Balance".into(), aggregation: agg, filters: vec![filter], sign, weight: None }
}

fn term(m: &str, sign: Sign) -> Term {
    Term { measure: m.into(), sign }
}

fn ratio(name: &str, tab: &str, num: Vec<Term>, den: Vec<Term>, annualise: Option<DayCount>) -> NamedMeasure {
    NamedMeasure::Ratio { name: name.into(), tab: tab.into(), numerator: num, denominator: den, annualise, format: RatioFormat::Percent }
}

pub fn measures() -> Vec<NamedMeasure> {
    let (group, class, head) = (product_field("Group"), product_field("Class"), gl_field("Head"));
    let a = Some(DayCount::Actual365);
    vec![
        base("CASA closing", "t1", TimeAgg::ClosingBalance, is(&group, "CASA"), Sign::Plus),
        base("Deposits closing", "t1", TimeAgg::ClosingBalance, is(&class, "Deposit"), Sign::Plus),
        base("Loans closing", "t1", TimeAgg::ClosingBalance, is(&class, "Loan"), Sign::Plus),
        ratio("CASA ratio", "t1", vec![term("CASA closing", Sign::Plus)], vec![term("Deposits closing", Sign::Plus)], None),
        ratio("ADR", "t1", vec![term("Loans closing", Sign::Plus)], vec![term("Deposits closing", Sign::Plus)], None),
        base("Loan interest", "t2", TimeAgg::PeriodAmount, is(&head, "Loan interest income"), Sign::Minus),
        base("Investment interest", "t2", TimeAgg::PeriodAmount, is(&head, "Investment interest income"), Sign::Minus),
        base("Deposit interest", "t2", TimeAgg::PeriodAmount, is(&head, "Deposit interest expense"), Sign::Plus),
        base("Average loans", "t2", TimeAgg::AverageBalance, is(&head, "Loans"), Sign::Plus),
        base("Average investments", "t2", TimeAgg::AverageBalance, is(&head, "Investments"), Sign::Plus),
        base("Average deposits", "t2", TimeAgg::AverageBalance, is(&head, "Deposits"), Sign::Plus),
        ratio("Yield on loans", "t2", vec![term("Loan interest", Sign::Plus)], vec![term("Average loans", Sign::Plus)], a),
        ratio("Cost of deposits", "t2", vec![term("Deposit interest", Sign::Plus)], vec![term("Average deposits", Sign::Plus)], a),
        ratio(
            "NIM",
            "t2",
            vec![term("Loan interest", Sign::Plus), term("Investment interest", Sign::Plus), term("Deposit interest", Sign::Minus)],
            vec![term("Average loans", Sign::Plus), term("Average investments", Sign::Plus)],
            a,
        ),
    ]
}

pub fn model() -> Model {
    Model {
        links: vec![link_branch(), link_product(), link_gl()],
        calendar: Some(ModelCalendar { settings: settings(1), date_links: vec![DateLink { data_tab: "t1".into(), column: "Date".into() }, DateLink { data_tab: "t2".into(), column: "Date".into() }], tab: "t6".into() }),
        column_kinds: vec![ColumnKind { tab: "t1".into(), column: "Balance".into(), kind: ColumnTag::Balance }],
        measures: measures(),
        ..Default::default()
    }
}

// ------------------------------------------------------------------------------------------------ the five pivot tabs

fn order_of(f: &ModelFieldRef, part: DatePart) -> Vec<PivotFieldOrder> {
    part.sort_order(&settings(1)).map(|o| vec![PivotFieldOrder { field: f.name.clone(), order: o }]).unwrap_or_default()
}

fn months(f: &ModelFieldRef, labels: &[&str]) -> PivotFilter {
    PivotFilter { field: f.name.clone(), values: labels.iter().map(|l| Some(l.to_string())).collect() }
}

fn tv(shift: TimeShift, show: TimeShow, window: TimeWindow) -> TimeValue {
    TimeValue { window, shift, show, ..Default::default() }
}

const LAST3: [&str; 3] = ["Aug 2026", "Sep 2026", "Oct 2026"];

fn pivot_steps() -> Vec<(&'static str, &'static str, PivotSpec)> {
    let (my1, my2) = (calendar_field("t1", "Accounts", "Month-Year"), calendar_field("t2", "GL TB", "Month-Year"));
    let (prior, sply) = (TimeShift::PriorPeriod, TimeShift::Sply);
    let (diff, pcent, val, period, ytd) = (TimeShow::Difference, TimeShow::PercentChange, TimeShow::Value, TimeWindow::Period, TimeWindow::Ytd);
    let region = branch_field("Region");
    let group = product_field("Group");
    let class = product_field("Class");
    let casa_by_month = PivotSpec {
        rows: vec![my1.name.clone()],
        values: vec![plain_measure("CASA ratio"), measure_value("CASA ratio", tv(prior, diff, period)), measure_value("CASA ratio", tv(sply, diff, period))],
        filters: vec![months(&my1, &LAST3)],
        model_fields: vec![my1.clone()],
        field_orders: order_of(&my1, DatePart::MonthYear),
        data_tab: "t1".into(),
        ..Default::default()
    };
    let casa_by_region = PivotSpec {
        rows: vec![region.name.clone()],
        columns: vec![my1.name.clone()],
        values: vec![plain_measure("CASA ratio"), plain_measure("CASA closing"), plain_measure("Deposits closing"), measure_value("CASA ratio", tv(sply, diff, period))],
        filters: vec![months(&my1, &["Oct 2026"])],
        column_totals: true,
        model_fields: vec![region.clone(), my1.clone()],
        field_orders: order_of(&my1, DatePart::MonthYear),
        data_tab: "t1".into(),
        ..Default::default()
    };
    let margins = PivotSpec {
        rows: vec![my2.name.clone()],
        values: vec![plain_measure("Yield on loans"), plain_measure("Cost of deposits"), plain_measure("NIM"), measure_value("NIM", tv(sply, diff, period)), measure_value("NIM", tv(TimeShift::None, val, ytd))],
        filters: vec![months(&my2, &LAST3)],
        model_fields: vec![my2.clone()],
        field_orders: order_of(&my2, DatePart::MonthYear),
        data_tab: "t2".into(),
        ..Default::default()
    };
    let loans = PivotSpec {
        rows: vec![group.name.clone()],
        columns: vec![my1.name.clone()],
        values: vec![plain_measure("Loans closing"), measure_value("Loans closing", tv(sply, pcent, period))],
        filters: vec![months(&my1, &["Oct 2026"]), PivotFilter { field: class.name.clone(), values: vec![Some("Loan".into())] }],
        column_totals: true,
        model_fields: vec![group.clone(), class.clone(), my1.clone()],
        field_orders: order_of(&my1, DatePart::MonthYear),
        data_tab: "t1".into(),
        ..Default::default()
    };
    let adr = PivotSpec {
        rows: vec![my1.name.clone()],
        values: vec![plain_measure("ADR"), measure_value("ADR", tv(sply, diff, period))],
        filters: vec![months(&my1, &LAST3)],
        model_fields: vec![my1.clone()],
        field_orders: order_of(&my1, DatePart::MonthYear),
        data_tab: "t1".into(),
        ..Default::default()
    };
    vec![("CASA ratio by month", "Accounts.csv", casa_by_month), ("CASA ratio by region", "Accounts.csv", casa_by_region), ("Margins", "GL_TB.csv", margins), ("Loans by product group", "Accounts.csv", loans), ("ADR", "Accounts.csv", adr)]
}

fn csv_source(path: &Path) -> TabSource {
    TabSource::SingleFile { path: path.to_path_buf(), is_parquet: false, config: micro_parquet::engine::auto_detect_smart_schema(path).expect("a schema") }
}

fn snap(id: u64, key: &str, title: &str, source: TabSource, steps: Vec<TransformStep>) -> TabSnapshot {
    let n = steps.len();
    TabSnapshot { id, key: Some(key.into()), title: title.into(), source, steps, exec_steps: n, col_formats: Default::default(), summary: Default::default(), control_col: None, chart: None, chart_tab: None, kept: None }
}

fn snapshots(data: &Path) -> Vec<TabSnapshot> {
    let mut snaps = vec![
        snap(1, "t1", "Accounts", csv_source(&data.join("Accounts.csv")), vec![]),
        snap(2, "t2", "GL TB", csv_source(&data.join("GL_TB.csv")), vec![]),
        snap(3, "t3", "Branch master", csv_source(&data.join("Branch_master.csv")), vec![]),
        snap(4, "t4", "Product master", csv_source(&data.join("Product_master.csv")), vec![]),
        snap(5, "t5", "GL map", csv_source(&data.join("GL_map.csv")), vec![]),
        snap(6, "t6", "Calendar", TabSource::Calendar { settings: settings(1), first_year: 2025, last_year: 2027 }, vec![]),
    ];
    for (i, (title, file, spec)) in pivot_steps().into_iter().enumerate() {
        snaps.push(snap(7 + i as u64, &format!("t{}", 7 + i), title, csv_source(&data.join(file)), vec![TransformStep::from_pivot_spec(spec)]));
    }
    snaps
}

fn paths_in(root: &Path) -> PortablePaths {
    PortablePaths { root_dir: root.to_path_buf(), macros_dir: root.join("m"), schemas_dir: root.join("s"), curves_dir: root.join("c"), buckets_file: root.join("b.json"), temp_dir: root.join("temp") }
}

/// Writes the fixture: the five files and the project. Run once with `cargo test --test phase3 m6_write_the_worked_example -- --ignored`.
#[test]
#[ignore]
fn m6_write_the_worked_example_fixture() {
    let root = fixture_root();
    std::fs::create_dir_all(root.join("data")).expect("data folder");
    std::fs::create_dir_all(root.join("proj")).expect("project folder");
    for (name, text) in data_files() {
        std::fs::write(root.join("data").join(name), text).expect("a data file");
    }
    let scratch = dir("p9m6_write");
    let pfile = root.join("proj/monthly.mpproj");
    let mut cache = HashCache::new();
    let mut saved = build_project(&snapshots(&root.join("data")), Some(7), &project_dir_of(&pfile), &paths_in(&scratch), &mut cache).expect("builds");
    saved.project.model = model();
    write_project(&pfile, &saved.project).expect("writes");
}

// ------------------------------------------------------------------------------------------------ the tables of the project

/// The fixture opened from a copy, exported headless: the pivot tabs by title.
pub fn project_tables(name: &str) -> Vec<(String, DataFrame)> {
    let work = dir(name);
    let copy = work.join("phase9_model");
    copy_dir(&fixture_root(), &copy);
    let out = work.join("out");
    std::fs::create_dir_all(&out).expect("out");
    let written = export_project_headless(&copy.join("proj/monthly.mpproj"), &out, &paths_in(&work)).unwrap_or_else(|e| panic!("headless: {e:#}"));
    let mut tables = Vec::new();
    for (title, _, _) in pivot_steps() {
        let file = written.iter().find(|p| p.file_name().is_some_and(|n| n.to_string_lossy().starts_with(title))).unwrap_or_else(|| panic!("the tab '{title}' in {written:?}"));
        tables.push((title.to_string(), LazyFrame::scan_parquet(file, Default::default()).expect("scan").collect().expect("the tab")));
    }
    tables
}

fn copy_dir(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).expect("folder");
    for e in std::fs::read_dir(from).expect("fixture folder").flatten() {
        let (p, q) = (e.path(), to.join(e.file_name()));
        if p.is_dir() {
            copy_dir(&p, &q);
        } else {
            std::fs::copy(&p, &q).expect("copy");
        }
    }
}

/// A cell of a table: a ratio (a heading with ADR, CASA ratio, Yield, Cost or NIM) is a percentage, "(pp)" in points.
fn cell(heading: &str, v: Option<f64>) -> String {
    let Some(v) = v else { return "(blank)".to_string() };
    // rates are stored in percent: 9.5 is 9.5%
    let ratio = ["ADR", "CASA ratio", "Yield", "Cost of", "NIM"].iter().any(|r| heading.contains(r));
    if heading.starts_with("Count") {
        format!("{:.0}", v)
    } else if heading.contains("(pp)") {
        format!("{:.2} pp", v)
    } else if heading.contains("vs SPLY %") || ratio {
        format!("{:.2}%", v)
    } else {
        micro_parquet::models::format_with_commas(v, 2)
    }
}

pub fn md(head: &[String], rows: &[Vec<String>], right_from: usize) -> String {
    let rule: Vec<&str> = (0..head.len()).map(|i| if i >= right_from { "---:" } else { "---" }).collect();
    // a heading with a column key ("Oct 2026 | CASA ratio") would cut the table: the bar is written as \|
    let head: Vec<String> = head.iter().map(|h| h.replace(" | ", " \\| ")).collect();
    let mut out = format!("| {} |\n| {} |", head.join(" | "), rule.join(" | "));
    for r in rows {
        out.push_str(&format!("\n| {} |", r.join(" | ")));
    }
    out
}

pub fn frame_table(df: &DataFrame) -> String {
    let cols: Vec<String> = df.get_column_names().iter().map(|n| n.to_string()).collect();
    let labels = strings(df, &cols[0]);
    let nums: Vec<Vec<Option<f64>>> = cols[1..].iter().map(|c| floats(df, c)).collect();
    let rows: Vec<Vec<String>> = (0..df.height()).map(|r| std::iter::once(labels[r].clone().unwrap_or_default()).chain(cols[1..].iter().enumerate().map(|(i, c)| cell(c, nums[i][r]))).collect()).collect();
    md(&cols, &rows, 1)
}

#[test]
fn m6_the_worked_example_files_are_the_ones_the_generator_writes_and_point_only_inside_the_fixture() {
    let root = fixture_root();
    for (name, text) in data_files() {
        let on_disk = std::fs::read_to_string(root.join("data").join(name)).unwrap_or_else(|e| panic!("{name}: {e}"));
        assert!(on_disk.replace("\r\n", "\n") == text, "{name}: differs from what the generator writes (run the ignored writer)");
    }
    let project = std::fs::read_to_string(root.join("proj/monthly.mpproj")).expect("the project");
    let mut paths = 0;
    for line in project.lines().filter(|l| l.contains("\"path\":") || l.contains("\"rel_path\":")) {
        paths += 1;
        let v = line.split('"').nth(3).unwrap_or("");
        assert!(v.contains("tests/fixtures/phase9_model/data/") || v.starts_with("../data/"), "a path outside the fixtures: {v}");
    }
    assert!(paths >= 10, "{paths} paths in the project");
    let p = load_project(&root.join("proj/monthly.mpproj")).expect("opens");
    assert_eq!(p.model, model(), "the model in the file is the one the generator makes");
    assert_eq!(p.tabs.len(), 11);
    // the five tabs are the ones the owner named, and the lookups are as small as they should be
    assert_eq!(p.model.all_links().len(), 5, "3 links and 2 date links to the Calendar");
    let accounts = std::fs::read_to_string(root.join("data/Accounts.csv")).expect("accounts");
    assert_eq!(accounts.lines().count(), 1 + 22 * 4 * 8);
    assert_eq!(std::fs::read_to_string(root.join("data/Branch_master.csv")).expect("branches").lines().count(), 5);
}

#[test]
fn m6_dump_the_tables_of_the_worked_example() {
    let Ok(dump) = std::env::var("MP_GUIDE_DUMP") else { return };
    std::fs::create_dir_all(&dump).expect("dump folder");
    let all: Vec<String> = project_tables("p9m6_dump").iter().map(|(t, df)| format!("{t}\n{}", frame_table(df))).collect();
    std::fs::write(PathBuf::from(&dump).join("phase9_project.txt"), all.join("\n\n===\n\n")).expect("write");
}

// ------------------------------------------------------------------------------------------------ the small examples

fn value(field: &str, agg: PivotAgg) -> PivotValue {
    PivotValue { field: field.into(), agg, weight_col: None, time: None }
}

/// A plain pivot (no model) on a frame: its table.
fn plain_pivot_table(df: DataFrame, rows: &[&str], values: Vec<PivotValue>) -> String {
    let spec = PivotSpec { rows: rows.iter().map(|r| r.to_string()).collect(), values, row_totals: true, column_totals: true, ..Default::default() };
    frame_table(&micro_parquet::pivot_table::build_pivot(df.lazy(), &spec).expect("the pivot").df)
}

/// The pop-out builds the pivot the docked dialog builds: Wing by Sum of Balance on the nine accounts of the links example.
pub fn popout_table() -> String {
    plain_pivot_table(m1::accounts(), &["Wing"], vec![value("Balance", PivotAgg::Sum)])
}

pub fn nested_table() -> String {
    let spec = PivotSpec {
        rows: vec!["Bank".into(), "CL status".into()],
        values: vec![value("Balance", PivotAgg::Sum), value("AC Number", PivotAgg::CountDistinct)],
        row_totals: true,
        column_totals: true,
        layout: crate::p9_p2::NESTED,
        ..Default::default()
    };
    let view = micro_parquet::pivot_view::build_nested(crate::p9_p2::book().lazy(), &spec).expect("nested");
    let cols: Vec<String> = view.values.get_column_names().iter().map(|n| n.to_string()).collect();
    let nums: Vec<Vec<Option<f64>>> = cols.iter().map(|c| floats(&view.values, c)).collect();
    let rows: Vec<Vec<String>> = view
        .rows
        .iter()
        .enumerate()
        .map(|(i, r)| std::iter::once(format!("{}{}", "\u{a0}\u{a0}".repeat(r.level), r.label)).chain(cols.iter().enumerate().map(|(j, c)| cell(c, nums[j][i]))).collect())
        .collect();
    md(&std::iter::once("Row Labels".to_string()).chain(cols.clone()).collect::<Vec<_>>(), &rows, 1)
}

pub fn links_table() -> String {
    let f = m1::fx("p9m6_links");
    let region = m1::field(&f, "Region");
    let rows = m1::run_pivot(&f, &m1::pivot_of(&[&region]));
    md(&["Branch master \u{b7} Region".to_string(), "Sum of Balance".to_string()], &rows.iter().map(|(l, v)| vec![l.clone(), cell("Sum of Balance", Some(*v))]).collect::<Vec<_>>(), 1)
}

pub fn key_check_table() -> String {
    let d = dir("p9m6_keys");
    let bad = crate::p9_m0::tab("t2", "Product master", crate::common::parquet(&d, "products2", &crate::p9_m0::products(true)));
    let df = duplicate_rows(&crate::p9_m0::composite(), &bad, &d).expect("the list");
    let cols: Vec<String> = df.get_column_names().iter().map(|n| n.to_string()).collect();
    let times = df.column("Times").expect("Times").cast(&DataType::Int64).expect("ints");
    let rows: Vec<Vec<String>> = (0..df.height())
        .map(|i| {
            cols.iter()
                .map(|c| if c == "Times" { times.get(i).expect("n").to_string() } else { df.column(c).expect("column").get(i).expect("v").get_str().unwrap_or("").to_string() })
                .collect()
        })
        .collect();
    md(&cols, &rows, 3)
}

pub fn calendar_table() -> String {
    use micro_parquet::dates::CalendarSettings;
    let parts = [DatePart::Year, DatePart::Quarter, DatePart::MonthYear, DatePart::YearQuarter, DatePart::FiscalYear, DatePart::FiscalQuarter, DatePart::FiscalMonth];
    let d = day(2026, 10, 31);
    let head: Vec<String> = std::iter::once("Fiscal year from".to_string()).chain(parts.iter().map(|p| p.label().to_string())).collect();
    let rows: Vec<Vec<String>> = [1u32, 7]
        .iter()
        .map(|m| {
            let cal = CalendarSettings { fy_start_month: *m, week_start: micro_parquet::dates::Weekday::Sat };
            std::iter::once(micro_parquet::dates::MONTHS_LONG[*m as usize - 1].to_string()).chain(parts.iter().map(|p| p.value(d, &cal).to_string())).collect()
        })
        .collect();
    md(&head, &rows, 9)
}

fn plain_cells(b: &micro_parquet::pivot_table::PivotBuild, rows: &[&str]) -> Vec<Vec<String>> {
    let cols = build_names(b);
    rows.iter().map(|r| std::iter::once(r.to_string()).chain(cols[1..].iter().map(|c| cell(c, crate::p9_m3::at(b, r, c)))).collect()).collect()
}

fn table_of(b: &micro_parquet::pivot_table::PivotBuild, rows: &[&str]) -> String {
    md(&build_names(b), &plain_cells(b, rows), 1)
}

pub fn daily_table() -> String {
    use crate::p9_m3::*;
    let dd = dir("p9m6_daily");
    let df = DataFrame::new(vec![dates("Date", &["2026-11-01", "2026-11-03"]).into(), nums("Balance", &[100.0, 130.0])]).expect("data");
    let s = spec(&[&month_year()], &[], vec![time("Balance", TimeAgg::ClosingBalance), time("Balance", TimeAgg::AverageBalance), time("Balance", TimeAgg::OpeningBalance), plain("Balance", PivotAgg::Sum)], &["Month-Year"]);
    table_of(&run(&dd, &df, scope(&dd, settings(1), "2026-11-03", vec![]), &s), &["Nov 2026", "Grand total"])
}

pub fn month_end_table() -> String {
    use crate::p9_m3::*;
    let dd = dir("p9m6_monthend");
    let df = DataFrame::new(vec![dates("Date", &["2026-01-31", "2026-02-28", "2026-03-31", "2026-04-30"]).into(), nums("Balance", &[100.0, 120.0, 140.0, 150.0])]).expect("data");
    let s = spec(&[&cal_field("Year-Quarter").name], &[], vec![time("Balance", TimeAgg::AverageBalance), time("Balance", TimeAgg::ClosingBalance), time("Balance", TimeAgg::OpeningBalance)], &["Year-Quarter"]);
    table_of(&run(&dd, &df, scope(&dd, settings(1), "2026-04-30", vec![]), &s), &["2026 Q1", "2026 Q2", "Grand total"])
}

pub fn ytd_table() -> String {
    use crate::p9_m3::*;
    let dd = dir("p9m6_ytd");
    let df = DataFrame::new(vec![dates("Date", &["2026-01-31", "2026-02-28", "2026-03-31"]).into(), nums("Income", &[10.0, 25.0, 45.0])]).expect("data");
    let s = spec(&[&month_year()], &[], vec![time("Income", TimeAgg::PeriodAmount)], &["Month-Year"]);
    let h = "Period amount of Income (YTD)";
    let (jan, feb) = (run(&dd, &df, scope(&dd, settings(1), "2026-03-31", vec![]), &s), run(&dd, &df, scope(&dd, settings(2), "2026-03-31", vec![]), &s));
    let rows: Vec<Vec<String>> = ["Jan 2026", "Feb 2026", "Mar 2026"]
        .iter()
        .map(|m| vec![m.to_string(), micro_parquet::models::format_with_commas(df.column("Income").map(|_| 0.0).unwrap_or(0.0), 0), cell(h, at(&jan, m, h)), cell(h, at(&feb, m, h))])
        .collect();
    let incomes = [10.0, 25.0, 45.0];
    let rows: Vec<Vec<String>> = rows.into_iter().enumerate().map(|(i, mut r)| { r[1] = micro_parquet::models::format_with_commas(incomes[i], 2); r }).collect();
    md(&["Month-end".to_string(), "Income in the trial balance (since the start of the fiscal year)".to_string(), "Period amount, fiscal year from January".to_string(), "Period amount, fiscal year from February".to_string()], &rows, 1)
}

pub fn time_table() -> String {
    use crate::p9_m4::{daily, tv};
    use crate::p9_m3::*;
    let dd = dir("p9m6_time");
    let df = daily("2025-01-01", "2026-11-24");
    let sc = scope(&dd, settings(1), "2026-11-24", vec![]);
    let (w, same, full, n, v) = (TimeWindow::Mtd, TimePartial::SameDays, TimePartial::FullPeriod, TimeShift::None, TimeShow::Value);
    let s = spec(
        &["Branch"],
        &[],
        vec![
            tv("Interest", TimeAgg::TotalFlow, w, n, v, same),
            tv("Interest", TimeAgg::TotalFlow, w, TimeShift::Sply, v, same),
            tv("Interest", TimeAgg::TotalFlow, w, TimeShift::Sply, v, full),
            tv("Balance", TimeAgg::ClosingBalance, w, n, v, same),
            tv("Balance", TimeAgg::ClosingBalance, w, TimeShift::Sply, v, same),
            tv("Balance", TimeAgg::ClosingBalance, w, TimeShift::Sply, TimeShow::PercentChange, same),
        ],
        &[],
    );
    let b = run(&dd, &df, sc, &s);
    let cols = build_names(&b);
    let rows: Vec<Vec<String>> = vec![cols[1..].iter().map(|c| cell(c, crate::p9_m3::at(&b, "B1", c))).collect()];
    // one row of six numbers is hard to read: the table is turned, a heading to a line
    let turned: Vec<Vec<String>> = cols[1..].iter().zip(rows[0].iter()).map(|(c, v)| vec![c.clone(), v.clone()]).collect();
    md(&["Value as of 24 Nov 2026".to_string(), "B1".to_string()], &turned, 1)
}

pub fn yield_table() -> String {
    use crate::p9_m3::*;
    use crate::p9_m5::*;
    let dd = dir("p9m6_yield");
    let df = tb(&[("2026-01-01", "2026-01-31"), ("2028-01-01", "2028-01-31")], &|_| 1.0, 0.5);
    let sc = scope5(&dd, "2028-01-31", Some(("GL map", "GL", gl_map())), yield_measures());
    let s = spec(&[&month_year()], &[], vec![plain_measure("Interest income"), plain_measure("Average earning assets"), plain_measure("Yield 365"), plain_measure("Yield 360"), plain_measure("Yield Act")], &["Month-Year"]);
    let b = run(&dd, &df, sc, &s);
    let head: Vec<String> = vec!["Month".into(), "Interest income".into(), "Average earning assets".into(), "Actual/365".into(), "Actual/360".into(), "Actual/Actual".into()];
    let cols = build_names(&b);
    let rows: Vec<Vec<String>> = ["Jan 2026", "Jan 2028"].iter().map(|m| std::iter::once(m.to_string()).chain(cols[1..].iter().map(|c| cell(c, at(&b, m, c)))).collect()).collect();
    md(&head, &rows, 1)
}

pub fn casa_table() -> String {
    use crate::p9_m3::*;
    use crate::p9_m5::*;
    let dd = dir("p9m6_casa");
    let df = casa_tb();
    let sc = scope5(&dd, "2026-11-24", Some(("Branch master", "Branch", branch_master())), casa_measures());
    let link = Link { data_tab: "t1".into(), lookup_tab: "t3".into(), keys: vec![KeyPair { data: "Branch".into(), lookup: "Branch".into() }], unmatched_value: None };
    let region = ModelFieldRef::new(&link, "Accounts \u{2192} Branch master", "Branch master", "Region");
    let mut s = spec(&[&region.name], &[], vec![plain_measure("CASA closing"), plain_measure("Deposits closing"), plain_measure("CASA ratio")], &[]);
    s.model_fields.push(region);
    let b = run(&dd, &df, sc, &s);
    table_of(&b, &["Dhaka", "Khulna", "Grand total"])
}

pub fn project_table(title: &str) -> String {
    let tables = project_tables("p9m6_tables");
    frame_table(&tables.iter().find(|(t, _)| t == title).unwrap_or_else(|| panic!("the tab {title}")).1)
}

/// (note, the test the example names, the table), in the order the notes show them.
pub fn examples() -> Vec<(&'static str, &'static str, String)> {
    let mut v = vec![
        ("pivot-popout.md", "p1_pop_out_by_a_real_click_edits_the_same_state_and_apply_makes_the_step_the_docked_dialog_makes", popout_table()),
        ("pivot-nested.md", "p2_every_subtotal_row_equals_a_pivot_by_bank_alone_and_a_distinct_count_is_not_the_sum_of_its_children", nested_table()),
        ("model-links.md", "m1_a_pivot_by_a_lookup_field_equals_the_join_tools_pivot_with_no_match_holding_the_unmatched_sum_and_the_total_is_the_ungrouped_total", links_table()),
        ("key-check.md", "m0_a_product_master_keyed_by_wing_and_product_code_links_and_a_second_row_makes_it_broken_and_lists_both_rows", key_check_table()),
        ("model-calendar.md", "m6_the_calendar_example_is_what_the_calendar_core_gives", calendar_table()),
        ("balances-and-flows.md", "m3_a_daily_tab_closes_at_the_last_balance_and_the_average_carries_the_balance_over_the_missing_day", daily_table()),
        ("balances-and-flows.md", "m3_a_month_end_tab_averages_its_month_ends_and_says_so_in_the_heading", month_end_table()),
        ("balances-and-flows.md", "m3_a_ytd_balance_gives_the_movement_of_each_period_and_a_new_fiscal_year_starts_from_nothing", ytd_table()),
        ("time-intelligence.md", "m4_mtd_and_sply_use_the_same_days_by_default_and_the_whole_period_when_asked", time_table()),
        ("measures-and-ratios.md", "m5_a_yield_is_the_income_of_the_month_annualised_over_the_average_earning_assets_by_day_count", yield_table()),
        ("measures-and-ratios.md", "m5_a_ratios_subtotal_and_total_come_from_its_own_numerator_and_denominator_not_the_average_of_the_regions", casa_table()),
    ];
    for (title, _, _) in pivot_steps() {
        v.push(("monthly-model.md", "m6_the_worked_example_gives_the_numbers_the_monthly_model_note_quotes", project_table(title)));
    }
    v
}

#[test]
fn m6_dump_the_examples_of_the_notes() {
    let Ok(dump) = std::env::var("MP_GUIDE_DUMP") else { return };
    std::fs::create_dir_all(&dump).expect("dump folder");
    let all: Vec<String> = examples().into_iter().map(|(n, t, tab)| format!("{n} ({t})\n{tab}")).collect();
    std::fs::write(PathBuf::from(&dump).join("phase9_examples.txt"), all.join("\n\n===\n\n")).expect("write");
}

// ------------------------------------------------------------------------------------------------ the checks the examples name

#[test]
fn m6_the_calendar_example_is_what_the_calendar_core_gives() {
    use micro_parquet::dates::{CalendarSettings, Weekday as W};
    let d = day(2026, 10, 31);
    let jan = CalendarSettings { fy_start_month: 1, week_start: W::Sat };
    let jul = CalendarSettings { fy_start_month: 7, week_start: W::Sat };
    assert_eq!((DatePart::MonthYear.value(d, &jan).to_string(), DatePart::YearQuarter.value(d, &jan).to_string()), ("Oct 2026".to_string(), "2026 Q4".to_string()));
    assert_eq!((DatePart::FiscalYear.value(d, &jan).to_string(), DatePart::FiscalQuarter.value(d, &jan).to_string(), DatePart::FiscalMonth.value(d, &jan).to_string()), ("FY2026".to_string(), "FQ4".to_string(), "10".to_string()));
    assert_eq!((DatePart::FiscalYear.value(d, &jul).to_string(), DatePart::FiscalQuarter.value(d, &jul).to_string(), DatePart::FiscalMonth.value(d, &jul).to_string()), ("FY2026-27".to_string(), "FQ2".to_string(), "4".to_string()), "October is the fourth month of a July fiscal year");
    assert_eq!(DatePart::MonthYear.value(d, &jan).to_string(), DatePart::MonthYear.value(d, &jul).to_string(), "the calendar parts do not move");
}

/// The balance of a product row of the generator, as the CSV writes it.
fn generated(branches: &[&str], group: &str, class: &str, t: usize) -> f64 {
    let mut sum = 0.0;
    for (code, _, _, factor, casa) in BRANCHES {
        if !branches.contains(&code) {
            continue;
        }
        for (_, _, _, g, k, base, growth) in PRODUCTS {
            if (group.is_empty() || g == group) && (class.is_empty() || k == class) {
                let mix = if g == "CASA" { casa } else { 1.0 };
                let v = base * factor * mix * (100.0 + growth * t as f64) / 100.0;
                sum += format!("{v:.2}").parse::<f64>().expect("a number");
            }
        }
    }
    sum
}

fn at_label(df: &DataFrame, label: &str, column: &str) -> f64 {
    let labels = strings(df, df.get_column_names()[0].as_str());
    let r = labels.iter().position(|l| l.as_deref() == Some(label)).unwrap_or_else(|| panic!("row '{label}' in {labels:?}"));
    floats(df, column)[r].unwrap_or_else(|| panic!("a number at {label} / {column}"))
}

fn near(a: f64, b: f64, tol: f64, what: &str) {
    assert!((a - b).abs() <= tol, "{what}: {a} against {b}");
}

#[test]
fn m6_the_worked_example_gives_the_numbers_the_monthly_model_note_quotes() {
    let tables = project_tables("p9m6_numbers");
    let t = |title: &str| tables.iter().find(|(n, _)| n == title).map(|(_, df)| df.clone()).unwrap_or_else(|| panic!("the tab {title}"));
    // Oct 2026 is snapshot 21 (Jan 2025 is 0); Oct 2025 is 9. The generator's own sums, worked outside the app.
    let (oct26, oct25) = (21, 9);
    let region = t("CASA ratio by region");
    for (name, branches) in [("Dhaka", vec!["BR01", "BR02"]), ("Chattogram", vec!["BR03"]), ("Khulna", vec!["BR04"])] {
        let (casa, dep) = (generated(&branches, "CASA", "", oct26), generated(&branches, "", "Deposit", oct26));
        near(at_label(&region, name, "Oct 2026 | CASA closing"), casa, 0.005, &format!("{name} CASA closing"));
        near(at_label(&region, name, "Oct 2026 | Deposits closing"), dep, 0.005, &format!("{name} deposits closing"));
        near(at_label(&region, name, "Oct 2026 | CASA ratio"), casa / dep * 100.0, 0.005, &format!("{name} CASA ratio"));
        let before = generated(&branches, "CASA", "", oct25) / generated(&branches, "", "Deposit", oct25) * 100.0;
        near(at_label(&region, name, "Oct 2026 | CASA ratio \u{b7} vs SPLY (pp)"), casa / dep * 100.0 - before, 0.005, &format!("{name} against last year"));
    }
    let all = ["BR01", "BR02", "BR03", "BR04"];
    let (casa, dep) = (generated(&all, "CASA", "", oct26), generated(&all, "", "Deposit", oct26));
    near(at_label(&region, "Grand total", "Oct 2026 | CASA ratio"), casa / dep * 100.0, 0.005, "the Grand total is all CASA over all deposits");
    let by_month = t("CASA ratio by month");
    near(at_label(&by_month, "Oct 2026", "CASA ratio"), casa / dep * 100.0, 0.005, "Oct 2026");
    let sep = generated(&all, "CASA", "", 20) / generated(&all, "", "Deposit", 20) * 100.0;
    near(at_label(&by_month, "Oct 2026", "CASA ratio \u{b7} vs Prior period (pp)"), casa / dep * 100.0 - sep, 0.005, "against September");
    // loans by group, and the ADR
    let loans = t("Loans by product group");
    for (name, group) in [("Term loan", "Term loan"), ("Working capital", "Working capital"), ("Investment (Islami)", "Investment (Islami)")] {
        let (now, before) = (generated(&all, group, "Loan", oct26), generated(&all, group, "Loan", oct25));
        near(at_label(&loans, name, "Oct 2026 | Loans closing"), now, 0.005, name);
        near(at_label(&loans, name, "Oct 2026 | Loans closing \u{b7} vs SPLY %"), (now - before) / before * 100.0, 0.005, name);
    }
    near(at_label(&loans, "Grand total", "Oct 2026 | Loans closing"), generated(&all, "", "Loan", oct26), 0.005, "all loans");
    let adr = t("ADR");
    near(at_label(&adr, "Oct 2026", "ADR"), generated(&all, "", "Loan", oct26) / dep * 100.0, 0.005, "ADR");
    // the trial balance was made with a loan rate of 11% and a deposit rate of 6% in 2026: Actual/365 gives them back
    let margins = t("Margins");
    for m in ["Aug 2026", "Sep 2026", "Oct 2026"] {
        near(at_label(&margins, m, "Yield on loans"), 11.0, 0.005, &format!("{m} yield"));
        near(at_label(&margins, m, "Cost of deposits"), 6.0, 0.005, &format!("{m} cost"));
        // NIM = (11% of loans + 8.5% of investments - 6% of deposits) / (loans + investments), on the averages of the month
        let nim = at_label(&margins, m, "NIM");
        assert!(nim > 3.9 && nim < 4.2, "{m}: NIM {nim}");
    }
}

// ------------------------------------------------------------------------------------------------ the notes

const NEW_NOTES: [&str; 9] = ["pivot-popout.md", "pivot-nested.md", "model-links.md", "key-check.md", "model-calendar.md", "balances-and-flows.md", "time-intelligence.md", "measures-and-ratios.md", "monthly-model.md"];

const SECTIONS: [&str; 8] = ["## 1. What it does", "## 2. When to use it", "## 3. How to use it", "## 4. Example", "## 5. What it refuses and why", "## 6. Python export", "## 7. Saved in", "## 8. Related tools"];

fn guide() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("docs/guide")
}

fn read(name: &str) -> String {
    std::fs::read_to_string(guide().join(name)).unwrap_or_else(|e| panic!("{name}: {e}"))
}

fn section(text: &str, heading: &str) -> String {
    let mut lines = text.lines();
    lines.by_ref().find(|l| l.trim_end() == heading);
    lines.take_while(|l| !l.starts_with("## ")).collect::<Vec<_>>().join("\n")
}

/// Every table of a body, with the first non-empty line after it.
fn tables_of(body: &str) -> Vec<(String, String)> {
    let lines: Vec<&str> = body.lines().collect();
    let (mut out, mut i) = (Vec::new(), 0);
    while i < lines.len() {
        if lines[i].trim_start().starts_with('|') {
            let s = i;
            while i < lines.len() && lines[i].trim_start().starts_with('|') {
                i += 1;
            }
            let follower = lines[i..].iter().find(|l| !l.trim().is_empty()).map(|l| l.to_string()).unwrap_or_default();
            out.push((lines[s..i].iter().map(|l| l.trim_end()).collect::<Vec<_>>().join("\n"), follower));
        } else {
            i += 1;
        }
    }
    out
}

/// Every Rust source of the tests and of the window's render tests, to find the tests the notes name.
fn all_test_sources() -> String {
    let mut all = String::new();
    let mut stack = vec![PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests"), PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src")];
    while let Some(d) = stack.pop() {
        for e in std::fs::read_dir(&d).expect("a folder").flatten() {
            let p = e.path();
            if p.is_dir() {
                stack.push(p);
            } else if p.extension().is_some_and(|x| x == "rs") {
                all.push_str(&std::fs::read_to_string(&p).unwrap_or_default());
            }
        }
    }
    all
}

#[test]
fn m6_the_nine_new_notes_have_their_eight_sections_and_their_examples_are_the_tables_the_code_gives() {
    if let Ok(dump) = std::env::var("MP_GUIDE_DUMP") {
        if !dump.is_empty() {
            return; // the dump tests write the tables; the notes are written from them
        }
    }
    let sources = all_test_sources();
    for n in NEW_NOTES {
        let text = read(n);
        assert!(text.lines().next().is_some_and(|l| l.starts_with("# ")), "{n}: a title");
        let headings: Vec<&str> = text.lines().filter(|l| l.starts_with("## ")).map(str::trim_end).collect();
        assert_eq!(headings, SECTIONS.to_vec(), "{n}: the eight sections, in order");
        for s in SECTIONS {
            assert!(!section(&text, s).trim().is_empty(), "{n}: '{s}' is empty");
        }
        let how = section(&text, "## 3. How to use it");
        assert!(how.lines().any(|l| l.starts_with("1. ")) && how.lines().any(|l| l.starts_with("2. ")), "{n}: numbered steps");
        for (i, (_, follower)) in tables_of(&section(&text, "## 4. Example")).iter().enumerate() {
            let name = follower.split('`').nth(1).unwrap_or("");
            assert!(follower.trim_start().starts_with("Test:") && sources.contains(&format!("fn {name}(")), "{n}: table {} is followed by 'Test: `name`' of a test that exists: {follower:?}", i + 1);
        }
        let lines = text.lines().count();
        assert!(lines <= 150, "{n}: {lines} lines");
        for w in ["Polars", "Rust", "serde", "LazyFrame", "Expr"] {
            assert!(!text.contains(w), "{n}: '{w}' is a word of the code, not of a banker");
        }
    }
    // the tables of the examples are the ones the code gives, in order, each with the test it names
    let all = examples();
    for note in NEW_NOTES {
        let text = read(note);
        let in_note = tables_of(&section(&text, "## 4. Example"));
        let want: Vec<(&str, String)> = all.iter().filter(|(n, _, _)| *n == note).map(|(_, t, tab)| (*t, tab.clone())).collect();
        assert_eq!(in_note.len(), want.len(), "{note}: the tables of the example");
        for (i, ((table, follower), (test, tab))) in in_note.iter().zip(want).enumerate() {
            assert_eq!(table, &tab, "{note}: table {} differs from what the code gives", i + 1);
            assert!(follower.contains(&format!("`{test}`")), "{note}: table {} names {test}: {follower}", i + 1);
        }
    }
}

#[test]
fn m6_every_picture_a_note_names_exists_in_light_and_dark_and_the_readme_lists_the_notes() {
    let img = guide().join("img");
    let mut named = 0;
    for n in NEW_NOTES.iter().copied().chain(["pivot.md", "chart.md", "filter.md", "projects.md", "date-grouping.md"]) {
        for line in read(n).lines() {
            let mut rest = line;
            while let Some(i) = rest.find("](img/") {
                let tail = &rest[i + 6..];
                let file = tail.split(')').next().unwrap_or("");
                assert!(img.join(file).metadata().map(|m| m.len() > 3000).unwrap_or(false), "{n}: the picture {file} exists");
                named += 1;
                if !file.contains("-dark") && !file.starts_with("charts/") {
                    let dark = file.replace(".png", "-dark.png");
                    assert!(img.join(&dark).exists(), "{n}: {file} has a dark version {dark}");
                }
                rest = &tail[file.len()..];
            }
        }
    }
    assert!(named >= 50, "{named} pictures named");
    let readme = read("README.md");
    for n in NEW_NOTES {
        assert!(readme.contains(&format!("]({n})")), "the README lists {n}");
    }
    assert!(readme.contains("## The model (Phase 9)"));
}

#[test]
fn m6_the_notes_that_phase_9_changed_say_what_the_code_now_does() {
    let pivot = read("pivot.md");
    for needle in ["pivot-popout.md", "pivot-nested.md", "model-links.md", "Measures ›", "Time", "(no match)", "Show rows"] {
        assert!(pivot.contains(needle), "pivot.md says {needle}");
    }
    assert!(pivot.contains("This number combines several filtered series. Use Show rows on a column value instead."));
    let chart = read("chart.md");
    assert!(chart.contains("Branch master › Region") && chart.contains("Measures"), "chart.md lists the model's fields and the measures");
    let filter = read("filter.md");
    assert!(filter.contains("(no match)") && filter.contains("model-links.md"), "filter.md takes lookup fields");
    let projects = read("projects.md");
    assert!(projects.contains("\"project_version\": 2") && projects.contains("Model: 4 links"), "projects.md says what version 2 holds");
    let grouping = read("date-grouping.md");
    assert!(grouping.contains("balances-and-flows.md") && !grouping.contains("are measures of Phase 9"), "the stocks box points to the new note");
    // the refusal the notes quote is the text the code gives
    let src = all_test_sources();
    assert!(src.contains("Python export can't follow links between tabs yet. Use the Join tool to copy the column, or export the result tab."));
}

#[allow(dead_code)]
fn unused() {
    let _ = (pct, cal_field, calendar_field);
}
