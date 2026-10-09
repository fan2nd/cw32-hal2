# F020 ADC12 and GTIM integration checkpoint

Completed 2026-10-08 09:17 UTC. This is a scoped F020 checkpoint on top of stage 6,
with exact F030/A030 regression controls. It is not a new full 54-feature HAL matrix.
No firmware was flashed or executed on a microcontroller, and no hardware behavior
or electrical accuracy is established by these checks.

## Accepted checks

- `cw32f020`, `cw32f020f6u7`, `cw32f020k6u7` and `cw32f020c6u7` each passed
  224 unit tests, 0 IRQ tests, 2 compile-fail doctests, and Cortex-M0+ release
  builds with `rt,defmt`.
- F030C8T7 and A030C8T7 regression controls: each passed 231 unit tests,
  2 interrupt-binding tests, 5 doctests and the same ARM release build.
- Exactly six ARM configurations were built for target `thumbv6m-none-eabi`, each
  with `--release --no-default-features --features CHIP,rt,defmt`.
- Every F020 external analog route and every qualified PWM route compiled:
  ADC 9/11/13 and PWM 17/32/46 for QFN20/QFN32/QFN48; alias intersection 9/17.
- All 104 F020 ADC/PWM diagnostic-specific intentional compile failures passed,
  covering ownership/lifetimes, wrong pin/channel/instance, and unavailable ADC14,
  external-reference, DMA, capture, async and ATIM APIs. The same expanded ADC/PWM
  suites passed on both x030 controls, with 52 intentional failures total.
- Existing F020 GPIO/serial/CRC regression passed 70 intentional failures.
- All three exact parts linked real ARM ELF smoke images containing ADC12, PWM,
  counter, GPIO, UART and CRC paths, verified FLASH/RAM bounds and UART3 vectors.
  QFN20/QFN32 used 16,706 FLASH bytes; QFN48 used 16,834; each used 44 static RAM
  bytes, excluding stack. These binaries were not executed.
- Full `./d test` passed: generator boundary tests, package/metadata contracts,
  source/PAC inventories and parity, access protections, original-source ADC/PWM
  and serial route replay, watchdog/CRC/GPIO/L011 audits and evidence-acquisition
  tests. F020 replay independently checked 15 source hashes, all 73 GTIM AF cells,
  and all 13 ADC manual/SDK/PDF-grid mux and pin rows.
- Main regeneration exactly reproduced the reviewed data/PAC bytes. Owned module
  layout, meaningful cfg naming and repository-wide formatting checks passed.

## Integration and cache recovery

The isolated reviewed patch was merged without replacing stage 6 runner, source
acquisition or documentation updates. The pre-existing formatting corrections were
retained. README and `d` received only the F020 additions; the F020 report now matches
mandatory original-PDF replay and the newly pinned pdfplumber dependency. All 15
required F020 inputs were already in the acquisition manifest, so no source paths,
archive members or recovery behavior were silently overwritten.

An initial generator invocation reused an old cached binary because copied files
retained earlier timestamps. It rejected the new analog schema before replacing any
generated output. This attempt is retained separately and is not accepted evidence.
All copied functional/generated files and relevant Cargo/build inputs were touched
before successful regeneration and accepted testing. The final all-input refresh
covered 43 paths; their mtimes are recorded alongside final hashes. All integrated
source hashes remained unchanged throughout the accepted validation pass.

## Reports

- Implementation/electrical scope: `f020-adc-timer-implementation.md`
- Qualified route proof: `f020-adc-pwm-routes.md` and its JSON evidence
- Accepted main logs: `verification-logs/stage7-f020/main/`
- Isolated pre-integration logs: `verification-logs/stage7-f020/pre-integration/`
- Final source hashes, input invalidation record and result counts:
  `verification-logs/stage7-f020/integration-manifest.json`

The earlier stage 6 checkpoint and logs are unchanged. Further shared-engine
variants require a new regression pass before a combined release claim.

## Primary source hashes

- F020 own RM CN V1.4: `279521eaf7d32b241e614a9553044a4d99c1e7ca2566efa3b0fdd2b66c106eed`
- Current printed DS CN V1.3: `1e330d800f10114c654b97cbd45b64d56a99c3cb39138d4ef20de3c12968dab0`
- F020 SDK V1.2 archive: `1d77fece47a0c615c8ae51374ea946b17ab489042222f33e38d93e6969945b5d`

The integration manifest records all 15 audited source-file SHA-256 values,
including selected SDK members and derived source texts. The legacy misnamed
Rev1.2 datasheet is recorded separately and was not substituted for the current
Rev1.3 AF or electrical evidence.
