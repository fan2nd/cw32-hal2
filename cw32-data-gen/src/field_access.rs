//! Validate and project source-reviewed read-only fields in mixed registers.
use anyhow::{Result, ensure};
use chiptool::ir;
use cw32_data_serde::register_write::FieldAccesses;
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::Path,
};

pub(crate) fn emit(root: &Path, output: &Path, version: &str, ir: &ir::IR) -> Result<()> {
    let all: FieldAccesses = crate::read_yaml(root.join("cw32-data/field-access.yaml"))?;
    ensure!(all.schema_version == 1, "unsupported field-access schema");
    let Some(entries) = all.registers.get(version) else {
        return Ok(());
    };
    let mut seen = BTreeSet::new();
    for e in entries {
        ensure!(
            seen.insert((&e.fieldset, &e.field)),
            "duplicate field restriction"
        );
        ensure!(
            !e.evidence.is_empty() && e.evidence.iter().all(|s| !s.trim().is_empty()),
            "field access needs own-source evidence"
        );
        let block = ir
            .blocks
            .get(&e.block)
            .ok_or_else(|| anyhow::anyhow!("unknown field-access block"))?;
        let item = block
            .items
            .iter()
            .find(|i| i.name == e.register)
            .ok_or_else(|| anyhow::anyhow!("unknown mixed register"))?;
        let ir::BlockItemInner::Register(reg) = &item.inner else {
            anyhow::bail!("field access target is not a register")
        };
        ensure!(
            reg.access == ir::Access::ReadWrite && reg.fieldset.as_ref() == Some(&e.fieldset),
            "restriction requires matching mixed RW register"
        );
        let field = ir.fieldsets[&e.fieldset]
            .fields
            .iter()
            .find(|f| f.name == e.field)
            .ok_or_else(|| anyhow::anyhow!("unknown read-only field"))?;
        ensure!(
            field.bit_offset == ir::BitOffset::Regular(e.bit_offset)
                && field.bit_size == e.bit_size
                && field.array.is_none(),
            "read-only field source guard failed"
        );
    }
    let projection = FieldAccesses {
        schema_version: 1,
        registers: BTreeMap::from([(version.to_owned(), entries.clone())]),
    };
    fs::create_dir_all(output.join("field-access"))?;
    super::write_json(
        &output.join("field-access").join(format!("{version}.json")),
        &projection,
    )
}
