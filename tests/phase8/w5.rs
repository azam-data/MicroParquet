//! W5: linked, editable chart tabs. The window's behaviour is tested in `src/ui/render_tests_p8.rs` (`w5_…`); here are
//! the parts that need no window: a Phase 7 `settings.json` with chart tabs loads and keeps them, and the chart a chart
//! tab is calculated from is the source's own (never its table).

use crate::common::dir;
use micro_parquet::charts::spec::{ChartAgg, ChartSpec};
use micro_parquet::models::*;
use micro_parquet::project::chart_tab_source;
use micro_parquet::settings::Settings;
use polars::prelude::*;

/// The shape Phase 7 wrote: a title, the tab's key and the chart's own JSON (no Phase 8 field in it).
const PHASE7_SETTINGS: &str = r#"{
  "right_panel_width": 420.0,
  "chart_tabs": [
    {"title": "Chart: Sum of Balance by Term period", "source": "Loans|3", "spec": {"category": "Term period", "series": null, "value": "Balance", "weight": null, "agg": "Sum", "kind": "Bar", "order": "Natural", "by_value": false, "top_n": 12}}
  ]
}"#;

#[test]
fn w5_a_phase7_settings_file_with_chart_tabs_loads_and_keeps_them() {
    let d = dir("w5_settings");
    std::fs::write(d.join("settings.json"), PHASE7_SETTINGS).expect("old settings");
    let (s, problem) = Settings::load(&d);
    assert!(problem.is_none(), "{problem:?}");
    assert_eq!(s.chart_tabs.len(), 1);
    // its chart reads with every Phase 8 field at its default
    let spec: ChartSpec = serde_json::from_value(s.chart_tabs[0]["spec"].clone()).expect("the Phase 7 chart reads");
    assert_eq!((spec.category.as_str(), spec.value.as_deref(), spec.agg), ("Term period", Some("Balance"), ChartAgg::Sum));
    assert!(spec.series_order.is_none() && spec.facet_order.is_none() && spec.category_part.is_none());
    // saving it again keeps the entry as it was
    s.save(&d).expect("saves");
    let (again, _) = Settings::load(&d);
    assert_eq!(again.chart_tabs, s.chart_tabs);
}

fn loans() -> DataFrame {
    DataFrame::new(vec![
        Series::new("Term period".into(), &["1Y", "2Y", "1Y", "3Y", "2Y", "1Y"]).into(),
        Series::new("Balance".into(), &[100.0f64, 200.0, 300.0, 400.0, 500.0, 600.0]).into(),
        Series::new("Branch".into(), &["A", "A", "B", "B", "A", "B"]).into(),
    ])
    .expect("loans")
}

#[test]
fn w5_the_chart_of_the_owners_case_is_calculated_from_the_source_and_reads_sum_of_balance() {
    let d = dir("w5_owner");
    let path = d.join("loans.parquet");
    let mut df = loans();
    ParquetWriter::new(std::fs::File::create(&path).expect("file")).finish(&mut df).expect("parquet");
    let source = TabSource::SingleFile { path, is_parquet: true, config: IngestionConfig::default() };
    let spec = ChartSpec { category: "Term period".into(), value: Some("Balance".into()), agg: ChartAgg::Sum, ..Default::default() };
    let (data, table_source) = chart_tab_source(&source, &[], &spec, &d.join("temp")).expect("the chart");
    assert_eq!(data.measure_names, vec!["Sum of Balance".to_string()]);
    assert_eq!(data.categories, vec!["1Y", "2Y", "3Y"]);
    assert_eq!(data.values[0], vec![Some(1000.0), Some(700.0), Some(400.0)]);
    // the same chart over the chart tab's own table would add up the sums ("Sum of Sum of Balance"); the chart is
    // calculated from the source, so a changed source gives numbers of the source
    let filter = TransformStep::FilterGroup { conditions: vec![FilterCondition { column: "Branch".into(), op: CondOp::Equals, value: "B".into(), ..Default::default() }], join: GroupJoin::And, keep_matching: true };
    let (b_only, _) = chart_tab_source(&source, &[filter], &spec, &d.join("temp")).expect("the chart of the filtered source");
    assert_eq!(b_only.measure_names, vec!["Sum of Balance".to_string()]);
    assert_eq!(b_only.values[0].iter().flatten().sum::<f64>(), 1300.0);
    // the chart tab's table is the chart's numbers, one row per category
    let TabSource::SingleFile { path, .. } = table_source else { panic!("a file") };
    let table = LazyFrame::scan_parquet(&path, Default::default()).expect("scan").collect().expect("table");
    assert_eq!(table.height(), 3);
}
