# V4 correspondence supplement to the Stage55 runtime review

2026-10-09. PASS for carrying the accepted V2 runtime review to the exact V4 source freeze. This is a narrow source/evidence comparison; the earlier runtime review and receipt are preserved unchanged. No Cargo, HAL tests/harnesses, hardware execution, candidate edit, cleanup, upload or publication was performed by this reviewer.

Only two of the 1,195 source entries differ from V2: `cw32-data/register-reuse.yaml` and `sources/SOURCES.md`. There are no additions/removals. All V4 source hashes match the files. All 902 generated hashes match; the generated manifest is byte-identical to V2. Every Rust file, generated hardware value, HAL build input and ordinary example configuration is unchanged from the reviewed runtime freeze.

The ledger change is the parent-approved patch `5da3b178ed49bc2ef96a2ff5f29476b7521fb32856e5cbd88c8ca408fa936642`: two current L011/L012 normalized-IR identities plus explicit historical omission records. Both original register files remain byte-identical to Stage54. This reviewer independently recalculated all 137 current normalized generated IR identities and found zero mismatches. The final `python-data-contracts-v3` receipt reports exit 0; its log records 155 Python data contracts passing. The prior failing V2 log remains visible. No runtime or generated register correction was needed.

The source-index document adds current HSE-PLL scope, exact own-source pages, crystal documented-composition limits and bypass waveform conditions, while preserving independent raw caps, rate-only limits and exclusion of loss recovery/runtime retuning/independent output. Its changed references agree with the already reviewed primary evidence. Parent separately owns acceptance of the wider shared-document wording.

The final matrix receipt is `matrix-acceptance-v3.json`, SHA-256 `a5075fe30fa1585d9f6ad17ea1541c2c6172d03c6915a251015801c9058b6fbd`. It preserves the eight library and eighteen ordinary ELF builds. All eighteen retained ELF bytes still match this reviewer’s V2 receipt. The enhanced layout receipt is `eed240c8e8a84e762649be4946947a3cbe53cb49c4343064fe935f0900cfb6db`; it adds the six time-image checks already independently established in the runtime review. GTIM1 is external IRQ16, at vector-table slot32, not slot16. The final wording now states that correctly. These additions do not imply hardware execution or new build coverage.

V4 source manifest SHA-256: `ab97796949b83b076c78e08db7278f62ddce6df200eee14fc14800d49a268ef1`.
V4 Stage54 delta SHA-256: `b67a72a215830d7542434a4eb2801ab6cb1eacbceb25a768ebcc4216f482b44b`.
V4 generated manifest SHA-256: `d93172e39c8aa1e0d5d8b6f17d8d17b74eb5a1968394db5fa0fb12dc5b76b54d`.

No runtime-review blocker remains for these bytes. Packaging may append independent review artifacts and final inventory; this supplement does not authorize source changes hidden within that packaging or claim a whole-archive review before that archive exists.
