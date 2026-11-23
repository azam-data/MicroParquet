//! Headless command-line mode (moved out of main.rs by Phase 3 R1).

use crate::ui::prelude::*;

/// `--pack <name> --slot <slot>=<file> ... --cutoff YYYY-MM-DD --export-xlsx <file>`: the same
/// run and the same workbook as the Report packs dialog (Phase 6 P2).
pub(crate) fn run_pack_cli(args: &crate::CliArgs, paths: &PortablePaths) -> anyhow::Result<()> {
    use micro_parquet::packs::*;
    let name = args.pack.as_deref().ok_or_else(|| anyhow::anyhow!("--pack <name> is required"))?;
    let pack = load_pack(&packs_dir(), name)?;
    let mut files = std::collections::BTreeMap::new();
    for s in &args.slot {
        let (slot, file) = s.split_once('=').ok_or_else(|| anyhow::anyhow!("--slot needs the form <slot>=<file>, for example --slot Loans=loans.csv (got '{}')", s))?;
        if files.insert(slot.trim().to_string(), PathBuf::from(file.trim())).is_some() {
            anyhow::bail!("--slot {} is given twice.", slot.trim());
        }
    }
    let cutoff = args.cutoff.clone().ok_or_else(|| anyhow::anyhow!("--cutoff YYYY-MM-DD is required with --pack"))?;
    let xlsx = args.export_xlsx.clone().ok_or_else(|| anyhow::anyhow!("--export-xlsx <file> is required with --pack"))?;
    let pdf = match (&args.export_pdf, pack.output.pdf) {
        (Some(p), _) => Some(p.clone()),
        (None, true) => Some(xlsx.with_extension("pdf")),
        (None, false) => None,
    };
    // the chart book: `--export-html <file>`, or `<pack>-charts.html` beside the workbook when the pack asks for it
    let html = match (&args.export_html, pack.output.html_charts) {
        (Some(p), _) => Some(p.clone()),
        (None, true) => Some(html_path_for(&xlsx, &pack.name)),
        (None, false) => None,
    };
    let spec = PackRunSpec { files, cutoff };
    let msg = micro_parquet::jobs::run_inline(|ctx| {
        let r = run_pack(&pack, &spec, paths, &run_time_now(), ctx)?;
        write_pack_with_charts(&r, &xlsx, pdf.as_deref(), html.as_deref(), ctx)
    })?;
    println!("{}", msg);
    Ok(())
}

