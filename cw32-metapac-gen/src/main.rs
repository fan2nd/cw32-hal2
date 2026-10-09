// Adapted from embassy-rs/stm32-data at 37a22f31552ba1fd29b3ef192c4578b84abee6e1.
// SPDX-License-Identifier: MIT OR Apache-2.0
use cw32_metapac_gen::*;
use std::{env, path::PathBuf};
fn main() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_owned();
    let mut out_dir = root.join("cw32-metapac");
    let mut data_dir = root.join("cw32-data/data");
    let mut chips = Vec::new();
    let mut args = env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--out-dir" => out_dir = args.next().expect("--out-dir requires a path").into(),
            "--data-dir" => data_dir = args.next().expect("--data-dir requires a path").into(),
            _ if arg.starts_with('-') => panic!("unknown argument: {arg}"),
            _ => chips.push(arg),
        }
    }
    if chips.is_empty() {
        chips = std::fs::read_dir(data_dir.join("chips"))
            .expect("run ./d gen first")
            .map(|r| r.unwrap().path())
            .filter(|p| p.extension().and_then(|x| x.to_str()) == Some("json"))
            .map(|p| p.file_stem().unwrap().to_str().unwrap().to_owned())
            .collect();
    }
    chips.sort();
    assert!(!chips.is_empty(), "no chip inputs");
    Gen::new(Options {
        out_dir,
        data_dir,
        chips,
    })
    .run_gen();
}
