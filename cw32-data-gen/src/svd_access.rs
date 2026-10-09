//! Review-only access capture before chiptool drops per-field access.
//!
//! The pinned SVD parser owns parsing and derived-value merging. This module
//! tracks only declaration provenance, then checks the complete field projection.
use anyhow::{Context, Result, bail, ensure};
use chiptool::{ir, svd2ir::replace_suffix};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::Path,
};
use svd_parser::{
    expand::{self, BlockPath},
    svd::{self, DeriveFrom},
};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct Origin {
    level: String,
    path: String,
    access: String,
}

#[derive(Clone)]
struct Declaration {
    level: &'static str,
    access: Option<svd::Access>,
    derived: Option<String>,
}

#[derive(Default)]
struct Origins(BTreeMap<String, Declaration>, BTreeSet<String>);
impl Origins {
    fn add(
        &mut self,
        path: &str,
        level: &'static str,
        access: Option<svd::Access>,
        derived: &Option<String>,
    ) -> Result<()> {
        ensure!(
            self.0
                .insert(
                    path.into(),
                    Declaration {
                        level,
                        access,
                        derived: derived.clone()
                    }
                )
                .is_none(),
            "duplicate SVD declaration {path}"
        );
        Ok(())
    }
    fn registers(&mut self, path: &str, children: &[svd::RegisterCluster]) -> Result<()> {
        for child in children {
            match child {
                svd::RegisterCluster::Cluster(c) => {
                    let p = format!("{path}.{}", c.name);
                    self.add(
                        &p,
                        "cluster",
                        c.default_register_properties.access,
                        &c.derived_from,
                    )?;
                    if !c.children.is_empty() {
                        self.1.insert(p.clone());
                    }
                    self.registers(&p, &c.children)?;
                }
                svd::RegisterCluster::Register(r) => {
                    let p = format!("{path}.{}", r.name);
                    self.add(&p, "register", r.properties.access, &r.derived_from)?;
                    if r.fields.is_some() {
                        self.1.insert(p.clone());
                    }
                    for f in r.fields() {
                        self.add(
                            &format!("{p}.{}", f.name),
                            "field",
                            f.access,
                            &f.derived_from,
                        )?;
                    }
                }
            }
        }
        Ok(())
    }
    // Track the first actual child collection in a derived chain. The pinned
    // value-merging helpers can lose this owner when an intermediate derived
    // node supplies its own children; access provenance must not follow that
    // returned path to a nonexistent declaration.
    fn children_owner(&self, path: &str) -> Result<String> {
        let mut current = path.to_owned();
        let mut seen = BTreeSet::new();
        loop {
            ensure!(
                seen.insert(current.clone()),
                "cyclic SVD child ownership at {current}"
            );
            let d = self.0.get(&current).context("missing SVD child owner")?;
            if self.1.contains(&current) || d.derived.is_none() {
                return Ok(current);
            }
            let base = d.derived.as_ref().unwrap();
            current = if base.contains('.') || d.level == "peripheral" {
                base.clone()
            } else {
                format!(
                    "{}.{}",
                    current
                        .rsplit_once('.')
                        .context("derived node has no parent")?
                        .0,
                    base
                )
            };
        }
    }

    fn origin(&self, path: &str) -> Result<Option<Origin>> {
        self.follow(path, &mut BTreeSet::new())
    }
    fn follow(&self, path: &str, seen: &mut BTreeSet<String>) -> Result<Option<Origin>> {
        ensure!(seen.insert(path.into()), "cyclic SVD derivedFrom at {path}");
        let d = self.0.get(path).with_context(|| format!("unresolved SVD declaration {path}; derivedFrom references to expanded array elements are unsupported"))?;
        // Validate the entire chain even when a local access overrides its base.
        let inherited = if let Some(base) = &d.derived {
            let target = if base.contains('.') || d.level == "peripheral" {
                base.clone()
            } else {
                format!(
                    "{}.{}",
                    path.rsplit_once('.')
                        .context("derived node has no parent")?
                        .0,
                    base
                )
            };
            let b = self.0.get(&target).with_context(|| format!("unresolved {} derivedFrom {path} -> {target}; expanded array-element references are unsupported", d.level))?;
            ensure!(
                b.level == d.level,
                "derivedFrom kind mismatch {path} -> {target}"
            );
            self.follow(&target, seen)?
        } else {
            None
        };
        Ok(d.access
            .map(|a| Origin {
                level: d.level.into(),
                path: path.into(),
                access: a.as_str().into(),
            })
            .or(inherited))
    }
}

