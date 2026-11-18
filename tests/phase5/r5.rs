//! Phase 5 R5: the CL-1 summary and its Excel layout.

use crate::common::*;
use micro_parquet::cl1::*;
use micro_parquet::engine::compile_tab_lazy;
use micro_parquet::models::*;
use micro_parquet::rules::set_data_dir_override;
use polars::prelude::*;
use std::path::Path;

/// Six loans, with the totals worked out by hand below.
fn book() -> DataFrame {
    DataFrame::new(vec![
        Series::new("Account_No".into(), &["A1", "A2", "A3", "A4", "A5", "A6"]),
        Series::new("Loan_Category".into(), &["Continuous", "CONTINUOUS", "Demand", "Fixed term", "Fixed term", "Short-term agri & micro"]),
        Series::new("CL_Final".into(), &["STD-0", "STD-2", "SMA", "SS", "B/L", "DF"]),
        Series::new("Balance".into(), &[1000.0, 2000.0, 500.0, 300.0, 100.0, 400.0]),
        Series::new("Base_For_Provision_BDT".into(), &[1000.0, 2000.0, 500.0, 250.0, 100.0, 300.0]),
        Series::new("Required_Provision_BDT".into(), &[10.0, 20.0, 25.0, 50.0, 100.0, 150.0]),
    ])
    .expect("book")
}

fn spec() -> Cl1Spec {
    Cl1Spec {
        category_col: Some("Loan_Category".into()),
        fixed_category: String::new(),
        grade_col: "CL_Final".into(),
        measure_cols: vec![("outstanding".into(), "Balance".into()), ("base".into(), "Base_For_Provision_BDT".into()), ("provision".into(), "Required_Provision_BDT".into())],
    }
}

fn template() -> Cl1Template {
    Cl1Template::parse(CL1_TEMPLATE, "cl1.json").expect("the shipped template")
}

fn maps() -> (Vec<(String, String)>, Vec<(String, String)>) {
    static N: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let data = dir(&format!("r5maps{}", N.fetch_add(1, std::sync::atomic::Ordering::SeqCst)));
    set_data_dir_override(Some(data.clone()));
    let cat = micro_parquet::overdue::load_category_map(&data).expect("category map");
    let cl = micro_parquet::banking::ProvisionConfig::load_or_create(&data).expect("cl map").cl_code_map;
    set_data_dir_override(None);
    (cat, cl)
}

fn frame() -> DataFrame {
    let (cat, cl) = maps();
    build_cl1_frame(&book(), &spec(), &template(), &cat, &cl).expect("the summary")
}

fn cell(df: &DataFrame, row: usize, col: &str) -> f64 {
    floats(df, col)[row].unwrap_or(f64::NAN)
}

