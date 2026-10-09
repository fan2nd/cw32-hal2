//! Remove only source-reviewed setters from mixed register fieldsets.
//! This operates on the Rust syntax tree, never on textual register patterns.
use chiptool::{ir, transform};
use cw32_data_serde::register_write::FieldAccesses;
use proc_macro2::TokenStream;
use quote::ToTokens;
use std::{fs, path::Path};

pub(crate) fn apply(data_dir: &Path, key: &str, items: TokenStream) -> TokenStream {
    let path = data_dir.join("field-access").join(format!("{key}.json"));
    if !path.exists() {
        return items;
    }
    let access: FieldAccesses = serde_json::from_slice(&fs::read(path).unwrap()).unwrap();
    assert_eq!(access.schema_version, 1);
    assert_eq!(access.registers.len(), 1);
    let mut syntax: syn::File = syn::parse2(items).unwrap();
    for entry in &access.registers[key] {
        // Reuse chiptool's sanitizer for the exact fieldset and method names.
        let mut names = ir::IR::new();
        names.fieldsets.insert(
            format!("regs::{}", entry.fieldset),
            ir::FieldSet {
                extends: None,
                description: None,
                bit_size: 32,
                fields: vec![ir::Field {
                    name: entry.field.clone(),
                    description: None,
                    bit_offset: ir::BitOffset::Regular(entry.bit_offset),
                    bit_size: entry.bit_size,
                    array: None,
                    enumm: None,
                }],
            },
        );
        transform::sanitize::Sanitize::default()
            .run(&mut names)
            .unwrap();
        let (name, fields) = names.fieldsets.iter().next().unwrap();
        let name = name.rsplit("::").next().unwrap();
        let getter = &fields.fields[0].name;
        let setter = format!("set_{getter}");
        let mut removed = 0;
        let mut reads = 0;
        restrict(
            &mut syntax.items,
            false,
            name,
            getter,
            &setter,
            &mut removed,
            &mut reads,
        );
        assert_eq!(
            removed, 1,
            "read-only setter restriction did not match exactly once"
        );
        assert_eq!(reads, 1, "read-only getter must remain exactly once");
    }
    syntax.into_token_stream()
}

fn restrict(
    items: &mut [syn::Item],
    in_regs: bool,
    fieldset: &str,
    getter: &str,
    setter: &str,
    removed: &mut usize,
    reads: &mut usize,
) {
    for item in items {
        match item {
            syn::Item::Mod(module) => {
                if let Some((_, items)) = &mut module.content {
                    restrict(
                        items,
                        in_regs || module.ident == "regs",
                        fieldset,
                        getter,
                        setter,
                        removed,
                        reads,
                    );
                }
            }
            syn::Item::Impl(implementation) if in_regs && implementation.trait_.is_none() => {
                let syn::Type::Path(path) = &*implementation.self_ty else {
                    continue;
                };
                if path.path.segments.last().unwrap().ident != fieldset {
                    continue;
                }
                implementation.items.retain(|item| {
                    if let syn::ImplItem::Fn(method) = item {
                        if method.sig.ident == getter {
                            *reads += 1;
                        }
                        if method.sig.ident == setter {
                            *removed += 1;
                            return false;
                        }
                    }
                    true
                });
            }
            _ => {}
        }
    }
}
