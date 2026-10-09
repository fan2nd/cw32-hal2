//! Physical package projections from reviewed datasheet tables. Package-less
//! aliases expose the conservative common GPIO set of applicable packages.
use crate::{ChipInput, Input};
use anyhow::{Context, Result, ensure};
use cw32_data_serde::chip::{self, core::Pin};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::Path,
};

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Source {
    source_ref: String,
    source_id: String,
    url: String,
    sha256: String,
    document_filename: String,
    pinout_evidence: String,
    table_pdf_page_indices: Vec<u32>,
    visually_reviewed_pdf_page_indices: Vec<u32>,
    validation: String,
}
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Row {
    positions: BTreeMap<String, Option<String>>,
    source_name: String,
    signals: Vec<String>,
    source_type: String,
    pin_type: String,
    io_structure: String,
    pdf_page_index: u32,
}
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Package {
    name: String,
    key: String,
    family_part: String,
    package: String,
    table_column: String,
    documented_orderable_parts: Vec<String>,
    flash_bytes: u32,
    ram_bytes: u32,
    lead_count: u32,
    additional_numbered_pads: Vec<String>,
    pins: Vec<chip::PackagePin>,
    gpio_pins: Vec<String>,
    gpio_count: usize,
    input_only_pins: Vec<String>,
    debug_pins: Vec<String>,
    gpio_pins_preserving_swd: Vec<String>,
}
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Pinouts {
    schema_version: u32,
    family: String,
    status: String,
    source: Source,
    notes: Vec<String>,
    table_columns: Vec<String>,
    table_rows: Vec<Row>,
    packages: Vec<Package>,
}
#[derive(Debug, Default, Serialize)]
pub struct PinoutReport {
    pub exact_parts: usize,
    pub physical_pin_entries: usize,
    pub alias_policy: String,
}

pub fn is_gpio(signal: &str) -> bool {
    let bytes = signal.as_bytes();
    bytes.len() >= 3
        && bytes[0] == b'P'
        && bytes[1].is_ascii_uppercase()
        && signal[2..].parse::<u8>().is_ok_and(|pin| pin < 16)
        && (bytes.len() == 3 || bytes[2] != b'0')
}
fn gpio_names(pins: &[chip::PackagePin]) -> BTreeSet<String> {
    pins.iter()
        .flat_map(|p| &p.signals)
        .filter(|s| is_gpio(s))
        .cloned()
        .collect()
}
fn unique(values: &[String], context: &str) -> Result<BTreeSet<String>> {
    let set: BTreeSet<_> = values.iter().cloned().collect();
    ensure!(
        set.len() == values.len() && !set.contains(""),
        "duplicate or empty {context}"
    );
    Ok(set)
}

