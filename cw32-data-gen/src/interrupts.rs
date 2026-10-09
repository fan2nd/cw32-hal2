//! Narrow, evidence-backed corrections to vendor peripheral IRQ ownership.
use anyhow::{Result, ensure};
use cw32_data_serde::chip;
use serde::Deserialize;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Deserialize)]
pub struct AssociationOverride {
    pub interrupt: String,
    pub number: u32,
    pub expected_owners: Vec<String>,
    pub owners: Vec<String>,
    pub evidence: String,
}

pub fn apply(
    peripherals: &mut [chip::core::Peripheral],
    vectors: &BTreeMap<String, u32>,
    corrections: &[AssociationOverride],
) -> Result<()> {
    let mut seen = BTreeSet::new();
    for c in corrections {
        ensure!(
            !c.evidence.trim().is_empty(),
            "IRQ ownership correction requires evidence"
        );
        ensure!(
            seen.insert(&c.interrupt),
            "duplicate IRQ ownership correction"
        );
        ensure!(
            vectors.get(&c.interrupt) == Some(&c.number),
            "IRQ correction vector/number does not match source"
        );
        let actual: BTreeSet<_> = peripherals
            .iter()
            .filter(|p| p.interrupts.iter().any(|i| i.interrupt == c.interrupt))
            .map(|p| p.name.clone())
            .collect();
        let expected: BTreeSet<_> = c.expected_owners.iter().cloned().collect();
        ensure!(
            expected.len() == c.expected_owners.len() && actual == expected,
            "IRQ correction source owners differ from reviewed expectation"
        );
        let owners: BTreeSet<_> = c.owners.iter().cloned().collect();
        ensure!(
            !owners.is_empty() && owners.len() == c.owners.len(),
            "IRQ correction has empty or duplicate owners"
        );
        ensure!(
            owners
                .iter()
                .all(|n| peripherals.iter().any(|p| &p.name == n)),
            "IRQ correction invents a peripheral owner"
        );
        for p in peripherals.iter_mut() {
            p.interrupts.retain(|i| i.interrupt != c.interrupt);
            if owners.contains(&p.name) {
                p.interrupts.push(chip::core::peripheral::Interrupt {
                    signal: "GLOBAL".into(),
                    interrupt: c.interrupt.clone(),
                });
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn peripheral(name: &str, owner: bool) -> chip::core::Peripheral {
        chip::core::Peripheral {
            name: name.into(),
            address: 0,
            registers: None,
            rcc: None,
            rcc_control: None,
            spi: None,
            classic_timer_input: None,
            atim_complementary: None,
            gpio_interrupt: None,
            gpio: None,
            cordic: None,
            aes: None,
            trng: None,
            ram_parity: None,
            clock_limits: None,
            adc_limits: None,
            comparator_limits: None,
            reference_divider: None,
            dac_limits: None,
            opa_limits: None,
            lcd: None,
            rtc_calendar: None,
            rtc_alarms: None,
            lvd: None,
            ir: None,
            iwdt_clock: None,
            i2c_limits: None,
            flash_limits: None,

            pins: vec![],
            interrupts: if owner {
                vec![chip::core::peripheral::Interrupt {
                    signal: "GLOBAL".into(),
                    interrupt: "FAULT".into(),
                }]
            } else {
                vec![]
            },
            dma_channels: vec![],
            triggers: vec![],
            afio: None,
        }
    }
    fn correction() -> AssociationOverride {
        AssociationOverride {
            interrupt: "FAULT".into(),
            number: 31,
            expected_owners: vec!["CRC".into()],
            owners: vec!["SYSCTRL".into()],
            evidence: "Reviewed manual".into(),
        }
    }
    #[test]
    fn reassigns_only_reviewed_source_vector() {
        let mut p = vec![peripheral("CRC", true), peripheral("SYSCTRL", false)];
        apply(
            &mut p,
            &BTreeMap::from([("FAULT".into(), 31)]),
            &[correction()],
        )
        .unwrap();
        assert!(p[0].interrupts.is_empty());
        assert_eq!(p[1].interrupts[0].interrupt, "FAULT");
    }
    #[test]
    fn rejects_changed_source_owner_number_and_fabricated_target() {
        for mode in 0..4 {
            let mut p = vec![peripheral("CRC", true), peripheral("SYSCTRL", false)];
            let mut c = correction();
            match mode {
                0 => c.expected_owners = vec!["SPI1".into()],
                1 => c.number = 30,
                2 => c.owners = vec!["MADEUP".into()],
                _ => c.evidence.clear(),
            }
            assert!(apply(&mut p, &BTreeMap::from([("FAULT".into(), 31)]), &[c]).is_err());
            assert_eq!(p[0].interrupts.len(), 1);
            assert!(p[1].interrupts.is_empty());
        }
    }
}
