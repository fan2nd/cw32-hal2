# L031/R031/W031 RCC module split validation

Baseline: `cd0a91e9a0070dc30f86a960c5c758c27e906efa`. This batch replaces the single 2,094-line RCC backend with five responsibility modules rooted at `mod.rs`. No clock algorithm, public API, supported chip qualification, or hardware evidence boundary is changed. Whole `configure` retains its original ordered body. The source inverse and reference migration are recorded in [the relocation manifest](l031-r031-w031-module-relocation.json).

## Source and evidence correspondence

Independent review reconstructed every baseline source byte and line exactly once after reversing 36 restricted-visibility additions and 14 equivalent ancestry rewrites. It checked 37 function declarations and 12 types; eight semantic mutations were rejected without depending on file hashes. The three live capability references become seven references. Exact inverse YAML bytes and complete parsed-document equality pass; 21 mutation cases are rejected and 16 historical evidence/gate inputs stay byte-identical. Existing historical receipts are not rewritten to claim fresh hardware acceptance.

All 489 HAL/example inputs were frozen before integrated testing and checked after each accepted group. The 910 generated data/PAC files remain byte-identical. No production-generator, shared RCC root or unrelated peripheral changes are included.

## Actually executed software checks

- 113 ARM release-library commands passed: every 54 chip selections with rt and rt,defmt, plus the five existing time-driver selections.
- Three additional affected-family GTIM1 time-driver release builds passed, one exact package each for L031/R031/W031.
- 34 affected L031/R031/W031 firmware-link commands passed across HSE, LSE SYSCLK, retained LSE, LSI, RTC calendar and alarm examples. Commands build all enabled bins; this is not a count of boards or physical devices.
- 50 API probe outcomes passed with explicit expected compiler diagnostics: public root API paths, private-backend visibility rejection, unavailable PLL rejection, generic and unqualified-package LSI/LSE rejection and qualified exact-package low-speed source acceptance.
- Eleven actual-source pure-helper tests passed: bounded polling, exact poll budgets/error return, inspection error mapping/restore priority, and all documented HSI/AHB/APB divider factors. Ten extracted source items were compared to baseline. No PAC access or full configure execution occurs in this host harness.
- Sixteen clock/RCC/source/capability commands passed, including all existing six-family LSE checks.
- Source-provenance tests, generation-transaction tests and `./d provenance` passed.
- `./d lint`, `./d check`, `./d test`, and `git diff --check` passed with completed exit status 0, not merely a partial success log.
- Six normal rustdoc configurations passed for generic and exact L031/R031/W031; preserved public root paths were checked. The known strict whole-crate numeric-bracket warnings in unchanged flash documentation are not claimed fixed.

The source author also ran 22 focused ARM check commands against the frozen source. These are checks, not additional release builds and not hardware validation.

## Fixture corrections and limits

The initial host-test fixture incorrectly assumed HSI divide-by-32/64/128/256 variants; it was corrected to the actual divide-by-6/10/12/14 variants. No production code changed. An API fixture initially assumed every exact package exposed LSI/LSE; it was narrowed to the unchanged five-package qualification roster, with negative probes for the other exact packages. The temporary API package needed its lock adapted. Two probe outcomes produced with a regenerated, newer dependency lock were excluded; the accepted rerun uses a firmware-lock dependency subset with matching versions and checksums. Failed/excluded attempts remain recorded separately.

A clean tracked-source plus explicit-new-file staging tree is used for source packaging; local scratch files, vendor source copies and test executables are not distributed. The existing direct-from-worktree scratch inclusion hazard remains separate tooling work. The clean-stage packaging command completed with exit 0, producing 1,593 archive entries and passing distribution/module-layout validation; all staged source hashes match the accepted working tree.

No hardware, oscillator timing, interrupt behavior, physical clock transition, source-loss recovery, Miri, or remote CI pass is claimed. Low-power and RF remain deferred; previously documented evidence blockers are not opened by this relocation.

## Command and evidence index

The library matrix is the 113-command selection equivalent of `bash ci/check-hal.sh --matrix-only`. Each accepted command uses locked dependencies and `thumbv6m-none-eabi`; targeted family-time builds add `rt,defmt,time-driver-gtim1`. Example links use `cargo build --locked --release --bins --target thumbv6m-none-eabi --manifest-path examples/<group>/Cargo.toml --no-default-features --features <chip>`. API probes use locked offline `cargo check`, with expected rejection diagnostics E0599/E0603/E0432.

Individual command arrays, terminal exit codes, SHA-256 log hashes, failed attempts and frozen input inventories are retained in the validation artifacts. The following SHA-256 values identify those final records; local log content is not bundled in the source package.

- source-examples-snapshot.json: `76f7a35439ad23d0800a4779b8468301b4131585021481c638a8195a2bc4e8cb`
- generated-before.json: `7b3976aa0d132abd091fe14b8423bc512e87642e5c5d9e1f5cbf5770dbaccfe3`
- matrix-results.json: `8a057acd464670c38630518fc0d1b8d5b833ed44d30a7abeeb918a73cf2f2139`
- examples-results.json: `ee6558fb5035f143fef6b67f4b5c6332c3e102f546c6816e3c7d07341a9a5a00`
- api-results.json: `cf99f47231f81a72532e9c2d616c113dc472adab23b6ffbf5729d021d61369d9`
- family-time-results.json: `dd0389ac90b1604398a3e2f6df319c3738b1fef34e255b27bd2a1a1b96836820`
- doc-results.json: `f50a2280bab198bef571b36d80f650606b8c3e95e34c705b1075f505927006d8`
- sources-results.json: `2ed999b0fa9e936636b2c1a1256355f430fa0a3800e57f6cae6a3590696cd33d`
- quality-results.json: `cda20445e7d4a96fc06bae8eaaf7c4b409f05db61abae855f34b6d02d64a59af`
- quality-supplement-results.json: `5b6433b400ed31003c56948710a37f157baf8b66c12f04e35993d69dbf86795c`
- host-results.json: `34901aef9a11579405eb288ee141e2d67d69e11688dafaf6f9a36f1735460853`