pub fn load(root: &Path, path: &str, input: &Input, chips: &[ChipInput]) -> Result<Pinouts> {
    let file: Pinouts = crate::read_yaml(root.join(path))
        .with_context(|| format!("parse pinout metadata {path}"))?;
    file.validate(root, input, chips)?;
    Ok(file)
}
impl Pinouts {
    fn validate(&self, root: &Path, input: &Input, chips: &[ChipInput]) -> Result<()> {
        ensure!(
            self.schema_version == 1 && self.family == input.line,
            "pinout profile mismatch"
        );
        ensure!(
            self.status == "verified-from-official-datasheet",
            "unreviewed pinout status"
        );
        ensure!(
            input.alias_pin_policy.as_deref() == Some("common-package-intersection"),
            "pinout aliases require explicit common-package-intersection policy"
        );
        let source = &self.source;
        let authority: serde_json::Value =
            serde_json::from_slice(&fs::read(root.join("sources/evidence-sources.json"))?)?;
        let original = authority["artifacts"]
            .as_array()
            .context("missing source authority")?
            .iter()
            .find(|a| a["id"] == source.source_ref)
            .context("unknown pinout source_ref")?;
        ensure!(
            original["sha256"] == source.sha256
                && original["url"] == source.url
                && original["provenance"]["chip_scope"]
                    .as_array()
                    .context("missing pinout source scope")?
                    .iter()
                    .any(|f| f == &self.family),
            "pinout source_ref must match own-family evidence"
        );
        ensure!(
            source.sha256.len() == 64 && source.sha256.bytes().all(|b| b.is_ascii_hexdigit()),
            "invalid pinout source hash"
        );
        ensure!(
            source.url.starts_with("https://www.whxy.com/")
                && !source.document_filename.is_empty()
                && !source.pinout_evidence.is_empty()
                && !source.validation.is_empty()
                && !source.table_pdf_page_indices.is_empty()
                && !source.visually_reviewed_pdf_page_indices.is_empty(),
            "pinout evidence incomplete"
        );
        ensure!(
            !self.notes.is_empty() && self.notes.iter().all(|s| !s.is_empty()),
            "pinout notes missing"
        );
        let catalog_path = input
            .parts_catalog
            .as_ref()
            .context("physical pins require exact-parts catalog")?;
        let catalog: serde_json::Value = crate::read_yaml(root.join(catalog_path))?;
        let catalog_source = &catalog["sources"][&source.source_id];
        ensure!(
            catalog_source["sha256"] == source.sha256 && catalog_source["url"] == source.url,
            "pinout source differs from reviewed parts catalog"
        );
        let columns = unique(&self.table_columns, "pinout column")?;
        let mut names = BTreeSet::new();
        for row in &self.table_rows {
            ensure!(
                row.positions.keys().cloned().collect::<BTreeSet<_>>() == columns,
                "pinout row columns differ"
            );
            unique(&row.signals, "pinout signal")?;
            ensure!(
                !row.signals.is_empty()
                    && !row.source_name.is_empty()
                    && !row.source_type.is_empty()
                    && !row.pin_type.is_empty()
                    && !row.io_structure.is_empty()
                    && source.table_pdf_page_indices.contains(&row.pdf_page_index),
                "pinout row lacks evidence"
            );
        }
        for package in &self.packages {
            ensure!(
                names.insert(package.name.clone()),
                "duplicate exact-part pinout"
            );
            ensure!(
                columns.contains(&package.table_column),
                "unknown package column"
            );
            ensure!(
                !package.key.is_empty()
                    && !package.family_part.is_empty()
                    && package.name.starts_with(&package.family_part)
                    && package.documented_orderable_parts.contains(&package.name),
                "invalid package identity"
            );
            unique(&package.documented_orderable_parts, "orderable part")?;
            let chip = chips
                .iter()
                .find(|c| c.name == package.name)
                .context("pinout exact part missing from catalog")?;
            ensure!(
                chip.packages.len() == 1 && chip.packages[0].package == package.package,
                "pinout package designation differs from catalog"
            );
            let memory: BTreeMap<_, _> = chip
                .memory
                .iter()
                .flatten()
                .map(|m| (m.name.as_str(), m.size))
                .collect();
            ensure!(
                memory.get("FLASH") == Some(&package.flash_bytes)
                    && memory.get("RAM") == Some(&package.ram_bytes),
                "pinout memory differs from catalog"
            );
            let mut expected = BTreeMap::new();
            let mut output_capable = BTreeSet::new();
            let mut input_only = BTreeSet::new();
            let mut debug = BTreeSet::new();
            for row in &self.table_rows {
                if let Some(position) = &row.positions[&package.table_column] {
                    ensure!(
                        expected
                            .insert(position.clone(), row.signals.clone())
                            .is_none(),
                        "duplicate source-table package position"
                    );
                    for signal in row.signals.iter().filter(|s| is_gpio(s)) {
                        match row.pin_type.as_str() {
                            "I/O" => {
                                output_capable.insert(signal.clone());
                            }
                            "I" => {
                                input_only.insert(signal.clone());
                            }
                            _ => anyhow::bail!("unreviewed GPIO pin type {}", row.pin_type),
                        }
                        if row.signals.iter().any(|s| s == "SWDIO" || s == "SWCLK") {
                            debug.insert(signal.clone());
                        }
                    }
                }
            }
            let actual: BTreeMap<_, _> = package
                .pins
                .iter()
                .map(|p| (p.position.clone(), p.signals.clone()))
                .collect();
            ensure!(
                actual.len() == package.pins.len() && actual == expected,
                "physical package pins differ from reviewed table"
            );
            let mut positions: BTreeSet<_> =
                (1..=package.lead_count).map(|p| p.to_string()).collect();
            for pad in &package.additional_numbered_pads {
                ensure!(
                    pad.parse::<u32>().is_ok() && positions.insert(pad.clone()),
                    "invalid additional pad"
                );
            }
            ensure!(
                positions == actual.keys().cloned().collect(),
                "package positions incomplete"
            );
            ensure!(
                unique(&package.gpio_pins, "GPIO pin")? == output_capable
                    && package.gpio_count == output_capable.len(),
                "GPIO capabilities differ from table"
            );
            ensure!(
                unique(&package.input_only_pins, "input-only pin")? == input_only,
                "input-only capabilities differ from table"
            );
            ensure!(
                unique(&package.debug_pins, "debug pin")? == debug,
                "debug pin capabilities differ from table"
            );
            ensure!(
                unique(&package.gpio_pins_preserving_swd, "debug-preserving pin")?
                    == output_capable.difference(&debug).cloned().collect(),
                "debug-preserving pins differ"
            );
        }
        let exact: BTreeSet<_> = chips
            .iter()
            .filter(|c| !c.packages.is_empty())
            .map(|c| c.name.clone())
            .collect();
        ensure!(
            names == exact,
            "pinout coverage differs from exact-part catalog"
        );
        Ok(())
    }