#[test]
fn r5_every_cell_is_the_hand_total_and_the_grand_totals_are_the_column_sums() {
    let df = frame();
    assert_eq!(df.height(), 5, "four categories and the total row");
    assert_eq!(strings(&df, "Loan category"), ["Continuous", "Demand", "Fixed term", "Short-term agri & micro", "Total"].iter().map(|s| Some(s.to_string())).collect::<Vec<_>>());
    let names: Vec<String> = df.get_column_names().iter().map(|n| n.to_string()).collect();
    assert_eq!(names.len(), 1 + 6 * 3, "five grade groups and the total, three measures each");
    assert_eq!(&names[1..4], ["STD | Outstanding", "STD | Base for provision", "STD | Provision required"]);
    assert_eq!(&names[16..19], ["Total | Outstanding", "Total | Base for provision", "Total | Provision required"]);
    // the hand totals: (row, column) -> value
    let hand: Vec<(usize, &str, f64)> = vec![
        // Continuous: A1 (STD-0) and A2 (STD-2) are both STD
        (0, "STD | Outstanding", 3000.0), (0, "STD | Base for provision", 3000.0), (0, "STD | Provision required", 30.0),
        (0, "SMA | Outstanding", 0.0), (0, "Total | Outstanding", 3000.0), (0, "Total | Provision required", 30.0),
        // Demand: A3
        (1, "SMA | Outstanding", 500.0), (1, "SMA | Base for provision", 500.0), (1, "SMA | Provision required", 25.0), (1, "STD | Outstanding", 0.0), (1, "Total | Base for provision", 500.0),
        // Fixed term: A4 (SS) and A5 (B/L)
        (2, "SS | Outstanding", 300.0), (2, "SS | Base for provision", 250.0), (2, "SS | Provision required", 50.0),
        (2, "B/L | Outstanding", 100.0), (2, "B/L | Base for provision", 100.0), (2, "B/L | Provision required", 100.0),
        (2, "Total | Outstanding", 400.0), (2, "Total | Base for provision", 350.0), (2, "Total | Provision required", 150.0),
        // Short-term agri & micro: A6
        (3, "DF | Outstanding", 400.0), (3, "DF | Base for provision", 300.0), (3, "DF | Provision required", 150.0), (3, "Total | Provision required", 150.0),
        // the total row
        (4, "STD | Outstanding", 3000.0), (4, "SMA | Outstanding", 500.0), (4, "SS | Outstanding", 300.0), (4, "DF | Outstanding", 400.0), (4, "B/L | Outstanding", 100.0),
        (4, "Total | Outstanding", 4300.0), (4, "Total | Base for provision", 4150.0), (4, "Total | Provision required", 355.0),
        (4, "SS | Provision required", 50.0), (4, "B/L | Provision required", 100.0),
    ];
    for (r, c, v) in hand {
        assert!((cell(&df, r, c) - v).abs() < 1e-9, "row {r} '{c}': {} vs hand total {v}", cell(&df, r, c));
    }
    // the last row is the sum of the rows above, in every column
    for n in &names[1..] {
        let v = floats(&df, n);
        let s: f64 = v[..4].iter().flatten().sum();
        assert!((v[4].unwrap_or(f64::NAN) - s).abs() < 1e-9, "'{n}': total row {:?} vs column sum {s}", v[4]);
    }
    // each row's total column is the sum of its groups, and the grand total is the book's total
    for r in 0..5 {
        for m in ["Outstanding", "Base for provision", "Provision required"] {
            let parts: f64 = ["STD", "SMA", "SS", "DF", "B/L"].iter().map(|g| cell(&df, r, &format!("{g} | {m}"))).sum();
            assert!((cell(&df, r, &format!("Total | {m}")) - parts).abs() < 1e-9, "row {r} {m}");
        }
    }
    let b = book();
    assert_eq!(floats(&b, "Balance").iter().flatten().sum::<f64>(), cell(&df, 4, "Total | Outstanding"));
    assert_eq!(floats(&b, "Required_Provision_BDT").iter().flatten().sum::<f64>(), cell(&df, 4, "Total | Provision required"));
}

#[test]
fn r5_a_phase_1_grade_counts_as_its_first_sub_grade_and_one_category_can_serve_all_rows() {
    let (cat, cl) = maps();
    let mut b = book();
    b.with_column(Series::new("CL_Grade".into(), &["STD", "STD", "SMA", "SS", "BL", "DF"])).expect("col");
    let s = Cl1Spec { category_col: None, fixed_category: "Demand".into(), grade_col: "CL_Grade".into(), ..spec() };
    let df = build_cl1_frame(&b, &s, &template(), &cat, &cl).expect("the summary");
    assert_eq!(cell(&df, 1, "STD | Outstanding"), 3000.0);
    assert_eq!(cell(&df, 1, "B/L | Outstanding"), 100.0);
    assert_eq!(cell(&df, 0, "Total | Outstanding"), 0.0, "everything is in the Demand row");
    assert_eq!(cell(&df, 4, "Total | Outstanding"), 4300.0);
}

