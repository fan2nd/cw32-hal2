//! Stale or renamed historical chip JSON cannot restore a normal PAC target.
use cw32_metapac_gen::{Gen, Options};
use std::{
    fs,
    panic::{AssertUnwindSafe, catch_unwind},
};

#[test]
fn retired_names_and_renamed_metadata_fail_before_output() {
    let root = std::env::temp_dir().join(format!("cw32-retired-pac-{}", std::process::id()));
    fs::create_dir_all(root.join("data/chips")).unwrap();
    for name in ["CW32W031", "CW32W031R8U6"] {
        let output = root.join(name);
        let result = catch_unwind(AssertUnwindSafe(|| {
            Gen::new(Options {
                chips: vec![name.into()],
                data_dir: root.join("data"),
                out_dir: output.clone(),
            })
            .run_gen()
        }));
        let error = result.expect_err("retired metadata unexpectedly generated");
        let message = error
            .downcast_ref::<String>()
            .map(String::as_str)
            .or_else(|| error.downcast_ref::<&str>().copied())
            .unwrap_or("");
        assert!(
            message.contains("retired target"),
            "wrong failure: {message}"
        );
        assert!(!output.exists());
    }
    for (name, line) in [("CW32W031R8U6", "CW32L031"), ("CW32TEST", "CW32W031")] {
        fs::write(root.join("data/chips/CW32TEST.json"), serde_json::json!({
            "name": name, "family": "CW32L0", "line": line, "cores": [], "memory": [], "packages": []
        }).to_string()).unwrap();
        let output = root.join("renamed");
        let result = catch_unwind(AssertUnwindSafe(|| {
            Gen::new(Options {
                chips: vec!["CW32TEST".into()],
                data_dir: root.join("data"),
                out_dir: output.clone(),
            })
            .run_gen()
        }));
        let error = result.expect_err("retired metadata unexpectedly generated");
        let message = error
            .downcast_ref::<String>()
            .map(String::as_str)
            .or_else(|| error.downcast_ref::<&str>().copied())
            .unwrap_or("");
        assert!(
            message.contains("retired target"),
            "wrong failure: {message}"
        );
        assert!(!output.exists());
    }
    fs::remove_dir_all(root).unwrap();
}