    /// Family pad capabilities stay separate from package pins and interrupt domains.
    pub(crate) fn project_gpio(&self, root: &Path, core: &mut chip::Core) -> Result<()> {
        #[derive(Deserialize)]
        struct Review {
            families: Vec<Family>,
        }
        #[derive(Deserialize)]
        struct Family {
            family: String,
            ports: Vec<Port>,
        }
        #[derive(Deserialize)]
        struct Port {
            name: String,
            family_pin_mask: String,
            pull_down_mask: String,
        }
        let review: Review =
            serde_json::from_slice(&fs::read(root.join("docs/gpio-shared-inventory.json"))?)?;
        let own = review
            .families
            .iter()
            .find(|f| f.family == self.family)
            .context("missing GPIO own-source capability review")?;
        let mut masks = BTreeMap::<String, u16>::new();
        for pin in self.packages.iter().flat_map(|p| &p.gpio_pins) {
            ensure!(
                is_gpio(pin) && pin.as_bytes()[1] <= b'F',
                "invalid family GPIO pad"
            );
            let index: u8 = pin[2..].parse()?;
            *masks.entry(format!("GPIO{}", &pin[1..2])).or_default() |= 1 << index;
        }
        for peripheral in core
            .peripherals
            .iter_mut()
            .filter(|p| p.name.starts_with("GPIO"))
        {
            let output_mask = masks.remove(&peripheral.name).unwrap_or(0);
            // Own-manual/SDK qualification narrows overbroad vendor PDR declarations.
            let pull_down_mask = match self.family.as_str() {
                "CW32L010" | "CW32L011" => 0,
                "CW32L012" if peripheral.name == "GPIOF" => 1 << 3,
                "CW32L012" => 0,
                _ => output_mask,
            };
            let port = own
                .ports
                .iter()
                .find(|p| p.name == peripheral.name)
                .context("GPIO capability review omits bank")?;
            ensure!(
                u16::from_str_radix(port.family_pin_mask.trim_start_matches("0x"), 16)?
                    == output_mask
                    && u16::from_str_radix(port.pull_down_mask.trim_start_matches("0x"), 16)?
                        == pull_down_mask,
                "GPIO pad/pull capabilities disagree with own-source review"
            );
            ensure!(
                pull_down_mask & !output_mask == 0,
                "GPIO pull-down without output-capable pad"
            );
            peripheral.gpio = Some(chip::core::peripheral::Gpio {
                output_mask,
                pull_down_mask,
            });
        }
        ensure!(masks.is_empty(), "family pad references absent GPIO bank");
        Ok(())
    }

