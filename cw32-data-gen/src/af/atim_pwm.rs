//! Qualified ATIM main-output sidecars. Hash changes require own-source requalification.
use super::*;
use sha2::{Digest, Sha256};
pub(super) fn apply(
    root: &Path,
    path: &str,
    profile: &str,
    core: &mut chip::Core,
    registers: &BTreeMap<String, ir::IR>,
) -> Result<usize> {
    let (hash, version) = match profile {
        "CW32F030" => (
            "ce948af8a55f8bd08a466fe90c16cd43364356c5e91266c3318a19196508ea33",
            "v1",
        ),
        "CW32A030" => (
            "e974f5f8bb1da8ffe15ea08dbf7a3c54bb46beee5c5d2e88efff9d3ebd8af370",
            "v1",
        ),
        "CW32F003" => (
            "cb30dfcac03774ffe596d0d80cc59766aa956697379a81588cc9339de66608d7",
            "cw32f003_v1",
        ),
        "CW32L031" => (
            "9816a9eb8242d4ef1e9f6e276fe29625140392f366f5c2fb08562c097bd720f8",
            "v1",
        ),
        "CW32R031" => (
            "e3eff12324ea9ad86529e1ae28e5ad8dce3259c2270940a404a88d14c2b7e7d0",
            "v1",
        ),
        "CW32W031" => (
            "d29421a04daffc62da45b09fbb7dfebe2d2d403ab95480601095d269699135ab",
            "v1",
        ),
        "CW32L052" => (
            "2be044befb9bbb91616eadaee57eda262a63b5e4278125a25ec6cd05da73b8f6",
            "cw32l052_v1",
        ),
        "CW32L083" => (
            "3408fe420ede96504d718d4b9303b46dc994702777e02a2a0c92ad55d2e48962",
            "v1",
        ),
        "CW32L010" => (
            "b9d3a0622dc4a278cba8495b3f696817cc8ad50ef512298432b8ded329ff39e4",
            "cw32l010_v1",
        ),
        "CW32L011" => (
            "9248a1284f2a42bdbe61d4f4ad4c5002f0ca2ac3bf13a16945151d9af6428e77",
            "cw32l010_v1",
        ),
        "CW32L012" => (
            "ce7f3e506821e3a58ca1d31dcc9bbb316ceb320f60dccb8af3a41ac01bdec3bf",
            "cw32l012_v1",
        ),
        _ => anyhow::bail!("unqualified ATIM family"),
    };
    ensure!(
        path == format!("cw32-data/af/{}-atim-pwm.yaml", profile.to_lowercase()),
        "ATIM sidecar path changed"
    );
    let bytes = fs::read(root.join(path))?;
    ensure!(
        format!("{:x}", Sha256::digest(&bytes)) == hash,
        "ATIM source-qualified sidecar changed"
    );
    let value: serde_json::Value = crate::parse_yaml(Path::new(path), &bytes)?;
    for key in ["pinouts", "sdk_candidates"] {
        let source = &value["sources"][key];
        let bytes =
            fs::read(root.join(source["file"].as_str().context("ATIM input path absent")?))?;
        ensure!(
            format!("{:x}", Sha256::digest(&bytes)) == source["sha256"].as_str().unwrap(),
            "ATIM package/SDK source changed"
        );
    }
    let pins = &value["sources"]["pinouts"];
    let pinouts: serde_json::Value = crate::read_yaml(root.join(pins["file"].as_str().unwrap()))?;
    let package_sets: Vec<BTreeSet<&str>> = pinouts["packages"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| {
            p["pins"]
                .as_array()
                .unwrap()
                .iter()
                .flat_map(|p| p["signals"].as_array().unwrap())
                .filter_map(|s| s.as_str())
                .filter(|s| crate::pinouts::is_gpio(s))
                .collect()
        })
        .collect();
    let common = package_sets
        .iter()
        .skip(1)
        .fold(package_sets[0].clone(), |a, b| {
            a.intersection(b).copied().collect()
        });
    let actual: BTreeSet<&str> = core.pins.iter().map(|p| p.name.as_str()).collect();
    ensure!(
        actual == common || package_sets.contains(&actual),
        "ATIM exact-package pin set differs"
    );
    let mut projected = core.clone();
    let timer = projected
        .peripherals
        .iter_mut()
        .find(|p| p.name == "ATIM")
        .context("ATIM absent")?;
    let reference = timer.registers.as_ref().context("ATIM registers absent")?;
    ensure!(
        reference.kind == "atim" && reference.block == "ATIM" && reference.version == version,
        "ATIM register variant changed"
    );
    // F/A030 already carry legacy qualified ATIM routes. Replace only main routes;
    // do not erase independently sourced B/input metadata or expose it in the HAL.
    timer.pins.retain(|p| {
        !matches!(
            p.signal.as_str(),
            "CH1A" | "CH2A" | "CH3A" | "CH1" | "CH2" | "CH3" | "CH4"
        )
    });
    let count = super::project(
        &serde_json::from_value(value)?,
        profile,
        &mut projected,
        registers,
    )?;
    *core = projected;
    Ok(count)
}
