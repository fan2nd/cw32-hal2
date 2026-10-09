//! Emit explicit command seeds without changing chiptool's zero Default.
use std::{fmt::Write, fs, path::Path};

use chiptool::{ir, transform};
use cw32_data_serde::register_write::RegisterWrites;

pub(crate) fn render(data_dir: &Path, module: &str, version: &str) -> String {
    let key = format!("{module}_{version}");
    let path = data_dir.join("register-writes").join(format!("{key}.json"));
    if !path.exists() {
        return String::new();
    }
    let writes: RegisterWrites = serde_json::from_slice(&fs::read(path).unwrap()).unwrap();
    assert_eq!(
        writes.schema_version, 1,
        "unsupported register-write schema"
    );
    assert_eq!(
        writes.registers.len(),
        1,
        "write-seed projection has foreign versions"
    );
    let entries = &writes.registers[&key];
    let mut output = String::new();
    for entry in entries {
        // Use the same sanitizer as the main generator, not an invented alias.
        let mut names = ir::IR::new();
        names.fieldsets.insert(
            format!("regs::{}", entry.fieldset),
            ir::FieldSet {
                extends: None,
                description: None,
                bit_size: 32,
                fields: Vec::new(),
            },
        );
        transform::sanitize::Sanitize::default()
            .run(&mut names)
            .unwrap();
        let name = names.fieldsets.keys().next().unwrap();
        writeln!(
            output,
            "impl {name} {{
            /// Source-reviewed hardware reset value; distinct from zero Default.
            #[inline(always)]
            pub const fn reset_value() -> Self {{ Self({}) }}
            /// No-op command preserving all events and documented reserved bits.
            /// Change only the intended command fields before writing; never RMW.
            #[inline(always)]
            pub const fn write_noop() -> Self {{ Self({}) }}
        }}",
            entry.reset_value, entry.write_noop
        )
        .unwrap();
    }
    output
}
