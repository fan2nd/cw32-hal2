//! Convert own-manual rational CORDIC domains to inclusive Q1.31 metadata.
use anyhow::{Context, Result, ensure};
use cw32_data_serde::chip::{
    self,
    core::peripheral::{Cordic, CordicDomain},
};
use serde::Deserialize;
use std::{collections::BTreeMap, fs, path::Path};

#[derive(Deserialize)]
struct Catalog {
    schema_version: u32,
    family: String,
    source_ref: String,
    printed_pages: Vec<u32>,
    pdf_pages_1_based: Vec<u32>,
    format: String,
    domains: BTreeMap<String, Domain>,
}
#[derive(Deserialize, PartialEq)]
struct Domain {
    minimum: (i64, i64),
    maximum: (i64, i64),
    maximum_inclusive: bool,
}
#[derive(Deserialize)]
struct Evidence {
    manual: Manual,
    q31_domains: BTreeMap<String, Domain>,
}
#[derive(Deserialize)]
struct Manual {
    source_ref: String,
    sha256: String,
    printed_pages: Vec<u32>,
    rendered_table_pdf_pages: Vec<u32>,
    pdf_page_offset: u32,
}
impl Domain {
    fn q31(&self, name: &str) -> Result<CordicDomain> {
        let scale = |(n, d): (i64, i64)| -> Result<(i64, i64)> {
            ensure!(d > 0, "CORDIC denominator must be positive");
            let n = n.checked_mul(1i64 << 31).context("CORDIC bound overflow")?;
            ensure!(n != i64::MIN, "CORDIC bound cannot be negated");
            Ok((n, d))
        };
        let (n, d) = scale(self.minimum)?;
        let minimum = -(-n).div_euclid(d);
        let (n, d) = scale(self.maximum)?;
        let maximum = if self.maximum_inclusive {
            n.div_euclid(d)
        } else {
            -(-n).div_euclid(d) - 1
        };
        ensure!(minimum <= maximum, "empty CORDIC domain");
        Ok(CordicDomain {
            name: name.into(),
            minimum: minimum.try_into()?,
            maximum: maximum.try_into()?,
        })
    }
}

pub(crate) fn apply(root: &Path, line: &str, core: &mut chip::Core) -> Result<()> {
    let Some(peripheral) = core.peripherals.iter_mut().find(|p| p.name == "CORDIC") else {
        return Ok(());
    };
    let data: Catalog = crate::read_yaml(root.join("cw32-data/accelerators.yaml"))?;
    let evidence: Evidence =
        serde_json::from_slice(&fs::read(root.join("docs/l012-math-evidence.json"))?)?;
    ensure!(
        data.schema_version == 1 && data.family == line && data.format == "signed Q1.31",
        "unqualified CORDIC format/family"
    );
    ensure!(
        data.source_ref == evidence.manual.source_ref
            && evidence.manual.sha256.len() == 64
            && data.pdf_pages_1_based == evidence.manual.rendered_table_pdf_pages
            && data
                .printed_pages
                .iter()
                .all(|p| evidence.manual.printed_pages.contains(p))
            && data
                .printed_pages
                .iter()
                .map(|p| p + evidence.manual.pdf_page_offset)
                .collect::<Vec<_>>()
                == data.pdf_pages_1_based,
        "CORDIC own-manual table qualification differs"
    );
    ensure!(
        peripheral
            .registers
            .as_ref()
            .is_some_and(|r| r.kind == "cordic" && r.version == "cw32l012_v1"),
        "unqualified CORDIC layout"
    );
    ensure!(
        data.domains.keys().map(String::as_str).collect::<Vec<_>>()
            == [
                "ATANH",
                "HYPERBOLIC",
                "LN1",
                "LN2",
                "LN3",
                "LN4",
                "SQRT0",
                "SQRT1",
                "SQRT2"
            ],
        "unreviewed CORDIC domain inventory"
    );
    ensure!(
        data.domains == evidence.q31_domains,
        "CORDIC domains differ from own-source table review"
    );
    peripheral.cordic = Some(Cordic {
        domains: data
            .domains
            .iter()
            .map(|(name, domain)| domain.q31(name))
            .collect::<Result<_>>()?,
    });
    Ok(())
}
