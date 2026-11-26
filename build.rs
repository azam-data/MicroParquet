//! Build script: records the build date (UTC, YYYY-MM-DD) for Help > About MicroParquet.
//! `SOURCE_DATE_EPOCH` (seconds since 1970) makes a build reproducible.
//! On Windows it also embeds the app icon (`assets/icon/microparquet.ico`) into the .exe.

fn civil_from_days(z: i64) -> (i64, u32, u32) {
    // days since 1970-01-01 to a calendar date (Howard Hinnant's algorithm)
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

/// Phase 14: the guide notes (`docs/guide/*.md`) are compiled into the program, so the assistant can search
/// them on a PC that has no docs folder. Writes `$OUT_DIR/guide_embedded.rs`, a list of (file name, text).
fn embed_guide() {
    println!("cargo:rerun-if-changed=docs/guide");
    let manifest = std::env::var("CARGO_MANIFEST_DIR").unwrap_or_else(|_| ".".to_string());
    let dir = std::path::Path::new(&manifest).join("docs").join("guide");
    let mut names: Vec<String> = std::fs::read_dir(&dir)
        .map(|rd| {
            rd.filter_map(|e| e.ok())
                .map(|e| e.file_name().to_string_lossy().to_string())
                .filter(|n| n.ends_with(".md"))
                .collect()
        })
        .unwrap_or_default();
    names.sort();
    let mut out = String::from("pub const GUIDE_FILES: &[(&str, &str)] = &[\n");
    for n in &names {
        println!("cargo:rerun-if-changed=docs/guide/{}", n);
        let abs = dir.join(n).to_string_lossy().replace('\\', "/");
        out.push_str(&format!("    ({:?}, include_str!({:?})),\n", n, abs));
    }
    out.push_str("];\n");
    let out_dir = std::env::var("OUT_DIR").expect("cargo sets OUT_DIR");
    std::fs::write(std::path::Path::new(&out_dir).join("guide_embedded.rs"), out).expect("the guide list could not be written");
}

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-env-changed=SOURCE_DATE_EPOCH");
    let secs = std::env::var("SOURCE_DATE_EPOCH")
        .ok()
        .and_then(|v| v.trim().parse::<i64>().ok())
        .or_else(|| std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).ok().map(|d| d.as_secs() as i64))
        .unwrap_or(0);
    let (y, m, d) = civil_from_days(secs.div_euclid(86_400));
    println!("cargo:rustc-env=MP_BUILD_DATE={:04}-{:02}-{:02}", y, m, d);
    embed_guide();

    // The .exe's own icon (Explorer, a pinned taskbar shortcut); the window icon is set at run time.
    #[cfg(windows)]
    {
        println!("cargo:rerun-if-changed=assets/icon/microparquet.ico");
        let mut res = winresource::WindowsResource::new();
        res.set_icon("assets/icon/microparquet.ico");
        if let Err(e) = res.compile() {
            panic!("Could not embed the Windows icon (assets/icon/microparquet.ico): {e}");
        }
    }
}
