//! Source-backed register access and command semantics absent from chiptool IR.
use std::collections::BTreeMap;

/// Register-version keyed write seeds, kept separate from zero-valued Default.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RegisterWrites {
    pub schema_version: u32,
    pub registers: BTreeMap<String, Vec<RegisterWrite>>,
}

/// One reviewed command register. Reserved bits are carried by the two seeds.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RegisterWrite {
    pub block: String,
    pub register: String,
    pub fieldset: String,
    pub reset_value: u64,
    pub write_noop: u64,
    pub zero_to_clear_fields: Vec<String>,
    pub evidence: Vec<String>,
}

/// Register-version keyed read-only fields in otherwise writable registers.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FieldAccesses {
    pub schema_version: u32,
    pub registers: BTreeMap<String, Vec<ReadOnlyField>>,
}

/// A reviewed field whose generated getter remains but whose setter is absent.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReadOnlyField {
    pub block: String,
    pub register: String,
    pub fieldset: String,
    pub field: String,
    pub bit_offset: u32,
    pub bit_size: u32,
    pub evidence: Vec<String>,
}
