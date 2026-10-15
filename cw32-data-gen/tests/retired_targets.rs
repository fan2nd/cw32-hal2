//! Retirement is a normal-generation admission decision, not an input rename.
use std::{fs, path::Path};

#[test]
fn public_generation_rejects_retired_manifest_before_output() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let output = std::env::temp_dir().join(format!("cw32-retired-data-{}", std::process::id()));
    assert!(!output.exists());
    let error =
        cw32_data_gen::generate(root, &root.join("cw32-data/inputs/cw32w031.yaml"), &output)
            .unwrap_err();
    assert!(error.to_string().contains("retired target"), "{error:#}");
    assert!(!output.exists());
    // Renaming the input file cannot restore support.
    let manifest =
        std::env::temp_dir().join(format!("cw32-retired-input-{}.yaml", std::process::id()));
    fs::copy(root.join("cw32-data/inputs/cw32w031.yaml"), &manifest).unwrap();
    let error = cw32_data_gen::generate(root, &manifest, &output).unwrap_err();
    assert!(error.to_string().contains("retired target"));
    assert!(!output.exists());
    fs::remove_file(manifest).unwrap();
}

#[test]
fn embedded_and_catalog_identities_cannot_bypass_retirement() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let work = std::env::temp_dir().join(format!("cw32-retired-identities-{}", std::process::id()));
    fs::create_dir_all(&work).unwrap();
    let original: serde_yaml::Value =
        serde_yaml::from_slice(&fs::read(root.join("cw32-data/inputs/cw32l031.yaml")).unwrap())
            .unwrap();
    let manifest = work.join("input.yaml");
    let output = work.join("output");
    for identity in ["line", "die", "chip"] {
        let mut input = original.clone();
        if identity == "chip" {
            input["chips"][0]["name"] = "CW32W031R8U6".into();
        } else {
            input[identity] = "CW32W031".into();
        }
        fs::write(&manifest, serde_yaml::to_string(&input).unwrap()).unwrap();
        let error = cw32_data_gen::generate(root, &manifest, &output).unwrap_err();
        assert!(
            error.to_string().contains("retired target"),
            "{identity}: {error:#}"
        );
        assert!(!output.exists());
    }
    let mut catalog: serde_yaml::Value =
        serde_yaml::from_slice(&fs::read(root.join("cw32-data/parts.yaml")).unwrap()).unwrap();
    let part = catalog["parts"]
        .as_sequence_mut()
        .unwrap()
        .iter_mut()
        .find(|part| part["name"] == "CW32W031R8U6")
        .unwrap();
    part["family"] = "CW32L031".into();
    let catalog_path = work.join("parts.yaml");
    fs::write(&catalog_path, serde_yaml::to_string(&catalog).unwrap()).unwrap();
    let mut input = original;
    input["parts_catalog"] = catalog_path.to_str().unwrap().into();
    fs::write(&manifest, serde_yaml::to_string(&input).unwrap()).unwrap();
    let error = cw32_data_gen::generate(root, &manifest, &output).unwrap_err();
    assert!(
        error
            .to_string()
            .contains("retired target in expanded part catalog"),
        "{error:#}"
    );
    assert!(!output.exists());
    fs::remove_dir_all(work).unwrap();
}
