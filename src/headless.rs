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
    let spec = PackRunSpec { files, cutoff };
    let msg = micro_parquet::jobs::run_inline(|ctx| {
        let r = run_pack(&pack, &spec, paths, &run_time_now(), ctx)?;
        write_pack(&r, &xlsx, pdf.as_deref(), ctx)
    })?;
    println!("{}", msg);
    Ok(())
}

pub(crate) fn run_headless_cli(args: &crate::CliArgs, paths: &PortablePaths) -> anyhow::Result<()> {
    if args.pack.is_some() {
        return run_pack_cli(args, paths);
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
        std::fs::write(&deposits, "ACNO,BAL,PROD\nD1,100,SB\nD2,50,TD\nD3,25,TD\n").expect("csv");
        let pivot = TransformStep::from_pivot_spec(PivotSpec { rows: vec!["PROD".into()], values: vec![PivotValue { field: "BAL".into(), agg: PivotAgg::Sum, weight_col: None }], row_totals: true, column_totals: true, ..Default::default() });
        let pack = ReportPack {
            name: "mini".into(),
            inputs: vec![PackInput { slot: "Deposits".into(), required_columns: vec!["ACNO".into(), "BAL".into(), "PROD".into()], ..Default::default() }],
            items: vec![
                PackItem { title: "Table".into(), sheet: "Table".into(), source: PackItemKind::Steps { slot: "Deposits".into() }, col_formats: Default::default() },
                PackItem { title: "By product".into(), sheet: "By product".into(), source: PackItemKind::Pivot { slot: "Deposits".into(), pivot: Some(pivot) }, col_formats: Default::default() },
            ],
            output: PackOutput::default(),
        };
        save_pack(&packs_dir(), &pack).expect("saves");
        // the library path (what the dialog's worker does)
        let spec = PackRunSpec { files: std::collections::BTreeMap::from([("Deposits".to_string(), deposits.clone())]), cutoff: "2026-10-31".into() };
        let r = micro_parquet::jobs::run_inline(|ctx| run_pack(&pack, &spec, &paths, "ui", ctx)).expect("runs");
        let ui_xlsx = root.join("ui.xlsx");
        micro_parquet::jobs::run_inline(|ctx| write_pack(&r, &ui_xlsx, None, ctx)).expect("writes");
        // the command line
        let cli_xlsx = root.join("cli.xlsx");
        let args = crate::CliArgs::parse_from(["MicroParquet", "--headless", "--pack", "mini", "--slot", &format!("Deposits={}", deposits.display()), "--cutoff", "2026-10-31", "--export-xlsx", &cli_xlsx.display().to_string()]);
        run_headless_cli(&args, &paths).expect("headless runs");
        for n in 2..=3 {
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
}