fn unzip_entry(zip: &Path, entry: &str) -> String {
    let out = std::process::Command::new("unzip").arg("-p").arg(zip).arg(entry).output().expect("unzip must be installed");
    assert!(out.status.success(), "unzip {entry}: {}", String::from_utf8_lossy(&out.stderr));
    String::from_utf8_lossy(&out.stdout).to_string()
}

/// The text or number of a cell of sheet XML (a text through the shared strings).
fn cell_text(sheet: &str, shared: &[String], r: &str) -> Option<String> {
    let tag = sheet.split(&format!("<c r=\"{r}\"")).nth(1)?;
    let (attrs, rest) = tag.split_once('>')?;
    if attrs.ends_with('/') {
        return None;
    }
    let v = rest.split("<v>").nth(1)?.split("</v>").next()?.to_string();
    if attrs.contains("t=\"s\"") {
        shared.get(v.parse::<usize>().ok()?).cloned()
    } else {
        Some(v)
    }
}

fn shared_strings(xml: &str) -> Vec<String> {
    xml.split("<si>").skip(1).map(|si| si.split("<t").nth(1).and_then(|t| t.split_once('>')).map(|(_, t)| t.split("</t>").next().unwrap_or("").replace("&amp;", "&")).unwrap_or_default()).collect()
}

fn merges(sheet: &str) -> Vec<String> {
    sheet.split("<mergeCell ref=\"").skip(1).map(|m| m.split('"').next().unwrap_or("").to_string()).collect()
}

#[test]
fn r5_the_excel_cells_and_merges_are_at_the_positions_of_the_template() {
    let df = frame();
    let t = template();
    let p = dir("r5x").join("cl1.xlsx");
    write_cl1_xlsx(&df, &t, &p, NumberDisplayFormat::Raw, 2).expect("writes");
    let sheet = unzip_entry(&p, "xl/worksheets/sheet1.xml");
    let ss = shared_strings(&unzip_entry(&p, "xl/sharedStrings.xml"));
    let m = merges(&sheet);
    // template: title row 0, unit row 1, group row 3, measure row 4, first data row 5, label column A; 18 value columns B..S
    for want in ["A1:S1", "A2:S2", "A4:A5", "B4:D4", "E4:G4", "H4:J4", "K4:M4", "N4:P4", "Q4:S4"] {
        assert!(m.iter().any(|x| x == want), "the merge {want} is in the sheet: {m:?}");
    }
    assert_eq!(m.len(), 9, "{m:?}");
    let at = |r: &str| cell_text(&sheet, &ss, r).unwrap_or_else(|| panic!("no cell {r}"));
    assert_eq!(at("A1"), t.title);
    assert_eq!(at("A2"), "Figures in BDT");
    assert_eq!(at("A4"), "Loan category");
    for (r, want) in [("B4", "STD"), ("E4", "SMA"), ("H4", "SS"), ("K4", "DF"), ("N4", "B/L"), ("Q4", "Total"), ("B5", "Outstanding"), ("C5", "Base for provision"), ("D5", "Provision required"), ("S5", "Provision required")] {
        assert_eq!(at(r), want, "{r}");
    }
    // the category rows start at row 6 and the total is the row after the last category
    for (r, want) in [("A6", "Continuous"), ("A7", "Demand"), ("A8", "Fixed term"), ("A9", "Short-term agri & micro"), ("A10", "Total")] {
        assert_eq!(at(r), want, "{r}");
    }
    let num = |r: &str| at(r).parse::<f64>().unwrap_or(f64::NAN);
    assert_eq!(num("B6"), 3000.0, "Continuous, STD, outstanding");
    assert_eq!(num("D6"), 30.0, "Continuous, STD, provision");
    assert_eq!(num("E7"), 500.0, "Demand, SMA, outstanding");
    assert_eq!(num("H8"), 300.0, "Fixed term, SS, outstanding");
    assert_eq!(num("N8"), 100.0, "Fixed term, B/L, outstanding");
    assert_eq!(num("K9"), 400.0, "agri, DF, outstanding");
    assert_eq!(num("Q10"), 4300.0, "grand total outstanding");
    assert_eq!(num("S10"), 355.0, "grand total provision");
    // the sample template's note is under the table, outside the template's cells
    assert!(ss.iter().any(|s| s.starts_with("Sample layout")), "{ss:?}");
    // in Crore the numbers are divided and the unit line says so
    let p2 = dir("r5x2").join("cl1c.xlsx");
    write_cl1_xlsx(&df, &t, &p2, NumberDisplayFormat::BdtCrore, 2).expect("writes");
    let sheet = unzip_entry(&p2, "xl/worksheets/sheet1.xml");
    let ss = shared_strings(&unzip_entry(&p2, "xl/sharedStrings.xml"));
    assert_eq!(cell_text(&sheet, &ss, "A2").as_deref(), Some("Figures in BDT Crore"));
    let v: f64 = cell_text(&sheet, &ss, "Q10").and_then(|x| x.parse().ok()).unwrap_or(f64::NAN);
    assert!((v - 4300.0 / 1e7).abs() < 1e-12, "{v}");
}

