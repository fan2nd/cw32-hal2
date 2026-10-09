# Independent narrow PLL schema review

Accepted for the isolated CW32L083 factory-HSI-fed PLL addition. The candidate schema identity may advance to the reviewed current hash; preserve previous review history. Final immutable source/build acceptance remains separate.

Baseline archive: `embassy-cw32-stage39-source-2026-10-09.zip`, archive SHA-256 `2d052b2b3b9a5489e797991109df4fb197042f5caf2382e13f04349206d2bd03`.

## Exact schema identities

- `cw32-data-serde/src/lib.rs`
  - Stage39: `e70e0c81debc6484b2949b12590fe54670001188d5caf23f3d432a61c24de133`
  - Reviewed candidate: `c42ba9daafed740a5ef3772d99849f2f5e1dcc99c9a09bd5a3331616dd82c959`
- `cw32-metapac-gen/src/data.rs`
  - Stage39: `c53354b75e9b50c72714f3476a3da9ecfc32dcaa29342968bf604bc32f10e6d0`
  - Reviewed candidate: `f35909acfb0b7a69fb57d6e38ec342688c78130626d3932b990bc462d0eb0821`
- `cw32-metapac-gen/res/src/metadata.rs`
  - Stage39: `2b1fa6d62be812e01ee2dbf9e90626cc6f1b658960fcbd99cf9fa221455ba4e8`
  - Reviewed candidate: `a906816ea381eb9784ba1b769b0eab4f2c7a63885e7896cc3b6d5286ad5bbea9`

## Reviewed change

Adds the project-authored `PllLimits` / `PeripheralPllLimits` record for separate electrical intervals, four input bins, five output bins, multiplier limits, supply/temperature qualification, HSI support, startup cycle/count encoding, required inherited debug default, and the explicitly cycle-to-cycle jitter value. Owned arrays retain fixed4/5 lengths; static metadata uses references to arrays of those same lengths. All scalar types and declaration order agree.

Appends `ClockLimits.pll: Option<PllLimits>` after HSE/HEX. Serde accepts missing values as `None` and omits `None` on serialization. Metapac owned input also defaults the optional field; static metadata carries the corresponding option. Existing HSE/HEX definitions and every prior field/type/width/order/Serde attribute remain unchanged. Existing Rust struct literals must supply the new field. This does not expand HSE/HEX qualification.

The authored policy is pinned by hash and equality-checked against electrical metadata. The generator restricts the capability to CW32L083 with HSI support and verifies its own selected manual/datasheet hashes, chip scope and page lists. Generated PLL metadata appears on the six L083 generic/package profiles and nowhere else. L052 retains no PLL qualification or API.

Own sources: U083 Rev2.0 PDF59–60,72,82 (printed58–59,71,81) and D083 Rev1.9 PDF47,55,56 (printed46,54,55). The4…24MHz input and12…64MHz qualified output intersection are distinct from analog bins. The300ps cycle-to-cycle jitter value does not supply an absolute period/phase guarantee.

The separate PAC enum alias correction for FREQOUT codes5/6/7 agrees with the manual's documented1xx encoding; HAL retains canonical code4. It does not alter this Rust metadata schema delta.

The companion JSON preserves the complete exact diffs and all reviewed hashes. This review is source/diff/generated-output inspection; it ran no Cargo, HAL tests, runtime model, or firmware. Build evidence remains separate. No independent legal clearance is claimed.
