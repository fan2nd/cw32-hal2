//! Test-only immutable chip snapshots preserve historical source assertions.
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

pub(crate) fn chip_path(root: &Path, name: &str) -> PathBuf {
    let live = root
        .join("cw32-data/data/chips")
        .join(format!("{name}.json"));
    let digest = match name {
        "CW32W031" => "1aaaab9824417c136b6509b6cba16f68f616ba09bb626d92340746ad44cfb075",
        "CW32W031R8U6" => "f8d4d0daf484dd615656e4ef1cffa69c17ff486ae43b8bb0a5ec3a137ca3980a",
        _ => return live,
    };
    assert!(cw32_data_serde::support::is_retired(name));
    assert!(
        !live.exists(),
        "retired target reappeared in generated metadata"
    );
    let historical = root.join("docs/retired-w031").join(format!("{name}.json"));
    assert_eq!(
        format!("{:x}", Sha256::digest(std::fs::read(&historical).unwrap())),
        digest
    );
    historical
}