fn block_path(path: &str) -> BlockPath {
    let mut components = path.split('.');
    BlockPath {
        peripheral: components.next().unwrap().into(),
        path: components.map(str::to_owned).collect(),
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct Dimension {
    path: String,
    len: u32,
    stride: u32,
    indices: Vec<String>,
    element_names: Vec<String>,
    dim_name: Option<String>,
}
fn dimension<T: svd::Name>(node: &svd::MaybeArray<T>, path: &str) -> Result<Option<Dimension>> {
    let svd::MaybeArray::Array(info, d) = node else {
        return Ok(None);
    };
    ensure!(d.dim > 0, "empty SVD array {path}");
    let indices = d
        .dim_index
        .clone()
        .unwrap_or_else(|| (0..d.dim).map(|i| i.to_string()).collect());
    ensure!(
        indices.len() == d.dim as usize
            && indices.iter().collect::<BTreeSet<_>>().len() == indices.len(),
        "ambiguous SVD array indices {path}"
    );
    ensure!(
        d.dim == 1 || d.dim_increment > 0,
        "overlapping SVD array {path}"
    );
    (d.dim - 1)
        .checked_mul(d.dim_increment)
        .with_context(|| format!("SVD array overflow {path}"))?;
    let element_names: Vec<_> = svd::array::names(info, d).collect();
    ensure!(
        element_names.iter().collect::<BTreeSet<_>>().len() == element_names.len(),
        "ambiguous SVD array element names {path}"
    );
    Ok(Some(Dimension {
        path: path.into(),
        len: d.dim,
        stride: d.dim_increment,
        indices,
        element_names,
        dim_name: d.dim_name.clone(),
    }))
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct Semantics {
    modified_write_values: Option<String>,
    read_action: Option<String>,
    write_constraint: Option<Value>,
}
impl Semantics {
    fn new(
        m: Option<svd::ModifiedWriteValues>,
        r: Option<svd::ReadAction>,
        w: Option<svd::WriteConstraint>,
    ) -> Self {
        Self {
            modified_write_values: m.map(|x| x.as_str().into()),
            read_action: r.map(|x| x.as_str().into()),
            write_constraint: w.map(|x| match x {
                svd::WriteConstraint::WriteAsRead(v) => json!({"writeAsRead": v}),
                svd::WriteConstraint::UseEnumeratedValues(v) => json!({"useEnumeratedValues": v}),
                svd::WriteConstraint::Range(v) => {
                    json!({"range": {"minimum": v.min, "maximum": v.max}})
                }
            }),
        }
    }
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct Field {
    peripheral: String,
    block: String,
    register_path: Vec<String>,
    register_offsets: Vec<u32>,
    field: String,
    declaration: String,
    raw_access: Option<String>,
    derived_access: Option<String>,
    effective_access: Option<String>,
    origin: Option<Origin>,
    dimensions: Vec<Dimension>,
    bit_offset: u32,
    bit_size: u32,
    field_semantics: Semantics,
    register_semantics: Semantics,
    projection: Option<Projection>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct Projection {
    block: String,
    register: String,
    fieldset: String,
    field: String,
}

pub(crate) struct Capture {
    pub device: svd::Device,
    fields: Vec<Field>,
}
impl Capture {
    pub(crate) fn parse(xml: &str) -> Result<Self> {
        let raw = svd_parser::parse_with_config(
            xml,
            &svd_parser::Config::default().validate_level(svd::ValidateLevel::Disabled),
        )?;
        let mut origins = Origins::default();
        origins.add(
            "device",
            "device",
            raw.default_register_properties.access,
            &None,
        )?;
        for p in &raw.peripherals {
            origins.add(
                &p.name,
                "peripheral",
                p.default_register_properties.access,
                &p.derived_from,
            )?;
            if p.registers.is_some() {
                origins.1.insert(p.name.clone());
            }
            origins.registers(&p.name, p.registers.as_deref().unwrap_or(&[]))?;
        }
        for path in origins.0.keys() {
            origins.origin(path)?;
        }
        let index = expand::Index::create(&raw);
        let mut device = raw.clone();
        let mut fields = Vec::new();
        let device_origin = origins.origin("device")?;
        for p in &mut device.peripherals {
            let original_path = p.name.clone();
            if let Some(base) = p.derived_from.take() {
                expand::derive_peripheral(p, &base, &index)?;
            }
            let children_path = block_path(&origins.children_owner(&original_path)?);
            // Header names inherited from a base with no headerStructName must
            // still identify the shared register block, as the old importer did.
            if p.header_struct_name.is_none() {
                p.header_struct_name = Some(children_path.peripheral.clone());
            }
            // Shared block descriptions belong to the declaration that supplied
            // its registers. Peripheral-instance descriptions may differ even
            // when the register map is identical (for example BTIM1/BTIM2).
            p.description = index
                .peripherals
                .get(&children_path)
                .context("resolved peripheral declaration missing")?
                .description
                .clone();
            let block = p.header_struct_name.clone().unwrap();
            let inherited = origins
                .origin(&original_path)?
                .or_else(|| device_origin.clone());
            p.default_register_properties = p
                .default_register_properties
                .derive_from(&raw.default_register_properties);
            let dims = dimension(p, &original_path)?
                .into_iter()
                .collect::<Vec<_>>();
            let defaults = p.default_register_properties;
            if let Some(regs) = p.registers.as_mut() {
                resolve_registers(
                    regs,
                    &children_path,
                    &original_path,
                    &block,
                    &[],
                    &[],
                    &dims,
                    defaults,
                    inherited,
                    &origins,
                    &index,
                    &mut fields,
                )?;
            }
        }
        Ok(Self { device, fields })
    }

    /// Check every captured source field against uncorrected chiptool IR before
    /// any authored register/field removal or width correction is applied.
    pub(crate) fn project(&mut self, peripheral: &str, ir: &ir::IR) -> Result<()> {
        let mut seen = BTreeSet::new();
        for f in self
            .fields
            .iter_mut()
            .filter(|f| f.peripheral == peripheral)
        {
            let mut block = f.block.clone();
            for (i, name) in f.register_path.iter().enumerate() {
                let normalized = replace_suffix(name, "");
                let b = ir
                    .blocks
                    .get(&block)
                    .with_context(|| format!("lost SVD block for {} in {block}", f.declaration))?;
                let items: Vec<_> = b.items.iter().filter(|x| x.name == normalized).collect();
                ensure!(
                    items.len() == 1,
                    "ambiguous/lost SVD register projection {} at {block}.{normalized}",
                    f.declaration
                );
                let item = items[0];
                ensure!(
                    item.byte_offset == f.register_offsets[i],
                    "SVD register offset changed for {}",
                    f.declaration
                );
                let dim_path = format!("{}.{}", f.peripheral, f.register_path[..=i].join("."));
                check_array(
                    &item.array,
                    f.dimensions.iter().find(|d| d.path == dim_path),
                    &f.declaration,
                )?;
                if i + 1 != f.register_path.len() {
                    let ir::BlockItemInner::Block(inner) = &item.inner else {
                        bail!("lost SVD cluster {}", f.declaration)
                    };
                    block = inner.block.clone();
                } else {
                    let ir::BlockItemInner::Register(reg) = &item.inner else {
                        bail!("SVD field parent is not register {}", f.declaration)
                    };
                    let fieldset = reg.fieldset.as_ref().context("SVD fieldset was dropped")?;
                    let name = replace_suffix(&f.field, "");
                    let matches: Vec<_> = ir.fieldsets[fieldset]
                        .fields
                        .iter()
                        .filter(|x| x.name == name)
                        .collect();
                    ensure!(
                        matches.len() == 1,
                        "ambiguous/lost SVD field {}",
                        f.declaration
                    );
                    let found = matches[0];
                    ensure!(
                        found.bit_offset == ir::BitOffset::Regular(f.bit_offset)
                            && found.bit_size == f.bit_size,
                        "SVD field bit span changed {}",
                        f.declaration
                    );
                    let dim_path =
                        format!("{}.{}.{}", f.peripheral, f.register_path.join("."), f.field);
                    check_array(
                        &found.array,
                        f.dimensions.iter().find(|d| d.path == dim_path),
                        &f.declaration,
                    )?;
                    let projection = Projection {
                        block: block.clone(),
                        register: normalized,
                        fieldset: fieldset.clone(),
                        field: name,
                    };
                    ensure!(
                        seen.insert((
                            projection.block.clone(),
                            projection.register.clone(),
                            projection.field.clone()
                        )),
                        "duplicate normalized SVD field {}",
                        f.declaration
                    );
                    f.projection = Some(projection);
                }
            }
        }
        let count: usize = ir
            .blocks
            .values()
            .flat_map(|b| &b.items)
            .filter_map(|i| match &i.inner {
                ir::BlockItemInner::Register(r) => r.fieldset.as_ref(),
                _ => None,
            })
            .map(|s| ir.fieldsets[s].fields.len())
            .sum();
        ensure!(
            seen.len() == count,
            "SVD field projection loss for {peripheral}: {} captured, {count} IR fields",
            seen.len()
        );
        Ok(())
    }
}

#[allow(clippy::too_many_arguments)]
fn resolve_registers(
    regs: &mut [svd::RegisterCluster],
    source_path: &BlockPath,
    peripheral: &str,
    block: &str,
    names: &[String],
    offsets: &[u32],
    dims: &[Dimension],
    defaults: svd::RegisterProperties,
    default_origin: Option<Origin>,
    origins: &Origins,
    index: &expand::Index<'_>,
    out: &mut Vec<Field>,
) -> Result<()> {
    for rc in regs {
        match rc {
            svd::RegisterCluster::Cluster(c) => {
                let declaration = source_path.new_cluster(&c.name);
                if let Some(base) = c.derived_from.take() {
                    expand::derive_cluster(c, &base, source_path, index)?;
                }
                let children_path = block_path(&origins.children_owner(&declaration.to_string())?);
                let mut names = names.to_vec();
                names.push(c.name.clone());
                let mut offsets = offsets.to_vec();
                offsets.push(c.address_offset);
                let mut dims = dims.to_vec();
                dims.extend(dimension(c, &format!("{peripheral}.{}", names.join(".")))?);
                let origin = origins
                    .origin(&declaration.to_string())?
                    .or_else(|| default_origin.clone());
                c.default_register_properties =
                    c.default_register_properties.derive_from(&defaults);
                let defaults = c.default_register_properties;
                resolve_registers(
                    &mut c.children,
                    &children_path,
                    peripheral,
                    block,
                    &names,
                    &offsets,
                    &dims,
                    defaults,
                    origin,
                    origins,
                    index,
                    out,
                )?;
            }
            svd::RegisterCluster::Register(r) => {
                let declaration = source_path.new_register(&r.name);
                if let Some(base) = r.derived_from.take() {
                    expand::derive_register(r, &base, source_path, index)?;
                }
                let owner = origins.children_owner(&declaration.to_string())?;
                let (parent, name) = owner
                    .rsplit_once('.')
                    .context("register owner lacks parent")?;
                let fields_path = block_path(parent).new_register(name);
                let origin = origins
                    .origin(&declaration.to_string())?
                    .or_else(|| default_origin.clone());
                r.properties = r.properties.derive_from(&defaults);
                let mut names = names.to_vec();
                names.push(r.name.clone());
                let mut offsets = offsets.to_vec();
                offsets.push(r.address_offset);
                let mut dims = dims.to_vec();
                dims.extend(dimension(r, &format!("{peripheral}.{}", names.join(".")))?);
                let register_semantics =
                    Semantics::new(r.modified_write_values, r.read_action, r.write_constraint);
                if let Some(fields) = r.fields.as_mut() {
                    for f in fields {
                        let declaration = fields_path.new_field(&f.name);
                        let raw_access = f.access.map(|a| a.as_str().into());
                        let enums_path = if let Some(base) = f.derived_from.take() {
                            expand::derive_field(f, &base, &fields_path, index)?
                        } else {
                            None
                        }
                        .unwrap_or_else(|| declaration.clone());
                        for ev in &mut f.enumerated_values {
                            if let Some(base) = ev.derived_from.take() {
                                // The upstream recursive enum resolver has no cycle
                                // guard. Reject this rare form with a location rather
                                // than risking recursion or silently dropping access.
                                bail!(
                                    "unsupported enumeratedValues derivedFrom at {enums_path}: {base}"
                                );
                            }
                        }
                        let field_origin = origins
                            .origin(&declaration.to_string())?
                            .or_else(|| origin.clone());
                        let mut field_dims = dims.clone();
                        field_dims.extend(dimension(
                            f,
                            &format!("{peripheral}.{}.{}", names.join("."), f.name),
                        )?);
                        out.push(Field {
                            peripheral: peripheral.into(),
                            block: block.into(),
                            register_path: names.clone(),
                            register_offsets: offsets.clone(),
                            field: f.name.clone(),
                            declaration: declaration.to_string(),
                            raw_access,
                            derived_access: f.access.map(|a| a.as_str().into()),
                            effective_access: field_origin.as_ref().map(|o| o.access.clone()),
                            origin: field_origin,
                            dimensions: field_dims,
                            bit_offset: f.bit_offset(),
                            bit_size: f.bit_width(),
                            field_semantics: Semantics::new(
                                f.modified_write_values,
                                f.read_action,
                                f.write_constraint,
                            ),
                            register_semantics: register_semantics.clone(),
                            projection: None,
                        });
                    }
                }
            }
        }
    }
    Ok(())
}
fn check_array(array: &Option<ir::Array>, dim: Option<&Dimension>, path: &str) -> Result<()> {
    ensure!(
        match (array, dim) {
            (None, None) => true,
            (Some(ir::Array::Regular(a)), Some(d)) => a.len == d.len && a.stride == d.stride,
            _ => false,
        },
        "SVD array projection changed {path}"
    );
    Ok(())
}

impl Capture {
    fn signatures(
        &self,
        input: &crate::Input,
        registers: &BTreeMap<String, ir::IR>,
    ) -> Result<BTreeMap<String, Value>> {
        let mut per_peripheral = BTreeMap::<String, (String, Vec<Value>)>::new();
        for f in &self.fields {
            let projection = f
                .projection
                .as_ref()
                .with_context(|| format!("unprojected SVD field {}", f.declaration))?;
            let kind = f.block.trim().to_ascii_lowercase();
            let version = input
                .register_versions
                .get(&kind)
                .unwrap_or(&input.register_version);
            let key = format!("{kind}_{version}");
            // Reuse is about the emitted candidate map. Keep original spans
            // in the source rows, but honor already-checked authored width and
            // removal corrections in the separate candidate signature.
            let Some((_, normalized)) = lookup(&registers[&kind], projection)? else {
                continue;
            };
            let ir::BitOffset::Regular(bit_offset) = normalized.bit_offset else {
                bail!("non-regular candidate field offset {}", f.declaration)
            };
            let dims: Vec<_> = f
                .dimensions
                .iter()
                .map(|d| {
                    json!({
                        "path": d.path.strip_prefix(&f.peripheral).unwrap(), "len": d.len,
                        "stride": d.stride, "indices": d.indices, "element_names": d.element_names, "dim_name": d.dim_name,
                    })
                })
                .collect();
            per_peripheral.entry(f.peripheral.clone()).or_insert_with(|| (key, vec![])).1.push(json!({
                "projection": projection, "register_path": f.register_path, "register_offsets": f.register_offsets,
                "bit_offset": bit_offset, "bit_size": normalized.bit_size, "dimensions": dims,
                "effective_access": f.effective_access, "field_semantics": f.field_semantics,
                "register_semantics": f.register_semantics,
            }));
        }
        let mut maps = BTreeMap::new();
        for (peripheral, (key, mut signature)) in per_peripheral {
            signature.sort_by_cached_key(Value::to_string);
            let signature = json!(signature);
            if let Some(old) = maps.insert(key.clone(), signature.clone()) {
                ensure!(
                    old == signature,
                    "conflicting SVD field-access aliases for {key} at {peripheral}"
                );
            }
        }
        Ok(maps)
    }

    /// Candidate fingerprints include access and semantics without changing the
    /// established register-shape hashes or authored PAC restrictions.
    pub(crate) fn emit(
        &self,
        root: &Path,
        output: &Path,
        input: &crate::Input,
        registers: &BTreeMap<String, ir::IR>,
    ) -> Result<()> {
        let signatures = self.signatures(input, registers)?;
        let target = output.join("field-access-candidates");
        fs::create_dir_all(target.join("maps"))?;
        let mut conflicts = Vec::new();
        for (version, signature) in &signatures {
            let path = target.join("maps").join(format!("{version}.json"));
            if path.exists() {
                let old: Value = serde_json::from_slice(&fs::read(&path)?)?;
                if old["signature"] != *signature {
                    let previous = old["signature"]
                        .as_array()
                        .context("invalid access candidate signature")?;
                    let current = signature.as_array().context("invalid current signature")?;
                    let differences: Vec<_> = current
                        .iter()
                        .filter_map(|row| {
                            let before = previous
                                .iter()
                                .find(|r| r["projection"] == row["projection"]);
                            (before != Some(row))
                                .then(|| json!({"previous": before, "current": row}))
                        })
                        .chain(
                            previous
                                .iter()
                                .filter(|before| {
                                    !current
                                        .iter()
                                        .any(|row| row["projection"] == before["projection"])
                                })
                                .map(|before| json!({"previous": before, "current": null})),
                        )
                        .collect();
                    conflicts.push(json!({"register_version": version, "first_profile": old["first_profile"],
                        "first_source": old["first_source"], "current_profile": input.line,
                        "current_source": {"path": input.source.path, "sha256": input.source.sha256},
                        "differences": differences}));
                }
            }
        }
        let restrictions: cw32_data_serde::register_write::FieldAccesses =
            crate::read_yaml(root.join("cw32-data/field-access.yaml"))?;
        ensure!(
            restrictions.schema_version == 1,
            "unsupported curated field-access schema"
        );
        let mut curated = BTreeMap::<String, Option<ir::IR>>::new();
        let mut comparisons = Vec::new();
        let mut counts = BTreeMap::<String, usize>::new();
        for f in &self.fields {
            let p = f
                .projection
                .as_ref()
                .context("SVD field projection missing")?;
            let kind = f.block.trim().to_ascii_lowercase();
            let version = input
                .register_versions
                .get(&kind)
                .unwrap_or(&input.register_version);
            let version = format!("{kind}_{version}");
            let candidate = lookup(&registers[&kind], p)?;
            let candidate_state = match candidate {
                Some((_, field))
                    if field.bit_offset == ir::BitOffset::Regular(f.bit_offset)
                        && field.bit_size == f.bit_size =>
                {
                    "preserved"
                }
                Some((_, field)) => {
                    ensure!(
                        input
                            .field_width_overrides
                            .iter()
                            .any(|c| c.block == p.block
                                && c.fieldset == p.fieldset
                                && c.field == p.field
                                && c.expected_bit_offset == f.bit_offset
                                && c.expected_bit_size == f.bit_size
                                && field.bit_offset
                                    == ir::BitOffset::Regular(c.expected_bit_offset)
                                && field.bit_size == c.bit_size),
                        "unexplained corrected candidate field span {}",
                        f.declaration
                    );
                    "authored-width-correction"
                }
                None => {
                    ensure!(
                        input.field_removals.iter().any(|c| c
                            .block
                            .as_ref()
                            .is_none_or(|b| b == &p.block)
                            && c.fieldset == p.fieldset
                            && c.field == p.field
                            && c.expected_bit_offset == f.bit_offset
                            && c.expected_bit_size == f.bit_size)
                            || input.register_removals.iter().any(|c| c.block == p.block
                                && c.register == p.register
                                && c.expected_fieldset == p.fieldset),
                        "unexplained corrected candidate field loss {}",
                        f.declaration
                    );
                    "authored-removal"
                }
            };
            if !curated.contains_key(&version) {
                let path = root
                    .join("cw32-data/registers")
                    .join(format!("{version}.yaml"));
                curated.insert(
                    version.clone(),
                    if path.exists() {
                        Some(crate::read_yaml(path)?)
                    } else {
                        None
                    },
                );
            }
            let found = match curated[&version].as_ref() {
                Some(r) => lookup(r, p)?,
                None => None,
            };
            let sidecars: Vec<_> = restrictions
                .registers
                .get(&version)
                .into_iter()
                .flatten()
                .filter(|r| {
                    r.block == p.block
                        && r.register == p.register
                        && r.fieldset == p.fieldset
                        && r.field == p.field
                })
                .collect();
            ensure!(
                sidecars.len() <= 1,
                "ambiguous curated field restriction {version}.{}.{}",
                p.fieldset,
                p.field
            );
            let mut evidence = Vec::new();
            let (state, curated_access) = match found {
                None => ("curated-field-unmapped", None),
                Some((_, field))
                    if field.bit_offset != ir::BitOffset::Regular(f.bit_offset)
                        || field.bit_size != f.bit_size
                        || !same_field_array(&field.array, f) =>
                {
                    ("curated-span-differs", None)
                }
                Some((reg, _)) => {
                    let access = if let Some(s) = sidecars.first() {
                        ensure!(
                            s.bit_offset == f.bit_offset && s.bit_size == f.bit_size,
                            "curated access guard differs from mapped field {}",
                            f.declaration
                        );
                        evidence = s.evidence.clone();
                        "read-only"
                    } else {
                        match reg.access {
                            ir::Access::Read => "read-only",
                            ir::Access::Write => "write-only",
                            ir::Access::ReadWrite => "read-write",
                        }
                    };
                    let state = match f.effective_access.as_deref() {
                        None => "svd-access-unspecified",
                        Some(source) if source == access => "agrees",
                        Some(_) if !sidecars.is_empty() => "curated-restriction-differs-from-svd",
                        Some(_) => "review-required",
                    };
                    (state, Some(access))
                }
            };
            let candidate_span = candidate.map(
                |(_, field)| json!({"bit_offset": field.bit_offset, "bit_size": field.bit_size}),
            );
            let transformation = if candidate_state == "authored-width-correction" {
                let correction = input
                    .field_width_overrides
                    .iter()
                    .find(|c| c.block == p.block && c.fieldset == p.fieldset && c.field == p.field)
                    .context("validated width correction missing")?;
                Some(
                    json!({"id": format!("{}:field_width_overrides:{}.{}.{}", input.line, p.block, p.fieldset, p.field),
                    "kind": "field-width-override", "expected_bit_offset": correction.expected_bit_offset,
                    "expected_bit_size": correction.expected_bit_size, "bit_size": correction.bit_size,
                    "evidence": correction.evidence}),
                )
            } else {
                None
            };
            *counts.entry(state.into()).or_default() += 1;
            comparisons.push(json!({"peripheral": f.peripheral, "register_version": version, "projection": p,
                "effective_svd_access": f.effective_access, "curated_access": curated_access, "status": state,
                "candidate_status": candidate_state, "source_bit_span": {"bit_offset": f.bit_offset, "bit_size": f.bit_size},
                "candidate_bit_span": candidate_span, "transformation": transformation, "existing_curated_evidence": evidence}));
        }
        // Complete validation precedes writes. Each source gets its own full
        // provenance rows, so a later family cannot overwrite an earlier one.
        for (version, signature) in signatures {
            let path = target.join("maps").join(format!("{version}.json"));
            if !path.exists() {
                crate::write_json(
                    &path,
                    &json!({"schema_version": 1, "first_profile": input.line,
                        "first_source": {"path": input.source.path, "sha256": input.source.sha256},
                        "signature": signature}),
                )?;
            }
        }
        crate::write_json(
            &target.join(format!("{}.json", input.line)),
            &json!({
                "schema_version": 1, "review_only": true, "profile": input.line,
                "source": {"path": input.source.path, "sha256": input.source.sha256, "url": input.source.url},
                "unspecified_access_policy": "Unspecified stays null; chiptool's separate register fallback is read-write, not an SVD fact.",
                "fields": self.fields,
            }),
        )?;
        fs::create_dir_all(output.join("reports"))?;
        crate::write_json(
            &output
                .join("reports")
                .join(format!("{}-field-access.json", input.line)),
            &json!({
                "schema_version": 1, "review_only": true, "profile": input.line, "source_sha256": input.source.sha256,
                "captured_fields": self.fields.len(), "unexplained_projection_losses": 0,
                "comparison_counts": counts, "comparisons": comparisons, "incompatible_shared_maps": conflicts,
                "limitations": ["SVD access preservation is not manual or silicon validation.",
                    "Candidate files never activate PAC restrictions. Current emission supports only reviewed read-only field restrictions.",
                    "Existing curated evidence is shown for review; disagreement is not automatically classified as vendor error.",
                    "Field and register write/read semantics are preserved separately, not converted to access."]
            }),
        )?;
        ensure!(
            conflicts.is_empty(),
            "conflicting profiles attempted to share field-access candidate(s): {}; see {}-field-access.json",
            conflicts
                .iter()
                .map(|c| c["register_version"].as_str().unwrap())
                .collect::<Vec<_>>()
                .join(", "),
            input.line
        );
        Ok(())
    }
}
fn lookup<'a>(ir: &'a ir::IR, p: &Projection) -> Result<Option<(&'a ir::Register, &'a ir::Field)>> {
    let Some(block) = ir.blocks.get(&p.block) else {
        return Ok(None);
    };
    let items: Vec<_> = block
        .items
        .iter()
        .filter(|i| i.name == p.register)
        .collect();
    ensure!(
        items.len() <= 1,
        "ambiguous register match {}.{}",
        p.block,
        p.register
    );
    let Some(item) = items.first() else {
        return Ok(None);
    };
    let ir::BlockItemInner::Register(reg) = &item.inner else {
        return Ok(None);
    };
    if reg.fieldset.as_ref() != Some(&p.fieldset) {
        return Ok(None);
    }
    let Some(fieldset) = ir.fieldsets.get(&p.fieldset) else {
        return Ok(None);
    };
    let fields: Vec<_> = fieldset
        .fields
        .iter()
        .filter(|f| f.name == p.field)
        .collect();
    ensure!(
        fields.len() <= 1,
        "ambiguous field match {}.{}",
        p.fieldset,
        p.field
    );
    Ok(fields.first().map(|f| (reg, *f)))
}
fn same_field_array(array: &Option<ir::Array>, field: &Field) -> bool {
    let path = format!(
        "{}.{}.{}",
        field.peripheral,
        field.register_path.join("."),
        field.field
    );
    check_array(
        array,
        field.dimensions.iter().find(|d| d.path == path),
        &field.declaration,
    )
    .is_ok()
}

#[cfg(test)]
#[path = "svd_access_tests.rs"]
mod tests;
