use polars::prelude::DataFrame;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Automatically initializes the zero-registry portable directory next to the executable
#[derive(Clone, Debug)]
pub struct PortablePaths {
    pub root_dir: PathBuf,
    pub macros_dir: PathBuf,
    pub schemas_dir: PathBuf,
    pub curves_dir: PathBuf,
    pub temp_dir: PathBuf,
}

impl PortablePaths {
    pub fn init() -> Self {
        let exe_dir = std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(|p| p.to_path_buf()))
            .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));

        let root_dir = exe_dir.join("portable_data");
        let macro_folder = root_dir.join("macros");
        let schema_folder = root_dir.join("schemas");
        let curve_folder = root_dir.join("curves");
        let temp_folder = root_dir.join("temp_scratch");

        let _ = std::fs::create_dir_all(¯o_folder);
        let _ = std::fs::create_dir_all(&schema_folder);
        let _ = std::fs::create_dir_all(&curve_folder);
        let _ = std::fs::create_dir_all(&temp_folder);

        std::env::set_var("POLARS_TEMP_DIR", &temp_folder);

        Self {
            root_dir,
            macros_dir: macro_folder,
            schemas_dir: schema_folder,
            curves_dir: curve_folder,
            temp_dir: temp_folder,
        }
    }
}
