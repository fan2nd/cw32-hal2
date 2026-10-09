//! Validate and project reviewed command-write seeds beside generated IR.
use std::{collections::BTreeMap, fs, path::Path};

use anyhow::{Result, ensure};
use chiptool::ir;
use cw32_data_serde::register_write::RegisterWrites;

pub(crate) fn emit(root: &Path, output: &Path, version: &str, ir: &ir::IR) -> Result<()> {
    let path = root.join("cw32-data/register-writes.yaml");
    if !path.exists() {
        return Ok(());
    }
    let writes: RegisterWrites = crate::read_yaml(path)?;
    ensure!(
        writes.schema_version == 1,
        "unsupported register-write schema"
    );
    let Some(entries) = writes.registers.get(version) else {
        return Ok(());
    };
    let mut used = std::collections::BTreeSet::new();
    for entry in entries {
        ensure!(
            used.insert(&entry.fieldset),
            "duplicate write-seed fieldset"
        );
        ensure!(
            !entry.evidence.is_empty() && entry.evidence.iter().all(|s| !s.trim().is_empty()),
            "write seeds require evidence"
        );
        let block = ir
            .blocks
            .get(&entry.block)
            .ok_or_else(|| anyhow::anyhow!("unknown write-seed block"))?;
        let item = block
            .items
            .iter()
            .find(|i| i.name == entry.register)
            .ok_or_else(|| anyhow::anyhow!("unknown write-seed register"))?;
        let ir::BlockItemInner::Register(register) = &item.inner else {
            anyhow::bail!("write-seed target must be register");
        };
        ensure!(
            register.access != ir::Access::Read
                && register.fieldset.as_ref() == Some(&entry.fieldset),
            "write-seed register must have writable matching fieldset"
        );
        let fieldset = &ir.fieldsets[&entry.fieldset];
        ensure!(
            register.bit_size == fieldset.bit_size && (1..=64).contains(&fieldset.bit_size),
            "write-seed width mismatch"
        );
        let maximum = u64::MAX >> (64 - fieldset.bit_size);
        ensure!(
            (entry.reset_value | entry.write_noop) & !maximum == 0,
            "write seeds exceed register width"
        );
        let mut fields = std::collections::BTreeSet::new();
        ensure!(
            !entry.zero_to_clear_fields.is_empty(),
            "empty command domain"
        );
        for name in &entry.zero_to_clear_fields {
            ensure!(fields.insert(name), "duplicate clear field");
            let field = fieldset
                .fields
                .iter()
                .find(|f| &f.name == name)
                .ok_or_else(|| anyhow::anyhow!("unknown command field {name}"))?;
            let ir::BitOffset::Regular(offset) = field.bit_offset else {
                anyhow::bail!("command field must have regular offset");
            };
            ensure!(
                field.bit_size == 1 && field.array.is_none(),
                "command field must be a scalar bit"
            );
            ensure!(
                entry.write_noop & (1 << offset) != 0,
                "R1W0 no-op must leave command field {name} set"
            );
        }
    }
    let projected = RegisterWrites {
        schema_version: 1,
        registers: BTreeMap::from([(version.to_owned(), entries.clone())]),
    };
    fs::create_dir_all(output.join("register-writes"))?;
    super::write_json(
        &output
            .join("register-writes")
            .join(format!("{version}.json")),
        &projected,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    fn validate(change: impl FnOnce(&mut RegisterWrites)) -> Result<()> {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let root = std::env::temp_dir().join(format!(
            "cw32-register-writes-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(root.join("cw32-data"))?;
        let mut writes: RegisterWrites =
            serde_yaml::from_str(include_str!("../../cw32-data/register-writes.yaml"))?;
        change(&mut writes);
        fs::write(
            root.join("cw32-data/register-writes.yaml"),
            serde_yaml::to_string(&writes)?,
        )?;
        let ir: ir::IR = serde_yaml::from_str(include_str!(
            "../../cw32-data/registers/uart_cw32l031_v1.yaml"
        ))?;
        let result = emit(&root, &root, "uart_cw32l031_v1", &ir);
        fs::remove_dir_all(root)?;
        result
    }

    #[test]
    fn valid_command_projection() {
        validate(|_| {}).unwrap();
    }

    #[test]
    fn rejects_unknown_clear_field() {
        assert!(
            validate(
                |data| data.registers.get_mut("uart_cw32l031_v1").unwrap()[0]
                    .zero_to_clear_fields
                    .push("TXBUSY".into())
            )
            .is_err()
        );
    }

    #[test]
    fn rejects_zero_seed_for_r1w0() {
        assert!(
            validate(|data| data.registers.get_mut("uart_cw32l031_v1").unwrap()[0].write_noop = 0)
                .is_err()
        );
    }

    #[test]
    fn rejects_seed_outside_transaction_width() {
        assert!(
            validate(
                |data| data.registers.get_mut("uart_cw32l031_v1").unwrap()[0].write_noop |= 1 << 32
            )
            .is_err()
        );
    }

    #[test]
    fn rejects_missing_evidence() {
        assert!(
            validate(
                |data| data.registers.get_mut("uart_cw32l031_v1").unwrap()[0]
                    .evidence
                    .clear()
            )
            .is_err()
        );
    }
}