#[test]
fn r5_another_layout_in_the_template_moves_the_cells() {
    let mut v: serde_json::Value = serde_json::from_str(CL1_TEMPLATE).expect("json");
    v["layout"] = serde_json::json!({"title_row": 1, "unit_row": 2, "group_row": 4, "measure_row": 5, "first_data_row": 6, "label_col": 1});
    v["measures"] = serde_json::json!([{"id": "outstanding", "label": "Outstanding"}, {"id": "provision", "label": "Provision"}]);
    v["sample"] = serde_json::json!(false);
    let t = Cl1Template::parse(&v.to_string(), "cl1.json").expect("parses");
    let (cat, cl) = maps();
    let s = Cl1Spec { measure_cols: vec![("outstanding".into(), "Balance".into()), ("provision".into(), "Required_Provision_BDT".into())], ..spec() };
    let df = build_cl1_frame(&book(), &s, &t, &cat, &cl).expect("summary");
    assert_eq!(df.width(), 1 + 6 * 2);
    let p = dir("r5y").join("cl1.xlsx");
    write_cl1_xlsx(&df, &t, &p, NumberDisplayFormat::Raw, 0).expect("writes");
    let sheet = unzip_entry(&p, "xl/worksheets/sheet1.xml");
    let ss = shared_strings(&unzip_entry(&p, "xl/sharedStrings.xml"));
    let m = merges(&sheet);
    // label column B (index 1), values C..N; groups of two columns
    for want in ["B2:N2", "B3:N3", "B5:B6", "C5:D5", "E5:F5", "G5:H5", "I5:J5", "K5:L5", "M5:N5"] {
        assert!(m.iter().any(|x| x == want), "the merge {want} is in the sheet: {m:?}");
    }
    assert_eq!(cell_text(&sheet, &ss, "B2").as_deref(), Some(t.title.as_str()));
    assert_eq!(cell_text(&sheet, &ss, "B7").as_deref(), Some("Continuous"));
    assert_eq!(cell_text(&sheet, &ss, "C7").as_deref(), Some("3000"));
    assert_eq!(cell_text(&sheet, &ss, "M11").as_deref(), Some("4300"));
    assert!(!ss.iter().any(|s| s.starts_with("Sample layout")), "not a sample template: no note");
    // a table that does not fit the template is refused, not written askew
    let e = format!("{:#}", write_cl1_xlsx(&frame(), &t, &dir("r5y2").join("x.xlsx"), NumberDisplayFormat::Raw, 2).err().expect("wrong shape"));
    assert!(e.contains("Build the summary again"), "{e}");
}

