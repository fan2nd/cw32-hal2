# CW32 clock metadata: verified boundary

All thirteen family profiles have exhaustive authored clock inventories in
`cw32-data/clock/`. There are 434 peripheral-view records: 273 project to the
unchanged upstream `Rcc` structure, while 161 remain explicit partial records.
These counts are per family/peripheral view, not unique hardware IP blocks.

## F030/A030 POR clock and reset facts

Official shared [CW32x030 CN V2.5 manual](https://www.whxy.com/uploads/files/20240920/CW32x030_UserManual_CN_V2.5.pdf):

- HSIOSC nominal 48 MHz, default divider 6, hence HSI 8 MHz (§4.3.3).
- POR selects HSI for SysClk; CR0 reset 0 selects HCLK/1 and PCLK/1, so nominal
  SysClk, HCLK and PCLK are all 8 MHz (§4.3.7, CR0 register table).
- CR0 writes require the 0x5A5A key in bits 31:16. Gate registers on these families
  are separate controls and do not use the L010/L011/L012 gate-write protocol.
- Gate bit 1 enables the clock. Reset bit 0 asserts reset; bit 1 releases it.
- POR/BOR establishes default peripheral state. Other resets can preserve state,
  including RTC, so a driver must deliberately reset the intended peripheral.
- HCLK derives from SysClk with divisors 2^n for n=0..7; PCLK derives from HCLK
  with divisors 2^n for n=0..3. APBEN1/APBEN2 do not establish two PCLK domains.

These are nominal POR facts, not a guarantee that a bootloader or earlier code
left clock registers unchanged. The SDK's `SystemCoreClockUpdate()` assigns
8000000 rather than calculating the current configured tree.

Source PDF SHA-256:
`1afd49261f0f0689af8cb8ebf1b0ac1c00e3209b20d3c722106707ff4a10bdd2`.
The companion extracted text has precise checked locations at lines 1820–1829,
2093–2104, 2351, 2965–2996 and 3732–3900; sidecar source manifests pin its hash.

## Deliberate schema limits

The exact upstream schema has no peripheral scope for mux fields, no gate write
key, no reset polarity, and no CW32-specific low-power vocabulary. Local muxes
remain scoped sidecar facts, not fake SYSCTRL selectors. Active-low reset and
keyed gating require CW32-specific consumers. `Stop1` is explicitly conservative
software policy, never a claim about a CW32 hardware mode. FLASH functional-clock
ambiguity and blocks without an established independent gate/kernel remain
partial. See `cw32-data/clock/README.md` for the complete adaptation boundary.

The L052 vendor SVD spells the UART source selector `SORCE`, while its SDK uses
`SOURCE`; scoped sidecar references preserve the curated IR spelling at bits9:8.
Source URLs for L012/L052/L083 manuals were checked against the cached official
catalog and corrected before ingestion. All 153 distinct source artifacts used
by the sidecars were rehashed successfully.

## Verification for this change

- Nine Rust clock tests pass, including validation of all thirteen sidecars
  against authoritative YAML and negative schema/evidence/shared-control cases.
- `tests/test_clock_contracts.py` adds six independent generated-projection tests;
  run it after the adapter is enabled and generated outputs are refreshed.
- No HAL correctness or on-silicon validation is implied by metadata validation.