pub(crate) fn run_headless_cli(args: &crate::CliArgs, paths: &PortablePaths) -> anyhow::Result<()> {
    if args.pack.is_some() {
        return run_pack_cli(args, paths);
    }
    if args.export_html.is_some() {
        anyhow::bail!("--export-html writes the chart book of a report pack run: add --pack <name> (with --slot, --cutoff and --export-xlsx). It is not an export of --input.");
    }
    let input_path = args
        .input
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("--input <FILE> is required in --headless mode"))?;

    let is_parquet = input_path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .eq_ignore_ascii_case("parquet");

    let (cfg, steps, recipe_control) = if let Some(m_path) = &args.macro_file {
        let recipe = load_macro_from_file(m_path)?;
        let control = recipe.control_column.clone();
        let ing = if is_parquet {
            match recipe.optional_ingestion_schema {
                // Applies to columns that exist and are String in the file; the others keep
                // their Parquet type.
                Some(saved_ing) => {
                    let (fitted, missing) = adapt_schema_to_parquet(input_path, &saved_ing)?;
                    if !missing.is_empty() {
                        eprintln!(
                            "[MicroParquet] Warning: macro schema columns not in the Parquet file, skipped: {}",
                            missing.join(", ")
                        );
                    }
                    fitted
                }
                None => IngestionConfig::default(),
            }
        } else if let Some(saved_ing) = recipe.optional_ingestion_schema {
            saved_ing
        } else {
            auto_detect_smart_schema(input_path)?
        };
        (ing, recipe.steps, control)
    } else {
        let ing = if is_parquet {
            IngestionConfig::default()
        } else {
            auto_detect_smart_schema(input_path)?
        };
        (ing, Vec::new(), None)
    };

    let source = TabSource::SingleFile {
        path: input_path.clone(),
        is_parquet,
        config: cfg,
    };

    if let Some(note) = ensure_pii_key_for(&source)? {
        eprintln!("[MicroParquet] {}", note);
    }
    let lf = compile_tab_lazy(&source, &steps, &paths.temp_dir)?;
    if let Some(w) = source_row_check(&source, &paths.temp_dir)?.and_then(|c| c.warning()) {
        eprintln!("[MicroParquet] WARNING: {}", w);
    }
    for note in blank_balance_notes(&source, &steps, &paths.temp_dir)? {
        eprintln!("[MicroParquet] {}", note);
    }
    for note in weighted_mixed_sign_notes(&source, &steps, &paths.temp_dir)? {
        eprintln!("[MicroParquet] {}", note);
    }

    // Values that could not be converted: always reported, and they stop the run when the
    // control column has any (unless --allow-parse-failures).
    let failures = source_parse_failures(&source, &paths.temp_dir)?;
    for f in &failures {
        eprintln!(
            "[MicroParquet] Parse failures: {} {} (examples: {})",
            f.column,
            f.count,
            f.examples.join(" | ")
        );
    }
    let control = match recipe_control {
        Some(c) => Some(c),
        None => micro_parquet::tab_jobs::guess_balance_column(&lf.clone().limit(0).collect()?),
    };
    if let Some(ctrl) = &control {
        if let Some(f) = failures.iter().find(|f| &f.column == ctrl) {
            if !args.allow_parse_failures {
                anyhow::bail!(
                    "Control column '{}' has {} value(s) that could not be converted (e.g. {}). \
                     Fix the file or its schema, or pass --allow-parse-failures to export anyway.",
                    ctrl,
                    f.count,
                    f.examples.join(" | ")
                );
            }
        }
    }

    if let Some(out_parq) = &args.export_parquet {
        micro_parquet::jobs::run_inline(|ctx| {
            export_tab_parquet(&source, &steps, &paths.temp_dir, out_parq, ParquetDialect::ArchiveZstd, None, ctx)
        })?;
        println!("Exported Parquet: {:?}", out_parq);
    }
    if let Some(out_csv) = &args.export_csv {
        micro_parquet::jobs::run_inline(|ctx| {
            export_tab_csv(&source, &steps, &paths.temp_dir, out_csv, false, None, ctx)
        })?;
        println!("Exported CSV: {:?}", out_csv);
    }
    if let Some(out_xlsx) = &args.export_xlsx {
        let df = lf.clone().collect()?;
        let rep = micro_parquet::jobs::run_inline(|ctx| {
            micro_parquet::jobs::write_atomic(out_xlsx, ctx, |tmp| {
                export_tabs_to_xlsx(vec![("Headless_Output".to_string(), df)], Vec::new(), tmp)
            })
        })?;
        println!("Exported Excel: {:?} ({})", out_xlsx, rep.summary());
    }
    if let Some(out_pdf) = &args.export_pdf {
        let df = lf.collect()?;
        let rep = micro_parquet::jobs::run_inline(|ctx| {
            micro_parquet::jobs::write_atomic(out_pdf, ctx, |tmp| {
                export_tabs_to_pdf(
                    vec![("Headless_Report".to_string(), df, HashMap::new())],
                    tmp,
                    NumberDisplayFormat::BdtCrore,
                    2,
                )
            })
        })?;
        println!("Exported PDF: {:?}", out_pdf);
        let s = rep.summary();
        if !s.is_empty() {
            eprintln!("WARNING (PDF): {}", s);
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;
    use micro_parquet::packs::*;

    fn unzip(zip: &Path, entry: &str) -> String {
        let out = std::process::Command::new("unzip").arg("-p").arg(zip).arg(entry).output().expect("unzip must be installed");
        String::from_utf8_lossy(&out.stdout).to_string()
    }

    #[test]
    fn p2_headless_pack_gives_the_same_workbook_as_the_dialog_path() {
        let root = std::env::temp_dir().join(format!("mp_p6_headless_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("temp")).expect("folder");
        micro_parquet::rules::set_data_dir_override(Some(root.clone()));
        let paths = PortablePaths { root_dir: root.clone(), macros_dir: root.join("m"), schemas_dir: root.join("s"), curves_dir: root.join("c"), buckets_file: root.join("b.json"), temp_dir: root.join("temp") };
        let deposits = root.join("deposits.csv");
        std::fs::write(&deposits, "ACNO,BAL,MAT,PROD\nD1,100,2026-12-31,SB\nD2,50,2027-06-30,TD\nD3,25,2026-11-30,TD\n").expect("csv");
        let loans = root.join("loans.csv");
        std::fs::write(&loans, "ACNO,BAL,MAT,CAT,GRADE,BASE,PROV\nL1,1000000,2027-03-31,Continuous,STD-0,1000000,10000\nL2,2000000,2028-10-31,Demand,SS,1500000,300000\n").expect("loans");
        // tool-run items too (G11): the gap ladder (a tool run over two slots) and the CL-1 summary
        let gap = micro_parquet::tool_runs::ToolRun::GapLadder(micro_parquet::tool_runs::GapLadderRun {
            inputs: vec![
                micro_parquet::tool_runs::GapInputRun { source: "Loans".into(), spec: micro_parquet::banking::GapInputSpec { role: micro_parquet::banking::GapRole::Asset, amount_col: "BAL".into(), maturity_col: "MAT".into() }, product_col: String::new() },
                micro_parquet::tool_runs::GapInputRun { source: "Deposits".into(), spec: micro_parquet::banking::GapInputSpec { role: micro_parquet::banking::GapRole::Liability, amount_col: "BAL".into(), maturity_col: "MAT".into() }, product_col: String::new() },
            ],
            preset: CustomBucketDef::default_presets()[0].name.clone(),
            preset_def: None,
            cutoff: "2020-01-31".into(),
            null_as_sight: true,
            behaviour_file: String::new(),
            stress_tdrr: false,
        });
        let cl1 = micro_parquet::tool_runs::ToolRun::Cl1Summary(micro_parquet::tool_runs::Cl1SummaryRun {
            source: "Loans".into(),
            spec: micro_parquet::cl1::Cl1Spec { category_col: Some("CAT".into()), fixed_category: String::new(), grade_col: "GRADE".into(), measure_cols: vec![("outstanding".into(), "BAL".into()), ("base".into(), "BASE".into()), ("provision".into(), "PROV".into())] },
            template_file: String::new(),
        });
        let pivot = TransformStep::from_pivot_spec(PivotSpec { rows: vec!["PROD".into()], values: vec![PivotValue { field: "BAL".into(), agg: PivotAgg::Sum, weight_col: None }], row_totals: true, column_totals: true, ..Default::default() });
        let pack = ReportPack {
            name: "mini".into(),
            inputs: vec![
                PackInput { slot: "Deposits".into(), required_columns: vec!["ACNO".into(), "BAL".into(), "MAT".into(), "PROD".into()], ..Default::default() },
                PackInput { slot: "Loans".into(), required_columns: vec!["ACNO".into(), "BAL".into(), "MAT".into(), "GRADE".into()], ..Default::default() },
            ],
            items: vec![
                PackItem { title: "Gap".into(), sheet: "Gap".into(), source: PackItemKind::Run { run: gap }, col_formats: Default::default(), chart: None },
                PackItem { title: "CL-1".into(), sheet: "CL1".into(), source: PackItemKind::Run { run: cl1 }, col_formats: Default::default(), chart: None },
                PackItem { title: "Table".into(), sheet: "Table".into(), source: PackItemKind::Steps { slot: "Deposits".into() }, col_formats: Default::default(), chart: None },
                PackItem { title: "By product".into(), sheet: "By product".into(), source: PackItemKind::Pivot { slot: "Deposits".into(), pivot: Some(pivot) }, col_formats: Default::default(), chart: None },
            ],
            output: PackOutput::default(),
        };
        save_pack(&packs_dir(), &pack).expect("saves");
        // the library path (what the dialog's worker does)
        let spec = PackRunSpec { files: std::collections::BTreeMap::from([("Deposits".to_string(), deposits.clone()), ("Loans".to_string(), loans.clone())]), cutoff: "2026-10-31".into() };
        let r = micro_parquet::jobs::run_inline(|ctx| run_pack(&pack, &spec, &paths, "ui", ctx)).expect("runs");
        let ui_xlsx = root.join("ui.xlsx");
        micro_parquet::jobs::run_inline(|ctx| write_pack(&r, &ui_xlsx, None, ctx)).expect("writes");
        // the command line
        let cli_xlsx = root.join("cli.xlsx");
        let args = crate::CliArgs::parse_from(["MicroParquet", "--headless", "--pack", "mini", "--slot", &format!("Deposits={}", deposits.display()), "--slot", &format!("Loans={}", loans.display()), "--cutoff", "2026-10-31", "--export-xlsx", &cli_xlsx.display().to_string()]);
        run_headless_cli(&args, &paths).expect("headless runs");
        // the Cover (sheet 1) is the same too, except the run time; then every item sheet, tool runs included
        let mask = |t: String| -> String {
            // "2026-11-19 14:03:07" -> "T"
            let b: Vec<char> = t.chars().collect();
            let mut out = String::new();
            let mut i = 0;
            while i < b.len() {
                let is_time = i + 19 <= b.len() && b[i..i + 19].iter().enumerate().all(|(k, c)| match k {
                    4 | 7 => *c == '-',
                    10 => *c == ' ',
                    13 | 16 => *c == ':',
                    _ => c.is_ascii_digit(),
                });
                if is_time {
                    out.push('T');
                    i += 19;
                } else {
                    out.push(b[i]);
                    i += 1;
                }
            }
            out
        };
        let strings = |p: &Path| mask(unzip(p, "xl/sharedStrings.xml"));
        let (sa, sb) = (strings(&ui_xlsx), strings(&cli_xlsx));
        assert!(!sa.is_empty() && sa.replace(">ui<", ">T<") == sb, "the Cover's text is the same from the command line (run time aside)");
        for n in 1..=5 {
            let e = format!("xl/worksheets/sheet{n}.xml");
            let (a, b) = (unzip(&ui_xlsx, &e), unzip(&cli_xlsx, &e));
            assert!(!a.is_empty() && a == b, "sheet {n} is the same from the command line");
        }
        let wb = |p: &Path| unzip(p, "xl/workbook.xml");
        assert_eq!(wb(&ui_xlsx), wb(&cli_xlsx), "the same sheets");
        // refusals: a missing slot, a bad --slot, no cutoff
        let bad = crate::CliArgs::parse_from(["MicroParquet", "--headless", "--pack", "mini", "--cutoff", "2026-10-31", "--export-xlsx", &root.join("x.xlsx").display().to_string()]);
        let e = format!("{:#}", run_headless_cli(&bad, &paths).expect_err("no slot file"));
        assert!(e.contains("'Deposits'"), "{e}");
        let bad = crate::CliArgs::parse_from(["MicroParquet", "--headless", "--pack", "mini", "--slot", "Deposits", "--cutoff", "2026-10-31", "--export-xlsx", "x.xlsx"]);
        assert!(format!("{:#}", run_headless_cli(&bad, &paths).expect_err("bad slot")).contains("<slot>=<file>"));
        let bad = crate::CliArgs::parse_from(["MicroParquet", "--headless", "--pack", "mini", "--slot", &format!("Deposits={}", deposits.display()), "--export-xlsx", "x.xlsx"]);
        assert!(format!("{:#}", run_headless_cli(&bad, &paths).expect_err("no cutoff")).contains("--cutoff"));
        assert!(!root.join("x.xlsx").exists());
        micro_parquet::rules::set_data_dir_override(None);
        let _ = std::fs::remove_dir_all(&root);
    }

    /// C6: `--export-html` writes the chart book of the run, the same file as the dialog's path; a chart
    /// that cannot be drawn stops the run and nothing is written.
    #[test]
    fn c6_headless_export_html_gives_the_same_chart_book_as_the_dialog_path_and_a_failing_item_writes_nothing() {
        let root = std::env::temp_dir().join(format!("mp_c6_headless_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("temp")).expect("folder");
        micro_parquet::rules::set_data_dir_override(Some(root.clone()));
        let paths = PortablePaths { root_dir: root.clone(), macros_dir: root.join("m"), schemas_dir: root.join("s"), curves_dir: root.join("c"), buckets_file: root.join("b.json"), temp_dir: root.join("temp") };
        let deposits = root.join("deposits.csv");
        std::fs::write(&deposits, "ACNO,BAL,MAT,PROD\nD1,100,2026-12-31,SB\nD2,50,2027-06-30,TD\nD3,25,2026-11-30,TD\n").expect("csv");
        let loans = root.join("loans.csv");
        std::fs::write(&loans, "ACNO,BAL,MAT\nL1,1000000,2027-03-31\nL2,2000000,2028-10-31\n").expect("loans");
        let gap = micro_parquet::tool_runs::ToolRun::GapLadder(micro_parquet::tool_runs::GapLadderRun {
            inputs: vec![
                micro_parquet::tool_runs::GapInputRun { source: "Loans".into(), spec: micro_parquet::banking::GapInputSpec { role: micro_parquet::banking::GapRole::Asset, amount_col: "BAL".into(), maturity_col: "MAT".into() }, product_col: String::new() },
                micro_parquet::tool_runs::GapInputRun { source: "Deposits".into(), spec: micro_parquet::banking::GapInputSpec { role: micro_parquet::banking::GapRole::Liability, amount_col: "BAL".into(), maturity_col: "MAT".into() }, product_col: String::new() },
            ],
            preset: CustomBucketDef::default_presets()[0].name.clone(),
            preset_def: None,
            cutoff: "2020-01-31".into(),
            null_as_sight: true,
            behaviour_file: String::new(),
            stress_tdrr: false,
        });
        let mut pack = ReportPack {
            name: "charted".into(),
            inputs: vec![
                PackInput { slot: "Deposits".into(), required_columns: vec!["ACNO".into(), "BAL".into(), "MAT".into(), "PROD".into()], ..Default::default() },
                PackInput { slot: "Loans".into(), required_columns: vec!["ACNO".into(), "BAL".into(), "MAT".into()], ..Default::default() },
            ],
            items: vec![PackItem { title: "Gap".into(), sheet: "Gap".into(), source: PackItemKind::Run { run: gap }, col_formats: Default::default(), chart: Some(micro_parquet::charts::pack::PackChart::Template { template: "gap-ladder".into() }) }],
            output: PackOutput::default(),
        };
        save_pack(&packs_dir(), &pack).expect("saves");
        let slots = |a: &Path, b: &Path| vec!["--slot".to_string(), format!("Deposits={}", a.display()), "--slot".to_string(), format!("Loans={}", b.display())];
        // the dialog's path
        let spec = PackRunSpec { files: std::collections::BTreeMap::from([("Deposits".to_string(), deposits.clone()), ("Loans".to_string(), loans.clone())]), cutoff: "2026-10-31".into() };
        let r = micro_parquet::jobs::run_inline(|ctx| run_pack(&pack, &spec, &paths, "ui", ctx)).expect("runs");
        let (ui_xlsx, ui_html) = (root.join("ui.xlsx"), root.join("ui.html"));
        micro_parquet::jobs::run_inline(|ctx| write_pack_with_charts(&r, &ui_xlsx, None, Some(&ui_html), ctx)).expect("writes");
        // the command line: --export-html names the book
        let (cli_xlsx, cli_html) = (root.join("cli.xlsx"), root.join("cli.html"));
        let mut argv: Vec<String> = ["MicroParquet", "--headless", "--pack", "charted"].iter().map(|s| s.to_string()).collect();
        argv.extend(slots(&deposits, &loans));
        argv.extend(["--cutoff", "2026-10-31", "--export-xlsx"].iter().map(|s| s.to_string()));
        argv.push(cli_xlsx.display().to_string());
        argv.push("--export-html".to_string());
        argv.push(cli_html.display().to_string());
        let args = crate::CliArgs::parse_from(argv.clone());
        run_headless_cli(&args, &paths).expect("headless runs");
        let (a, b) = (std::fs::read_to_string(&ui_html).expect("ui book"), std::fs::read_to_string(&cli_html).expect("cli book"));
        assert!(a.contains("class=\"mp-chart\"") && a == b, "the chart book is the same from the command line");
        // the chart book cannot be written (its folder is missing): the workbook is not left behind either
        let (bx, bh) = (root.join("book-fail.xlsx"), root.join("no-such-folder").join("book.html"));
        let mut argv5: Vec<String> = ["MicroParquet", "--headless", "--pack", "charted"].iter().map(|s| s.to_string()).collect();
        argv5.extend(slots(&deposits, &loans));
        argv5.extend(["--cutoff", "2026-10-31", "--export-xlsx"].iter().map(|s| s.to_string()));
        argv5.push(bx.display().to_string());
        argv5.push("--export-html".to_string());
        argv5.push(bh.display().to_string());
        let r5 = run_headless_cli(&crate::CliArgs::parse_from(argv5), &paths);
        assert!(r5.is_err(), "an unwritable chart book stops the run");
        assert!(!bx.exists(), "and the workbook is not left behind");
        for n in 1..=2 {
            let e = format!("xl/charts/chart{n}.xml");
            let (x, y) = (unzip(&ui_xlsx, &e), unzip(&cli_xlsx, &e));
            assert!(!x.is_empty() && x == y, "{e} is the same from the command line");
        }
        // the pack asks for the book: it goes beside the workbook as <pack>-charts.html
        pack.output.html_charts = true;
        save_pack(&packs_dir(), &pack).expect("saves");
        let beside = root.join("beside.xlsx");
        let mut argv2: Vec<String> = ["MicroParquet", "--headless", "--pack", "charted"].iter().map(|s| s.to_string()).collect();
        argv2.extend(slots(&deposits, &loans));
        argv2.extend(["--cutoff", "2026-10-31", "--export-xlsx"].iter().map(|s| s.to_string()));
        argv2.push(beside.display().to_string());
        run_headless_cli(&crate::CliArgs::parse_from(argv2), &paths).expect("headless runs");
        assert!(root.join("charted-charts.html").exists(), "<pack>-charts.html beside the workbook");
        // a pack without a chart cannot make a book
        pack.items[0].chart = None;
        save_pack(&packs_dir(), &pack).expect("saves");
        let (nx, nh) = (root.join("nochart.xlsx"), root.join("nochart.html"));
        let mut argv3: Vec<String> = ["MicroParquet", "--headless", "--pack", "charted"].iter().map(|s| s.to_string()).collect();
        argv3.extend(slots(&deposits, &loans));
        argv3.extend(["--cutoff", "2026-10-31", "--export-xlsx"].iter().map(|s| s.to_string()));
        argv3.push(nx.display().to_string());
        argv3.push("--export-html".to_string());
        argv3.push(nh.display().to_string());
        let e = format!("{:#}", run_headless_cli(&crate::CliArgs::parse_from(argv3), &paths).expect_err("no chart"));
        assert!(e.contains("no chart"), "{e}");
        assert!(!nx.exists() && !nh.exists(), "nothing is written");
        // an item whose saved chart is missing stops the run
        pack.items[0].chart = Some(micro_parquet::charts::pack::PackChart::Saved { name: "Nothing".into() });
        save_pack(&packs_dir(), &pack).expect("saves");
        let (fx, fh) = (root.join("fail.xlsx"), root.join("fail.html"));
        let mut argv4: Vec<String> = ["MicroParquet", "--headless", "--pack", "charted"].iter().map(|s| s.to_string()).collect();
        argv4.extend(slots(&deposits, &loans));
        argv4.extend(["--cutoff", "2026-10-31", "--export-xlsx"].iter().map(|s| s.to_string()));
        argv4.push(fx.display().to_string());
        argv4.push("--export-html".to_string());
        argv4.push(fh.display().to_string());
        let e = format!("{:#}", run_headless_cli(&crate::CliArgs::parse_from(argv4), &paths).expect_err("failing chart"));
        assert!(e.contains("'Gap'") && e.contains("Nothing"), "{e}");
        assert!(!fx.exists() && !fh.exists(), "nothing is written when an item fails");
        micro_parquet::rules::set_data_dir_override(None);
        let _ = std::fs::remove_dir_all(&root);
    }

    /// `--export-html` without `--pack` is an error, never silently ignored.
    #[test]
    fn r17_export_html_without_pack_is_an_error() {
        let root = std::env::temp_dir().join(format!("mp_r17_{}", std::process::id()));
        let paths = PortablePaths { root_dir: root.clone(), macros_dir: root.join("m"), schemas_dir: root.join("s"), curves_dir: root.join("c"), buckets_file: root.join("b.json"), temp_dir: root.join("temp") };
        let args = crate::CliArgs::parse_from(["MicroParquet", "--headless", "--input", "x.csv", "--export-html", "out.html"]);
        let e = format!("{:#}", run_headless_cli(&args, &paths).expect_err("refused"));
        assert!(e.contains("--export-html") && e.contains("--pack"), "{e}");
    }
}
