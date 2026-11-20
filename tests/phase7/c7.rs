//! C7: "Copy as Python". For each form the script parses as Python (`python3 -c "import ast"`, skipped with
//! a note when python3 is absent), holds the chart's numbers, names matplotlib (plotly for a Sankey and a
//! treemap) and uses the house palette. Where matplotlib and pandas are installed the matplotlib scripts
//! are also run (headless backend) and must finish without an error.

use crate::p7_c3::{forms, opts};
use crate::p7_f::every_chart;
use micro_parquet::chart::ChartKind;
use micro_parquet::charts::data::{heatmap, waterfall_from_data, ChartData};
use micro_parquet::charts::python::chart_to_python;
use micro_parquet::charts::spec::Transform;
use micro_parquet::charts::tokens as t;

fn python() -> bool {
    std::process::Command::new("python3").arg("--version").output().map(|o| o.status.success()).unwrap_or(false)
}

fn has_modules(mods: &str) -> bool {
    std::process::Command::new("python3").arg("-c").arg(format!("import {mods}")).output().map(|o| o.status.success()).unwrap_or(false)
}

fn parses(script: &str, name: &str) {
    let dir = std::env::temp_dir().join(format!("mp-c7-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join(format!("{}.py", name.replace([' ', '/', '(', ')'], "_")));
    std::fs::write(&path, script).unwrap();
    let o = std::process::Command::new("python3").arg("-I").arg("-c").arg("import ast,sys; ast.parse(open(sys.argv[1], encoding='utf-8').read())").arg(&path).output().expect("python3 runs");
    assert!(o.status.success(), "{name}: the script is not valid Python:\n{}", String::from_utf8_lossy(&o.stderr));
}

/// The numbers a script must hold for this chart.
fn numbers(d: &ChartData) -> Vec<f64> {
    let mut v: Vec<f64> = Vec::new();
    match d.spec.kind {
        ChartKind::Scatter | ChartKind::Bubble => {
            for p in &d.points {
                v.push(p.x);
                v.push(p.y);
            }
        }
        ChartKind::BoxPlot => {
            for b in &d.boxes {
                v.extend([b.q1, b.median, b.q3, b.low_whisker, b.high_whisker]);
                v.extend(b.outliers.iter().copied());
            }
        }
        ChartKind::Waterfall => {
            for s in waterfall_from_data(d).unwrap() {
                v.extend([s.value, s.base, s.top]);
            }
        }
        ChartKind::Heatmap => {
            for r in heatmap(d, Transform::None).cells {
                v.extend(r.into_iter().flatten());
            }
        }
        ChartKind::Sankey | ChartKind::Treemap => v.extend(d.values.iter().flatten().flatten().copied().filter(|x| *x > 0.0)),
        _ if !d.facets.is_empty() => {
            // small multiples: each facet's own table (the top-level table is the whole, which is not drawn)
            for f in &d.facets {
                v.extend(f.data.values.iter().flatten().flatten().copied());
            }
        }
        _ => {
            for m in 0..d.measure_count() {
                if let Some(md) = d.measure(m) {
                    v.extend(md.values.iter().flatten().flatten().copied());
                }
            }
        }
    }
    v
}

#[test]
fn c7_every_form_gives_a_python_script_that_parses_and_holds_its_numbers() {
    let have_python = python();
    if !have_python {
        eprintln!("python3 is not installed: the scripts are checked for their text only");
    }
    let run = have_python && has_modules("matplotlib, pandas");
    let run_plotly = have_python && has_modules("plotly, pandas");
    if !run {
        eprintln!("matplotlib and pandas are not installed: the scripts are parsed, not run");
    }
    let mut count = 0;
    for (name, d) in every_chart() {
        let script = match chart_to_python(&d, &opts()) {
            Ok(s) => s,
            Err(e) => panic!("{name}: {e:#}"),
        };
        count += 1;
        // the library the script names, and the way to install it
        let plotly = matches!(d.spec.kind, ChartKind::Sankey | ChartKind::Treemap);
        if plotly {
            assert!(script.contains("import plotly.graph_objects as go") && script.contains("plotly"), "{name}: a Sankey and a treemap use plotly");
            assert!(script.contains("no matplotlib form"), "{name}: the comment says why");
        } else {
            assert!(script.contains("import matplotlib.pyplot as plt") && script.contains("pip install pandas matplotlib"), "{name}");
        }
        // the house style: the palette, the fonts, the title
        assert!(script.contains(&format!("\"{}\"", t::hex_text(t::SERIES_LIGHT[0]))) && script.contains("Source Sans 3"), "{name}");
        assert!(script.contains("TITLE = ") && script.contains("pd.DataFrame"), "{name}");
        // the data
        for v in numbers(&d) {
            let text = format!("{:?}", v);
            assert!(script.contains(&text), "{name}: the number {text} is in the script");
        }
        for c in &d.categories {
            assert!(script.contains(&format!("\"{}\"", c.replace('"', "\\\""))), "{name}: the category {c} is in the script");
        }
        if have_python {
            parses(&script, &name);
        }
        let dir = std::env::temp_dir().join(format!("mp-c7-{}", std::process::id()));
        let path = dir.join(format!("{}.py", name.replace([' ', '/', '(', ')'], "_")));
        if run && !plotly {
            // run it with the headless backend; with MP_PY_OUT=<folder> the figure is also saved as a PNG to look at
            let out = std::env::var("MP_PY_OUT").ok();
            let png = out.as_ref().map(|d| {
                std::fs::create_dir_all(d).unwrap();
                format!("{}/{}.png", d, name.replace([' ', '/', '(', ')'], "_"))
            });
            let wrapper = format!(
                "import matplotlib\nmatplotlib.use('Agg')\nimport sys\nsrc = open(sys.argv[1], encoding='utf-8').read()\nexec(compile(src, sys.argv[1], 'exec'), {{'__name__': '__main__'}})\nimport matplotlib.pyplot as plt\nif len(sys.argv) > 2:\n    plt.gcf().savefig(sys.argv[2], dpi=100)\n"
            );
            let mut cmd = std::process::Command::new("python3");
            cmd.arg("-c").arg(wrapper).arg(&path).env("MPLBACKEND", "Agg");
            if let Some(p) = &png {
                cmd.arg(p);
            }
            let o = cmd.output().expect("python3 runs");
            assert!(o.status.success(), "{name}: the script runs:\n{}", String::from_utf8_lossy(&o.stderr));
        }
        if run_plotly && plotly {
            // the plotly scripts build their figure; show() is replaced by to_json(), so no browser opens
            let wrapper = "import sys\nimport plotly.basedatatypes as b\nb.BaseFigure.show = lambda self, *a, **k: self.to_json()\nsrc = open(sys.argv[1], encoding='utf-8').read()\nexec(compile(src, sys.argv[1], 'exec'), {'__name__': '__main__'})\n";
            let o = std::process::Command::new("python3").arg("-c").arg(wrapper).arg(&path).output().expect("python3 runs");
            assert!(o.status.success(), "{name}: the plotly script runs:\n{}", String::from_utf8_lossy(&o.stderr));
        }
    }
    assert!(count >= 30, "{count} charts were checked");
}

#[test]
fn c7_the_script_keeps_the_marks_of_the_house_style_and_the_chart_s_own_details() {
    let forms = forms();
    let pick = |n: &str| forms.iter().find(|(k, _)| *k == n).unwrap().1.clone();
    // a reference line in its status colour with its label, on the horizontal bar form
    let s = chart_to_python(&pick("bar"), &opts()).unwrap();
    assert!(s.contains("axvline(150.0") && s.contains(&format!("\"{}\"", t::hex_text(t::STATUS_CRITICAL))) && s.contains("Limit (Critical)"), "{s}");
    // stacked: the form says so; percent too
    assert!(chart_to_python(&pick("stacked"), &opts()).unwrap().contains("stack=\"stacked\""));
    assert!(chart_to_python(&pick("percent"), &opts()).unwrap().contains("stack=\"percent\""));
    // a stepped line, and the area opacities (10% plain, 35% stacked)
    assert!(chart_to_python(&pick("stepped"), &opts()).unwrap().contains("stepped=True"));
    let s = chart_to_python(&pick("stacked-area"), &opts()).unwrap();
    assert!(s.contains("alpha=0.35") && s.contains("alpha=0.10"));
    // linked panels: two axes, small multiples: one panel per facet
    assert!(chart_to_python(&pick("linked-panels"), &opts()).unwrap().contains("plt.subplots(2, 1, sharex=True"));
    assert!(chart_to_python(&pick("small-multiples"), &opts()).unwrap().contains("facets["));
    // the dark theme flag follows the chart's theme
    let mut o = opts();
    o.dark = true;
    assert!(chart_to_python(&pick("column"), &o).unwrap().contains("DARK = True"));
    assert!(chart_to_python(&pick("column"), &opts()).unwrap().contains("DARK = False"));
    // the heatmap's dark ramp runs the other way
    assert!(chart_to_python(&pick("heatmap"), &opts()).unwrap().contains("RAMP = RAMP[::-1]"));
    // strings are escaped: a quote and a backslash in a title or a category cannot break the script
    let mut d = pick("column");
    d.spec.title = "A \"quoted\" title \\ with a backslash".into();
    let s = chart_to_python(&d, &opts()).unwrap();
    assert!(s.contains("A \\\"quoted\\\" title \\\\ with a backslash"), "{s}");
    if python() {
        parses(&s, "escaped");
    }
    // nothing to chart is refused with the reason
    let e = chart_to_python(&ChartData::default(), &opts()).unwrap_err().to_string();
    assert!(e.contains("No data to chart"), "{e}");
}
