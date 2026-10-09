//! Public-boundary tests with isolated synthetic fixtures, not asserted CW32 facts.
//! The real vendor inventories are checked independently by the Python auditors.
use cw32_data_gen::{generate, import_register_candidates};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};

static NEXT: AtomicU64 = AtomicU64::new(0);

struct Fixture {
    root: PathBuf,
    manifest: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "cw32-generation-boundary-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(root.join("cw32-data/registers")).unwrap();
        fs::write(
            root.join("cw32-data/field-access.yaml"),
            "schema_version: 1\nregisters: {}\n",
        )
        .unwrap();
        // Normal generation validates the shared catalog even when the synthetic
        // family has no reference-divider profile.
        fs::copy(
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../cw32-data/reference-dividers.yaml"),
            root.join("cw32-data/reference-dividers.yaml"),
        )
        .unwrap();
        let manifest = root.join("input.yaml");
        let fixture = Self { root, manifest };
        fixture.write_profile("CW32TEST", "v1", 0);
        fs::write(
            fixture.root.join("cw32-data/registers/uart_v1.yaml"),
            r#"block/UART:
  description: Reviewed peripheral description
  items:
  - name: CTRL
    description: Reviewed register description
    byte_offset: 4
    fieldset: CTRL
fieldset/CTRL:
  fields:
  - name: ENABLE
    description: Reviewed field name and position
    bit_offset: 3
    bit_size: 1
"#,
        )
        .unwrap();
        fixture
    }

    fn write_profile(&self, chip: &str, version: &str, offset: u32) {
        let source = format!(
            r#"<device schemaVersion="1.3">
<name>{chip}</name><version>1</version><description>Synthetic fixture</description>
<addressUnitBits>8</addressUnitBits><width>32</width><size>32</size><access>read-write</access>
<peripherals><peripheral><name>UART1</name><description>Vendor description</description>
<headerStructName>UART</headerStructName><baseAddress>0x40000000</baseAddress>
<registers><register><name>CTRL</name><description>Vendor register</description>
<addressOffset>{offset}</addressOffset><fields><field><name>EN</name><description>Vendor field</description>
<bitOffset>0</bitOffset><bitWidth>1</bitWidth></field></fields></register></registers>
</peripheral></peripherals></device>"#
        );
        fs::write(self.root.join("fixture.svd"), &source).unwrap();
        let manifest = json!({
            "schema_version": 1,
            "source": {"path": "fixture.svd", "sha256": format!("{:x}", Sha256::digest(source.as_bytes())),
                       "url": "https://example.invalid/synthetic-fixture.svd"},
            "expected_svd_name": chip, "core": "cm0p", "die": chip, "chip_prefix": "CW32TEST",
            "family": "CW32TEST", "line": chip, "register_version": version,
            "nvic_priority_bits": 2, "nvic_priority_bits_evidence": "Synthetic test fixture only",
            "chips": [{"name": chip, "memory": []}]
        });
        fs::write(&self.manifest, serde_yaml::to_string(&manifest).unwrap()).unwrap();
    }

    fn source_path(&self) -> PathBuf {
        self.root.join("cw32-data/registers/uart_v1.yaml")
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

#[test]
fn normal_generation_preserves_reviewed_names_offsets_and_field_positions() {
    let fixture = Fixture::new();
    let authored = fs::read(fixture.source_path()).unwrap();
    let out = fixture.root.join("generated");
    generate(&fixture.root, &fixture.manifest, &out).unwrap();
    let ir: Value =
        serde_json::from_slice(&fs::read(out.join("registers/uart_v1.json")).unwrap()).unwrap();
    assert_eq!(ir["block/UART"]["items"][0]["byte_offset"], 4);
    assert_eq!(ir["fieldset/CTRL"]["fields"][0]["name"], "ENABLE");
    assert_eq!(ir["fieldset/CTRL"]["fields"][0]["bit_offset"], 3);
    assert_eq!(fs::read(fixture.source_path()).unwrap(), authored);
    // A second run must still use the authored definitions, not reset to the SVD.
    generate(&fixture.root, &fixture.manifest, &out).unwrap();
    assert_eq!(fs::read(fixture.source_path()).unwrap(), authored);
}

#[test]
fn changed_authored_ir_needs_fresh_output_and_shared_ip_conflicts_still_fail() {
    let fixture = Fixture::new();
    let output = fixture.root.join("generated");
    generate(&fixture.root, &fixture.manifest, &output).unwrap();
    let old = fs::read(output.join("registers/uart_v1.json")).unwrap();
    let changed = fs::read_to_string(fixture.source_path())
        .unwrap()
        .replace("bit_offset: 3", "bit_offset: 5");
    fs::write(fixture.source_path(), &changed).unwrap();
    let error = generate(&fixture.root, &fixture.manifest, &output).unwrap_err();
    assert!(
        error
            .to_string()
            .contains("conflicting profiles attempted to share register version uart_v1"),
        "{error:#}"
    );
    assert_eq!(
        fs::read(output.join("registers/uart_v1.json")).unwrap(),
        old
    );
    let fresh = fixture.root.join("fresh-output");
    generate(&fixture.root, &fixture.manifest, &fresh).unwrap();
    let ir: Value =
        serde_json::from_slice(&fs::read(fresh.join("registers/uart_v1.json")).unwrap()).unwrap();
    assert_eq!(ir["fieldset/CTRL"]["fields"][0]["bit_offset"], 5);
    assert_eq!(fs::read_to_string(fixture.source_path()).unwrap(), changed);
}

#[test]
fn refreshing_vendor_candidates_never_promotes_them_into_authored_sources() {
    let fixture = Fixture::new();
    // A curated-only capability reference must neither block a raw candidate
    // import nor get advertised as reviewed metadata by that import.
    let mut profile: Value = serde_yaml::from_slice(&fs::read(&fixture.manifest).unwrap()).unwrap();
    profile["adc_sequence_metadata"] = "missing-curated-sequence.yaml".into();
    fs::write(&fixture.manifest, serde_yaml::to_string(&profile).unwrap()).unwrap();
    let authored = fs::read(fixture.source_path()).unwrap();
    let out = fixture.root.join("candidates");
    import_register_candidates(&fixture.root, &fixture.manifest, &out).unwrap();
    let ir: Value =
        serde_json::from_slice(&fs::read(out.join("registers/uart_v1.json")).unwrap()).unwrap();
    assert_eq!(ir["block/UART"]["items"][0]["byte_offset"], 0);
    assert_eq!(ir["fieldset/CTRL"]["fields"][0]["name"], "EN");
    assert!(!out.join("chips").exists());
    assert!(
        generate(
            &fixture.root,
            &fixture.manifest,
            &fixture.root.join("normal")
        )
        .is_err()
    );
    assert_eq!(fs::read(fixture.source_path()).unwrap(), authored);
}

#[test]
fn differing_source_layouts_cannot_share_one_candidate_version() {
    let fixture = Fixture::new();
    let out = fixture.root.join("candidates");
    import_register_candidates(&fixture.root, &fixture.manifest, &out).unwrap();
    fixture.write_profile("CW32TEST2", "v1", 4);
    let error = import_register_candidates(&fixture.root, &fixture.manifest, &out).unwrap_err();
    assert!(
        error.to_string().contains("conflicting profiles"),
        "{error:#}"
    );
}

#[test]
fn differing_source_layouts_are_accepted_as_distinct_versions() {
    let fixture = Fixture::new();
    let out = fixture.root.join("candidates");
    import_register_candidates(&fixture.root, &fixture.manifest, &out).unwrap();
    fixture.write_profile("CW32TEST2", "v2", 4);
    import_register_candidates(&fixture.root, &fixture.manifest, &out).unwrap();
    let first: Value =
        serde_json::from_slice(&fs::read(out.join("registers/uart_v1.json")).unwrap()).unwrap();
    let second: Value =
        serde_json::from_slice(&fs::read(out.join("registers/uart_v2.json")).unwrap()).unwrap();
    assert_ne!(first, second);
}

#[test]
fn candidate_output_cannot_be_curated_directory_or_dot_dot_alias() {
    let fixture = Fixture::new();
    let authored = fs::read(fixture.source_path()).unwrap();
    fs::create_dir_all(fixture.root.join("intermediate")).unwrap();
    for out in [
        fixture.root.join("cw32-data"),
        fixture.root.join("intermediate/../cw32-data"),
    ] {
        let error = import_register_candidates(&fixture.root, &fixture.manifest, &out).unwrap_err();
        assert!(error.to_string().contains("curated"), "{error:#}");
        assert_eq!(fs::read(fixture.source_path()).unwrap(), authored);
    }
}

#[test]
#[cfg(unix)]
fn candidate_output_cannot_be_relative_path_into_curated_directory() {
    let fixture = Fixture::new();
    let authored = fs::read(fixture.source_path()).unwrap();
    let depth = std::env::current_dir().unwrap().components().count() - 1;
    let relative = PathBuf::from("../".repeat(depth))
        .join(fixture.root.strip_prefix("/").unwrap())
        .join("cw32-data");
    assert!(!relative.is_absolute());
    let error =
        import_register_candidates(&fixture.root, &fixture.manifest, &relative).unwrap_err();
    assert!(error.to_string().contains("curated"), "{error:#}");
    assert_eq!(fs::read(fixture.source_path()).unwrap(), authored);
}

#[test]
#[cfg(unix)]
fn candidate_output_cannot_be_symlink_into_curated_directory() {
    let fixture = Fixture::new();
    let authored = fs::read(fixture.source_path()).unwrap();
    let link = fixture.root.join("source-alias");
    std::os::unix::fs::symlink(fixture.root.join("cw32-data"), &link).unwrap();
    let error = import_register_candidates(&fixture.root, &fixture.manifest, &link).unwrap_err();
    assert!(error.to_string().contains("curated"), "{error:#}");
    assert_eq!(fs::read(fixture.source_path()).unwrap(), authored);
}

impl Fixture {
    fn field_access(&self, access: &str) {
        let source = fs::read_to_string(self.root.join("fixture.svd")).unwrap();
        let source = source.replace(
            "</bitWidth></field>",
            &format!("</bitWidth><access>{access}</access></field>"),
        );
        fs::write(self.root.join("fixture.svd"), &source).unwrap();
        let mut manifest: Value =
            serde_yaml::from_slice(&fs::read(&self.manifest).unwrap()).unwrap();
        manifest["source"]["sha256"] = format!("{:x}", Sha256::digest(source.as_bytes())).into();
        fs::write(&self.manifest, serde_yaml::to_string(&manifest).unwrap()).unwrap();
    }
}

#[test]
fn raw_import_preserves_field_access_without_emitting_active_restrictions() {
    let fixture = Fixture::new();
    fixture.field_access("read-only");
    let authored = fs::read(fixture.source_path()).unwrap();
    let restrictions = fs::read(fixture.root.join("cw32-data/field-access.yaml")).unwrap();
    let out = fixture.root.join("candidates");
    import_register_candidates(&fixture.root, &fixture.manifest, &out).unwrap();
    let candidate: Value = serde_json::from_slice(
        &fs::read(out.join("field-access-candidates/CW32TEST.json")).unwrap(),
    )
    .unwrap();
    let field = &candidate["fields"][0];
    assert_eq!(field["raw_access"], "read-only");
    assert_eq!(field["effective_access"], "read-only");
    assert_eq!(field["origin"]["path"], "UART1.CTRL.EN");
    assert_eq!(field["projection"]["field"], "EN");
    assert_eq!(candidate["review_only"], true);
    assert!(!out.join("chips").exists());
    assert!(!out.join("field-access").exists());
    assert_eq!(fs::read(fixture.source_path()).unwrap(), authored);
    assert_eq!(
        fs::read(fixture.root.join("cw32-data/field-access.yaml")).unwrap(),
        restrictions
    );
}

#[test]
fn identical_register_shape_with_different_field_access_cannot_share_candidate_map() {
    let fixture = Fixture::new();
    let out = fixture.root.join("candidates");
    import_register_candidates(&fixture.root, &fixture.manifest, &out).unwrap();
    let first = fs::read(out.join("field-access-candidates/maps/uart_v1.json")).unwrap();
    let authored_before = fs::read(fixture.source_path()).unwrap();
    let restrictions_before = fs::read(fixture.root.join("cw32-data/field-access.yaml")).unwrap();
    fixture.write_profile("CW32TEST2", "v1", 0);
    fixture.field_access("read-only");
    let error = import_register_candidates(&fixture.root, &fixture.manifest, &out).unwrap_err();
    assert!(
        error
            .to_string()
            .contains("conflicting profiles attempted to share field-access candidate(s): uart_v1"),
        "{error:#}"
    );
    assert_eq!(
        fs::read(out.join("field-access-candidates/maps/uart_v1.json")).unwrap(),
        first
    );
    assert_eq!(fs::read(fixture.source_path()).unwrap(), authored_before);
    assert_eq!(
        fs::read(fixture.root.join("cw32-data/field-access.yaml")).unwrap(),
        restrictions_before
    );
    assert!(!out.join("chips").exists());
    assert!(!out.join("field-access").exists());
    assert!(out.join("field-access-candidates/CW32TEST2.json").exists());
    let report: Value =
        serde_json::from_slice(&fs::read(out.join("reports/CW32TEST2-field-access.json")).unwrap())
            .unwrap();
    assert_eq!(
        report["incompatible_shared_maps"][0]["register_version"],
        "uart_v1"
    );
    assert_eq!(
        report["incompatible_shared_maps"][0]["differences"][0]["current"]["effective_access"],
        "read-only"
    );
}

#[test]
fn curated_agreement_and_manual_conflict_are_reported_without_reclassification() {
    for (svd_access, expected) in [
        ("read-only", "agrees"),
        ("write-only", "curated-restriction-differs-from-svd"),
    ] {
        let fixture = Fixture::new();
        fixture.field_access(svd_access);
        let out = fixture.root.join("candidates");
        import_register_candidates(&fixture.root, &fixture.manifest, &out).unwrap();
        fs::copy(out.join("registers/uart_v1.yaml"), fixture.source_path()).unwrap();
        fs::write(
            fixture.root.join("cw32-data/field-access.yaml"),
            r#"schema_version: 1
registers:
  uart_v1:
  - block: UART
    register: CTRL
    fieldset: CTRL
    field: EN
    bit_offset: 0
    bit_size: 1
    evidence: [Synthetic reviewed manual fixture]
"#,
        )
        .unwrap();
        import_register_candidates(&fixture.root, &fixture.manifest, &out).unwrap();
        let report: Value = serde_json::from_slice(
            &fs::read(out.join("reports/CW32TEST-field-access.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(report["unexplained_projection_losses"], 0);
        assert_eq!(report["comparisons"][0]["status"], expected);
        assert_eq!(
            report["comparisons"][0]["existing_curated_evidence"][0],
            "Synthetic reviewed manual fixture"
        );
    }
}

#[test]
#[cfg(unix)]
fn access_candidate_nested_redirect_cannot_overwrite_authored_file() {
    let fixture = Fixture::new();
    let out = fixture.root.join("candidates");
    fs::create_dir_all(out.join("field-access-candidates/maps")).unwrap();
    let before = fs::read(fixture.source_path()).unwrap();
    std::os::unix::fs::symlink(
        fixture.source_path(),
        out.join("field-access-candidates/maps/uart_v1.json"),
    )
    .unwrap();
    let error = import_register_candidates(&fixture.root, &fixture.manifest, &out).unwrap_err();
    assert!(
        error
            .to_string()
            .contains("redirect could overwrite curated data"),
        "{error:#}"
    );
    assert_eq!(fs::read(fixture.source_path()).unwrap(), before);
}

#[test]
fn reviewed_width_correction_does_not_become_a_false_access_alias_conflict() {
    let fixture = Fixture::new();
    let mut manifest: Value =
        serde_yaml::from_slice(&fs::read(&fixture.manifest).unwrap()).unwrap();
    manifest["field_width_overrides"] = json!([{"block":"UART","fieldset":"CTRL","field":"EN","expected_bit_offset":0,"expected_bit_size":1,"bit_size":2,"evidence":"Synthetic width correction fixture"}]);
    fs::write(&fixture.manifest, serde_yaml::to_string(&manifest).unwrap()).unwrap();
    let out = fixture.root.join("candidates");
    import_register_candidates(&fixture.root, &fixture.manifest, &out).unwrap();
    fixture.write_profile("CW32TEST2", "v1", 0);
    let source = fs::read_to_string(fixture.root.join("fixture.svd"))
        .unwrap()
        .replace("<bitWidth>1</bitWidth>", "<bitWidth>2</bitWidth>");
    fs::write(fixture.root.join("fixture.svd"), &source).unwrap();
    let mut manifest: Value =
        serde_yaml::from_slice(&fs::read(&fixture.manifest).unwrap()).unwrap();
    manifest["source"]["sha256"] = format!("{:x}", Sha256::digest(source.as_bytes())).into();
    fs::write(&fixture.manifest, serde_yaml::to_string(&manifest).unwrap()).unwrap();
    import_register_candidates(&fixture.root, &fixture.manifest, &out).unwrap();
    let first: Value = serde_json::from_slice(
        &fs::read(out.join("field-access-candidates/CW32TEST.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(first["fields"][0]["bit_size"], 1);
    let map: Value = serde_json::from_slice(
        &fs::read(out.join("field-access-candidates/maps/uart_v1.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(map["signature"][0]["bit_size"], 2);
    let report: Value =
        serde_json::from_slice(&fs::read(out.join("reports/CW32TEST-field-access.json")).unwrap())
            .unwrap();
    assert_eq!(
        report["comparisons"][0]["transformation"]["id"],
        "CW32TEST:field_width_overrides:UART.CTRL.EN"
    );
    assert_eq!(report["comparisons"][0]["source_bit_span"]["bit_size"], 1);
    assert_eq!(
        report["comparisons"][0]["candidate_bit_span"]["bit_size"],
        2
    );
}

#[test]
fn raw_import_rejects_existing_active_metadata_without_touching_it() {
    for active in ["chips", "field-access", "register-writes"] {
        let fixture = Fixture::new();
        let out = fixture.root.join("candidates");
        fs::create_dir_all(out.join(active)).unwrap();
        let old = out.join(active).join("existing.json");
        fs::write(&old, "existing user-selected output").unwrap();
        let error = import_register_candidates(&fixture.root, &fixture.manifest, &out).unwrap_err();
        assert!(
            error
                .to_string()
                .contains("candidate output contains active curated metadata"),
            "{error:#}"
        );
        assert_eq!(
            fs::read_to_string(old).unwrap(),
            "existing user-selected output"
        );
        assert!(!out.join("field-access-candidates").exists());
        assert!(!out.join("reports").exists());
    }
}

#[test]
fn candidate_reuse_requires_intact_importer_provenance() {
    let fixture = Fixture::new();
    let out = fixture.root.join("candidates");
    fs::create_dir_all(&out).unwrap();
    fs::write(out.join("notes.txt"), "user-owned notes").unwrap();
    let error = import_register_candidates(&fixture.root, &fixture.manifest, &out).unwrap_err();
    assert!(
        error.to_string().contains("lacks importer provenance"),
        "{error:#}"
    );
    assert_eq!(
        fs::read_to_string(out.join("notes.txt")).unwrap(),
        "user-owned notes"
    );
    fs::remove_file(out.join("notes.txt")).unwrap();
    import_register_candidates(&fixture.root, &fixture.manifest, &out).unwrap();
    fs::write(out.join("reports/user-notes.json"), "user-owned report").unwrap();
    let error = import_register_candidates(&fixture.root, &fixture.manifest, &out).unwrap_err();
    assert!(
        error.to_string().contains("unknown or modified files"),
        "{error:#}"
    );
    assert_eq!(
        fs::read_to_string(out.join("reports/user-notes.json")).unwrap(),
        "user-owned report"
    );
}
