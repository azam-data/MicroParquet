//! Headless command-line mode (moved out of main.rs by Phase 3 R1).

use crate::ui::prelude::*;

pub(crate) fn run_headless_cli(args: &crate::CliArgs, paths: &PortablePaths) -> anyhow::Result<()> {
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
        None => MicroParquetApp::guess_balance_column(&lf.clone().limit(0).collect()?),
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
        export_to_parquet_stream(lf.clone(), out_parq, ParquetDialect::ArchiveZstd)?;
        println!("Exported Parquet: {:?}", out_parq);
    }
    if let Some(out_csv) = &args.export_csv {
        export_to_csv_stream(lf.clone(), out_csv, false)?;
        println!("Exported CSV: {:?}", out_csv);
    }
    if let Some(out_xlsx) = &args.export_xlsx {
        let df = lf.clone().collect()?;
        let rep = export_tabs_to_xlsx(vec![("Headless_Output".to_string(), df)], Vec::new(), out_xlsx)?;
        println!("Exported Excel: {:?} ({})", out_xlsx, rep.summary());
    }
    if let Some(out_pdf) = &args.export_pdf {
        let df = lf.collect()?;
        let rep = export_tabs_to_pdf(
            vec![("Headless_Report".to_string(), df, HashMap::new())],
            out_pdf,
            NumberDisplayFormat::BdtCrore,
            2,
        )?;
        println!("Exported PDF: {:?}", out_pdf);
        let s = rep.summary();
        if !s.is_empty() {
            eprintln!("WARNING (PDF): {}", s);
        }
    }

    Ok(())
}