#[test]
fn r5_a_template_that_names_an_unknown_grade_is_refused_with_the_name() {
    let edit = |f: &dyn Fn(&mut serde_json::Value)| -> String {
        let mut v: serde_json::Value = serde_json::from_str(CL1_TEMPLATE).expect("json");
        f(&mut v);
        format!("{:#}", Cl1Template::parse(&v.to_string(), "report_templates/cl1.json").err().expect("refused"))
    };
    let e = edit(&|v| v["grade_groups"][1]["grades"] = serde_json::json!(["SMA-1"]));
    assert!(e.contains("'SMA-1'") && e.contains("report_templates/cl1.json") && e.contains("STD-0, STD-1, STD-2, SMA, SS, DF, B/L"), "{e}");
    let e = edit(&|v| v["grade_groups"][2]["grades"] = serde_json::json!(["SS", "SMA"]));
    assert!(e.contains("'SMA'") && e.contains("more than one group"), "{e}");
    let e = edit(&|v| v["measures"][1]["id"] = serde_json::json!("profit"));
    assert!(e.contains("'profit'"), "{e}");
    let e = edit(&|v| v["layout"]["measure_row"] = serde_json::json!(2));
    assert!(e.contains("each below the one before"), "{e}");
    let e = edit(&|v| v["categories"] = serde_json::json!(["Demand", "demand"]));
    assert!(e.contains("twice"), "{e}");
    let e = edit(&|v| v["source"] = serde_json::json!(""));
    assert!(e.contains("no \"source\""), "{e}");
    let e = format!("{:#}", Cl1Template::parse("{ nope", "cl1.json").err().expect("json"));
    assert!(e.contains("cl1.json is not valid"), "{e}");
}

#[test]
fn r5_bad_data_stops_the_run_with_the_count_and_the_first_five_accounts() {
    let (cat, cl) = maps();
    let t = template();
    let n = 7;
    let accts: Vec<String> = (1..=n).map(|i| format!("B{i}")).collect();
    // a grade the template does not know on 7 rows
    let df = DataFrame::new(vec![
        Series::new("Account_No".into(), accts.clone()),
        Series::new("Loan_Category".into(), vec!["Demand"; n]),
        Series::new("CL_Final".into(), vec!["XYZ"; n]),
        Series::new("Balance".into(), vec![1.0; n]),
        Series::new("Base_For_Provision_BDT".into(), vec![1.0; n]),
        Series::new("Required_Provision_BDT".into(), vec![1.0; n]),
    ])
    .expect("book");
    let e = format!("{:#}", build_cl1_frame(&df, &spec(), &t, &cat, &cl).err().expect("unknown grade"));
    assert!(e.contains("not one of the grades of the template") && e.contains("on 7 row(s)") && e.contains("first 5 values of Account_No: B1, B2, B3, B4, B5") && !e.contains("B6"), "{e}");
    // blank amounts
    let mut df = book();
    df.with_column(Series::new("Balance".into(), &[Some(1.0), None, Some(1.0), Some(1.0), Some(1.0), None])).expect("col");
    let e = format!("{:#}", build_cl1_frame(&df, &spec(), &t, &cat, &cl).err().expect("blank amounts"));
    assert!(e.contains("'Balance' (Outstanding) is blank or not a number on 2 row(s)") && e.contains("A2, A6"), "{e}");
    // an unmapped loan category code
    let mut df = book();
    df.with_column(Series::new("Loan_Category".into(), &["Continuous", "TL", "Demand", "Demand", "Demand", "Demand"])).expect("col");
    let e = format!("{:#}", build_cl1_frame(&df, &spec(), &t, &cat, &cl).err().expect("unmapped"));
    assert!(e.contains("'TL'") && e.contains("loan_category_map.json") && e.contains("the template cl1.json"), "{e}");
    // missing and non-numeric columns, and an unchosen measure
    let e = format!("{:#}", build_cl1_frame(&book(), &Cl1Spec { grade_col: "Nope".into(), ..spec() }, &t, &cat, &cl).err().expect("missing"));
    assert!(e.contains("grade column 'Nope' was not found"), "{e}");
    let mut text = book();
    text.with_column(Series::new("Balance".into(), &["1", "2", "3", "4", "5", "6"])).expect("col");
    let e = format!("{:#}", build_cl1_frame(&text, &spec(), &t, &cat, &cl).err().expect("text amounts"));
    assert!(e.contains("not a number"), "{e}");
    let e = format!("{:#}", build_cl1_frame(&book(), &Cl1Spec { measure_cols: vec![("outstanding".into(), "Balance".into())], ..spec() }, &t, &cat, &cl).err().expect("missing measure"));
    assert!(e.contains("choose the column of 'Base for provision'"), "{e}");
}

