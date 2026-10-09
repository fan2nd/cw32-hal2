//! External comparator input routes, independently qualified by each own manual.
//! Route labels and mux encodings are distinct (notably R031 VC1 CH0..3/mux4..7).
use anyhow::{Context, Result, ensure};
use cw32_data_serde::chip;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{collections::BTreeSet, fs, path::Path};

fn array<'a>(v: &'a Value, key: &str) -> Result<&'a Vec<Value>> {
    v[key]
        .as_array()
        .with_context(|| format!("comparator {key} is missing"))
}
fn string<'a>(v: &'a Value, key: &str) -> Result<&'a str> {
    v[key]
        .as_str()
        .with_context(|| format!("comparator {key} is missing"))
}

pub fn apply(root: &Path, path: &str, profile: &str, core: &mut chip::Core) -> Result<usize> {
    let data = fs::read(root.join(path))?;
    let hash = format!("{:x}", Sha256::digest(&data));
    let expected = PROFILES
        .iter()
        .find(|p| p.0 == profile)
        .context("unreviewed comparator family")?;
    ensure!(
        hash == expected.1,
        "comparator route qualification changed; review own source cells first"
    );
    let input: Value = crate::parse_yaml(Path::new(path), &data)?;
    ensure!(
        input["schema_version"] == 1
            && input["profile"] == profile
            && input["kind"] == "comparator"
            && input["status"] == "verified-own-manual-and-datasheet"
            && input["alias_pin_policy"] == "common-package-intersection",
        "comparator qualification mismatch"
    );
    let lock: Value =
        serde_json::from_slice(&fs::read(root.join("sources/evidence-sources.json"))?)?;
    for key in ["reference_manual", "datasheet"] {
        let source = &input["sources"][key];
        let artifact = array(&lock, "artifacts")?
            .iter()
            .find(|a| a["path"] == source["file"])
            .context("comparator source not in canonical evidence lock")?;
        ensure!(
            artifact["sha256"] == source["sha256"] && artifact["url"] == source["url"],
            "comparator canonical source mismatch"
        );
    }
    let source = &input["sources"]["pinouts"];
    let bytes = fs::read(root.join(string(source, "file")?))?;
    ensure!(
        format!("{:x}", Sha256::digest(&bytes)) == string(source, "sha256")?,
        "comparator package source changed"
    );
    let pinouts: Value = crate::parse_yaml(Path::new(string(source, "file")?), &bytes)?;
    ensure!(
        pinouts["family"] == profile
            && pinouts["source"]["sha256"] == input["sources"]["datasheet"]["sha256"],
        "comparator own datasheet mismatch"
    );
    let pins: BTreeSet<_> = core.pins.iter().map(|p| p.name.clone()).collect();
    let package_sets: Vec<BTreeSet<String>> = array(&pinouts, "packages")?
        .iter()
        .map(|p| {
            p["pins"]
                .as_array()
                .unwrap()
                .iter()
                .flat_map(|p| p["signals"].as_array().unwrap())
                .filter_map(Value::as_str)
                .filter(|s| crate::pinouts::is_gpio(s))
                .map(str::to_owned)
                .collect()
        })
        .collect();
    let first = package_sets
        .first()
        .context("comparator package list is empty")?;
    let common = package_sets
        .iter()
        .skip(1)
        .fold(first.clone(), |a, b| a.intersection(b).cloned().collect());
    ensure!(
        package_sets.contains(&pins) || pins == common,
        "comparator requires exact-package/common-intersection pins"
    );
    let evidence = &input["limits_evidence"];
    let policy = fs::read(root.join(string(evidence, "file")?))?;
    ensure!(
        format!("{:x}", Sha256::digest(&policy)) == string(evidence, "sha256")?
            && evidence["family"] == profile,
        "comparator electrical qualification changed"
    );
    let limits: chip::core::peripheral::ComparatorLimits =
        serde_json::from_value(input["limits"].clone())?;
    ensure!(
        limits.supply_mv.0 < limits.supply_mv.1,
        "invalid comparator supply range"
    );
    for p in core
        .peripherals
        .iter_mut()
        .filter(|p| p.registers.as_ref().is_some_and(|r| r.kind == "vc"))
    {
        p.comparator_limits = Some(limits.clone());
    }
    let mut seen = BTreeSet::new();
    let mut count = 0;
    for route in array(&input, "routes")? {
        let name = string(route, "peripheral")?;
        let pin = string(route, "pin")?;
        let signal = string(route, "signal")?;
        let direction = string(route, "direction")?;
        let mux = route["mux"]
            .as_u64()
            .context("comparator hardware mux missing")?;
        ensure!(
            matches!(direction, "INP" | "INN")
                && signal.starts_with(direction)
                && route["af"].is_null()
                && mux
                    <= input["external_mux_max"][direction]
                        .as_u64()
                        .context("missing external comparator mux bound")?
                && seen.insert((name, pin, direction)),
            "invalid comparator input route"
        );
        let peripheral = core
            .peripherals
            .iter_mut()
            .find(|p| p.name == name)
            .context("comparator absent")?;
        let registers = peripheral
            .registers
            .as_ref()
            .context("comparator register identity missing")?;
        ensure!(
            registers.kind == "vc"
                && registers.block == "VC"
                && registers.version == string(&input, "register_version")?,
            "comparator register identity mismatch"
        );
        if !pins.contains(pin) {
            continue;
        }
        ensure!(
            !peripheral
                .pins
                .iter()
                .any(|p| p.pin == pin && p.signal == signal),
            "comparator route duplicated"
        );
        peripheral.pins.push(chip::core::peripheral::Pin {
            pin: pin.into(),
            signal: signal.into(),
            af: None,
            adc_mux: None,
            comparator_mux: Some(mux as u8),
        });
        count += 1;
    }
    for peripheral in core
        .peripherals
        .iter_mut()
        .filter(|p| p.name.starts_with("VC"))
    {
        peripheral
            .pins
            .sort_by(|a, b| (&a.pin, &a.signal).cmp(&(&b.pin, &b.signal)));
    }
    Ok(count)
}