    pub fn project(&self, chip: &mut ChipInput, core: &mut chip::Core) -> Result<()> {
        let names = if let Some(package) = self.packages.iter().find(|p| p.name == chip.name) {
            ensure!(
                chip.packages.len() == 1,
                "exact pinout requires one package"
            );
            chip.packages[0].pins = package.pins.clone();
            gpio_names(&package.pins)
        } else {
            ensure!(
                chip.packages.is_empty(),
                "no reviewed physical package pins"
            );
            let applicable: Vec<_> = self
                .packages
                .iter()
                .filter(|p| chip.name == self.family || chip.name == p.family_part)
                .collect();
            ensure!(
                !applicable.is_empty(),
                "alias lacks applicable reviewed packages"
            );
            let mut common = gpio_names(&applicable[0].pins);
            for package in &applicable[1..] {
                common = common
                    .intersection(&gpio_names(&package.pins))
                    .cloned()
                    .collect();
            }
            common
        };
        for name in &names {
            ensure!(
                core.peripherals
                    .iter()
                    .any(|p| p.name == format!("GPIO{}", &name[1..2])),
                "pin references absent GPIO peripheral {name}"
            );
        }
        // Oscillator pads are analog package aliases, not digital alternate functions.
        // Require the same pad in every applicable package for package-less aliases.
        if let Some(sysctrl) = core.peripherals.iter_mut().find(|p| {
            p.clock_limits
                .as_ref()
                .is_some_and(|c| c.hse.is_some() || c.lse.is_some())
        }) {
            let exact = self.packages.iter().find(|p| p.name == chip.name);
            let applicable: Vec<_> = self
                .packages
                .iter()
                .filter(|p| {
                    exact.map_or(
                        chip.name == self.family || chip.name == p.family_part,
                        |selected| p.name == selected.name,
                    )
                })
                .collect();
            ensure!(
                !applicable.is_empty(),
                "oscillator pin projection lacks package evidence"
            );
            let limits = sysctrl.clock_limits.as_ref().unwrap();
            let mut signals = Vec::new();
            if limits.hse.is_some() {
                signals.extend([("OSC_IN", "HSE_IN"), ("OSC_OUT", "HSE_OUT")]);
            }
            if limits.lse.is_some() {
                signals.extend([("OSC32_IN", "LSE_IN"), ("OSC32_OUT", "LSE_OUT")]);
            }
            for (alias, signal) in signals {
                let mut selected = None;
                let mut present = true;
                for package in &applicable {
                    let pads: Vec<_> = package
                        .pins
                        .iter()
                        .filter(|p| p.signals.iter().any(|s| s == alias))
                        .collect();
                    ensure!(pads.len() <= 1, "ambiguous oscillator package pad");
                    let Some(pad) = pads.first() else {
                        present = false;
                        continue;
                    };
                    let gpios: Vec<_> = pad.signals.iter().filter(|s| is_gpio(s)).collect();
                    ensure!(
                        gpios.len() == 1,
                        "oscillator pad lacks unique GPIO identity"
                    );
                    if let Some(prior) = selected {
                        ensure!(
                            prior == gpios[0],
                            "oscillator pad differs across alias packages"
                        );
                    }
                    selected = Some(gpios[0]);
                }
                sysctrl.pins.retain(|p| p.signal != signal);
                if let Some(pin) = selected.filter(|_| present) {
                    ensure!(
                        names.contains(pin),
                        "oscillator pad absent from package GPIO projection"
                    );
                    sysctrl.pins.push(chip::core::peripheral::Pin {
                        pin: pin.clone(),
                        signal: signal.into(),
                        af: None,
                        adc_mux: None,
                        comparator_mux: None,
                    });
                }
            }
        }
        // HEX routes originate in the own-source electrical policy. Only retain
        // pads bonded on this exact package or every applicable alias package.
        for peripheral in &mut core.peripherals {
            if peripheral
                .clock_limits
                .as_ref()
                .is_some_and(|c| c.hex.is_some())
            {
                peripheral.pins.retain(|p| {
                    !matches!(p.signal.as_str(), "HEX_PB00" | "HEX_PB01") || names.contains(&p.pin)
                });
            }
        }
        core.pins = names.into_iter().map(|name| Pin { name }).collect();
        Ok(())
    }
    pub fn report(&self) -> PinoutReport {
        PinoutReport {
            exact_parts: self.packages.len(),
            physical_pin_entries: self.packages.iter().map(|p| p.pins.len()).sum(),
            alias_policy: "common-package-intersection: GPIO labels common to every reviewed package for the family or matching family_part; no physical package is assigned to an alias".into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> (
        std::path::PathBuf,
        Input,
        Vec<ChipInput>,
        Pinouts,
        chip::Core,
    ) {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .to_path_buf();
        let input: Input = crate::read_yaml(root.join("cw32-data/inputs/cw32f030.yaml")).unwrap();
        let chips = crate::expand_chip_inputs(&root, &input).unwrap();
        let file = load(
            &root,
            input.pinout_metadata.as_deref().unwrap(),
            &input,
            &chips,
        )
        .unwrap();
        let chip: cw32_data_serde::Chip = serde_json::from_slice(
            &fs::read(root.join("cw32-data/data/chips/CW32F030.json")).unwrap(),
        )
        .unwrap();
        (root, input, chips, file, chip.cores[0].clone())
    }
    #[test]
    fn exact_package_and_alias_scopes_are_distinct() {
        let (_, _, mut chips, file, core) = fixture();
        for (name, pb8, physical) in [
            ("CW32F030K8T7", false, true),
            ("CW32F030K8U7", true, true),
            ("CW32F030K8", false, false),
        ] {
            let chip = chips.iter_mut().find(|c| c.name == name).unwrap();
            let mut core = core.clone();
            file.project(chip, &mut core).unwrap();
            assert_eq!(core.pins.iter().any(|p| p.name == "PB8"), pb8);
            assert_eq!(!chip.packages.is_empty(), physical);
            assert!(
                core.pins.iter().any(|p| p.name == "PF3"),
                "physical input-only GPIO label remains represented"
            );
        }
    }
    #[test]
    fn physical_position_source_and_capability_mutations_are_rejected() {
        let (root, input, chips, mut file, _) = fixture();
        file.packages[0].pins[0].position = "999".into();
        assert!(file.validate(&root, &input, &chips).is_err());
        let (_, _, _, mut file, _) = fixture();
        file.source.sha256 = "0".repeat(64);
        assert!(file.validate(&root, &input, &chips).is_err());
        let (_, _, _, mut file, _) = fixture();
        file.packages[0].gpio_pins.push("PF3".into());
        assert!(file.validate(&root, &input, &chips).is_err());
    }
    #[test]
    fn alias_scope_requires_explicit_policy() {
        let (root, mut input, chips, file, _) = fixture();
        input.alias_pin_policy = None;
        assert!(file.validate(&root, &input, &chips).is_err());
    }
}
