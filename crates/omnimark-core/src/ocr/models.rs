//! ocrs model files in `<models root>/ocrs/`: pinned URLs and checksums, download and
//! verification. Skeleton from Task M2-0; Task M2-2 implements it.

use std::path::{Path, PathBuf};

/// Sub-folder of the models root that holds the ocrs files.
pub const OCRS_DIR: &str = "ocrs";

pub fn ocrs_dir(models_root: &Path) -> PathBuf {
    models_root.join(OCRS_DIR)
}

/// True when every ocrs model file is present with its expected size.
pub fn ocrs_models_present(_models_root: &Path) -> bool {
    false
}
