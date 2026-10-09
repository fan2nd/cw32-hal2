//! Own-manual-qualified classic B routes and buffered N/BK1 routes.
use super::*;
use sha2::{Digest, Sha256};
pub fn apply(
    root: &Path,
    path: &str,
    profile: &str,
    core: &mut chip::Core,
    registers: &BTreeMap<String, ir::IR>,
) -> Result<usize> {
    let (hash, version) = match profile {
        "CW32F030" => (
            "53975317fa86120b1fa91ecdd6b8e6579015589eb3b49d8e64f37204a0fdfe2d",
            "v1",
        ),
        "CW32A030" => (
            "e47fde505030895d6c5579a1acab1028713abb2fd3e25ac0c1066d8e7228381d",
            "v1",
        ),
        "CW32L010" => (
            "5550c07e2bb8920a8bbf799d68b645ddfd84757957dac8bb2a8c2f583796d957",
            "cw32l010_v1",
        ),
        "CW32L011" => (
            "954aab54e300f1281dd0807f5643766615d2edb22bc5d02a293418391ddf0189",
            "cw32l010_v1",
        ),
        "CW32L012" => (
            "71aae6f9162db4b6830b3c4205cfdbcf2ddc4f7ea31a458a96f6c75c50e0f780",
            "cw32l012_v1",
        ),
        _ => anyhow::bail!("unqualified complementary ATIM family"),
    };
    ensure!(
        path == format!(
            "cw32-data/af/{}-atim-complementary.yaml",
            profile.to_lowercase()
        ),
        "complementary route path changed"
    );
    let bytes = fs::read(root.join(path))?;
    ensure!(
        format!("{:x}", Sha256::digest(&bytes)) == hash,
        "complementary source qualification changed"
    );
    let value: serde_json::Value = crate::parse_yaml(Path::new(path), &bytes)?;
    for key in ["pinouts", "sdk_candidates"] {
        let source = &value["sources"][key];
        let bytes = fs::read(
            root.join(
                source["file"]
                    .as_str()
                    .context("missing complementary source")?,
            ),
        )?;
        ensure!(
            format!("{:x}", Sha256::digest(&bytes)) == source["sha256"].as_str().unwrap(),
            "complementary source changed"
        );
    }
    let capability: chip::core::peripheral::AtimComplementary =
        serde_json::from_value(value["capability"].clone())?;
    let expected = if matches!(profile, "CW32F030" | "CW32A030") {
        (3, 1010, 0)
    } else {
        (4, 1008, 1)
    };
    ensure!(
        (
            capability.channels,
            capability.dead_time_max_ticks,
            capability.break_inputs
        ) == expected,
        "unsupported complementary capability"
    );
    let timer = core
        .peripherals
        .iter_mut()
        .find(|p| p.name == "ATIM")
        .context("ATIM absent")?;
    let reference = timer.registers.as_ref().context("ATIM registers absent")?;
    ensure!(
        reference.kind == "atim" && reference.version == version,
        "complementary ATIM variant changed"
    );
    if matches!(profile, "CW32F030" | "CW32A030") {
        // Replace SDK candidates with the independently qualified B routes.
        // Existing BK routing facts do not create a classic brake capability.
        timer
            .pins
            .retain(|pin| !matches!(pin.signal.as_str(), "CH1B" | "CH2B" | "CH3B"));
    }
    timer.atim_complementary = Some(capability);
    super::project(&serde_json::from_value(value)?, profile, core, registers)
}