#[test]
fn r5_the_template_is_a_sample_written_only_when_missing_and_never_overwritten() {
    let src = std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("assets").join("report_templates").join("cl1.json")).expect("the source copy is in the repository");
    assert_eq!(src, CL1_TEMPLATE, "the built-in copy is the file under assets/report_templates/");
    let t = template();
    assert!(t.sample, "shipped as a sample for the owner to align with the current CL-1 form");
    assert!(t.source.contains("SAMPLE"));
    let data = dir("r5t");
    set_data_dir_override(Some(data.clone()));
    let f = data.join("report_templates").join("cl1.json");
    assert!(!f.exists());
    assert_eq!(load_template().expect("loads"), t);
    assert_eq!(std::fs::read_to_string(&f).expect("written"), CL1_TEMPLATE);
    // the owner aligned it: another title and a not-sample flag
    let aligned = CL1_TEMPLATE.replace("CL-1: Classification of loans and advances", "CL-1 (aligned)").replace("\"sample\": true", "\"sample\": false");
    std::fs::write(&f, &aligned).expect("edit");
    let t2 = load_template().expect("loads");
    assert_eq!((t2.title.as_str(), t2.sample), ("CL-1 (aligned)", false));
    assert_eq!(std::fs::read_to_string(&f).expect("read"), aligned, "an existing template is never overwritten");
    std::fs::write(&f, "{ broken").expect("break");
    let e = format!("{:#}", load_template().err().expect("broken"));
    assert!(e.contains("report_templates/cl1.json is not valid"), "{e}");
    assert_eq!(std::fs::read_to_string(&f).expect("read"), "{ broken");
    set_data_dir_override(None);
}

#[test]
fn r5_the_tab_source_builds_the_table_and_the_export_job_writes_the_sheet() {
    let tmp = dir("r5s");
    let data = dir("r5s_data");
    set_data_dir_override(Some(data));
    let input = parquet(&tmp, "loans", &book());
    let source = TabSource::Cl1Summary { source: Box::new(input), steps: vec![], spec: spec(), template: Box::new(template()) };
    let out = compile_tab_lazy(&source, &[], &tmp).and_then(|lf| Ok(lf.collect()?)).expect("compiles");
    assert!(out.equals_missing(&frame()), "the tab is the summary table");
    let p = tmp.join("export.xlsx");
    let msg = micro_parquet::jobs::run_inline(|ctx| micro_parquet::tab_jobs::export_cl1_xlsx(&source, &tmp, &p, NumberDisplayFormat::BdtCrore, 2, ctx)).expect("exports");
    assert!(msg.message.contains("Figures in BDT Crore"), "{}", msg.message);
    let sheet = unzip_entry(&p, "xl/worksheets/sheet1.xml");
    assert!(merges(&sheet).iter().any(|m| m == "B4:D4"));
    // another kind of tab is refused with what to do
    let plain = parquet(&tmp, "plain", &book());
    let e = micro_parquet::jobs::run_inline(|ctx| micro_parquet::tab_jobs::export_cl1_xlsx(&plain, &tmp, &tmp.join("no.xlsx"), NumberDisplayFormat::Raw, 2, ctx)).err().map(|e| format!("{e:#}")).expect("refused");
    assert!(e.contains("Open a CL-1 summary tab first"), "{e}");
    assert!(!tmp.join("no.xlsx").exists());
    set_data_dir_override(None);
}