// Frozen, original route records. Any routing or provenance update requires review.
const PROFILES: &[(&str, &str)] = &[
    (
        "CW32A030",
        "65db15cf1d8d7065e2de3da97180ff4b15753802de1aea87c5fc5400fa0aaabb",
    ),
    (
        "CW32F002",
        "d6574daad32f605d30dde60c62f4676a92aada901e9524ab80099bb3b3134cd0",
    ),
    (
        "CW32F003",
        "bd2a7d5687e032dd3c2bd458a5aa2caf84077d5e84aa0c45d4e67670ae2b4e1b",
    ),
    (
        "CW32F020",
        "17368af764b12282cb56fb4580723a4f3e963a71db7c7c262b8600c5116fe0c9",
    ),
    (
        "CW32F030",
        "e922da8124a732192a3adfbdf354eda50ef23b26aa4536ba19b0f3403325eced",
    ),
    (
        "CW32L010",
        "63d548b6b98eec1192c892f32496998e9731a5d2c796f01debcaaf1d8c52ab34",
    ),
    (
        "CW32L011",
        "2ac5e90a653cf9c574c55154e446116757228b122ff22d6084f0fc1cd92a5eb1",
    ),
    (
        "CW32L012",
        "2fdae0754e237b1a03e4ed8c77b299adb5eebee807d9d98335c6e182157c2883",
    ),
    (
        "CW32L031",
        "578c228e1571fe99f34413adf913ef4907f73ad3dc1f9e7ba9203e9a13f81f40",
    ),
    (
        "CW32L052",
        "c9b4cc5fd1f71dfe64d3ea667006563e6410058afaea771139d840afc31dfc84",
    ),
    (
        "CW32L083",
        "56e821c2460f2a3419a2333a21a86d16042176244b4fb7dbf27fa1ed81aed248",
    ),
    (
        "CW32R031",
        "ffc97643ffca80ae66b166a3517b92f323040a7467ea0ecaf42c775f83673c8e",
    ),
    (
        "CW32W031",
        "1b3acd9117c74e84d6e0a39746c3102d8a2f234d8dabce0c87ca773b19337198",
    ),
];
