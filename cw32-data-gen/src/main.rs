use anyhow::{Context, ensure};
use std::{env, path::PathBuf};
fn main() -> anyhow::Result<()> {
    // Preserve the historical compiled-checkout default; ./d passes its root explicitly.
    let mut root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_owned();
    let mut output = None;
    let mut requested_manifest = None;
    let mut import_candidates = false;
    let mut args = env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--root" => root = args.next().context("--root requires a path")?.into(),
            "--import-registers" => import_candidates = true,
            "--out-dir" => {
                output = Some(PathBuf::from(
                    args.next().expect("--out-dir requires a path"),
                ))
            }
            "--manifest" => {
                requested_manifest = Some(PathBuf::from(
                    args.next().expect("--manifest requires a path"),
                ))
            }
            _ => anyhow::bail!("unknown argument {arg}"),
        }
    }
    let root = root
        .canonicalize()
        .with_context(|| format!("resolve workspace root {}", root.display()))?;
    ensure!(
        root.is_dir(),
        "workspace root is not a directory: {}",
        root.display()
    );
    // Derive the default only after parsing so argument order cannot select a stale root.
    let output = output.unwrap_or_else(|| root.join("cw32-data/data"));
    let mut manifests = if let Some(path) = requested_manifest {
        vec![path]
    } else {
        std::fs::read_dir(root.join("cw32-data/inputs"))?
            .map(|r| r.map(|r| r.path()))
            .collect::<Result<Vec<_>, _>>()?
            .into_iter()
            .filter(|p| p.extension().and_then(|s| s.to_str()) == Some("yaml"))
            .collect()
    };
    manifests.sort();
    let mut import_failures = Vec::new();
    for manifest in manifests {
        let input: cw32_data_gen::Input = cw32_data_gen::load_input(&manifest)?;
        if let Some(reason) = input.quarantine {
            eprintln!("QUARANTINED {}: {reason}", input.line);
            continue;
        }
        let report = if import_candidates {
            match cw32_data_gen::import_register_candidates(&root, &manifest, &output) {
                Ok(report) => report,
                Err(error) => {
                    eprintln!("IMPORT REJECTED {}: {error:#}", input.line);
                    import_failures.push(input.line);
                    continue;
                }
            }
        } else {
            cw32_data_gen::generate(&root, &manifest, &output)?
        };
        println!(
            "{}: {} peripherals, {} blocks, {} registers, {} fields, {} IRQs",
            input.line,
            report.peripherals,
            report.register_blocks,
            report.registers,
            report.fields,
            report.interrupts
        );
    }
    ensure!(
        import_failures.is_empty(),
        "candidate import rejected profiles: {}",
        import_failures.join(", ")
    );
    Ok(())
}
