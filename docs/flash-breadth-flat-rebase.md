# Flash breadth rebase onto flattened production layout

The accepted Flash breadth implementation is rebased onto the current Stage12
layout in an isolated tree. Main is untouched. The accepted packet SHA-256 is
70e022c96b575d5d2a88d1423af8e088462d413936c2d7cf9a3d556eb09a60b5.

Only four HAL files differ from the current-main snapshot: build.rs, src/lib.rs,
flash/mod.rs and flash/backend.rs. The backend implementation is unchanged from
the accepted backend/mod.rs; its location is now a flat leaf within the real
Flash group. Flash module changes relative to the accepted implementation are
comments clarifying alias policy. build.rs contains only the accepted Flash cfg
and exact-part qualification additions rebased onto the current shared hooks;
lib.rs changes only the Flash cfg gate. All other 61 HAL files, including the
single shared gpio.rs, remain byte-identical. No old module trees, tests or
test-only hooks are imported.

The 25 exact-part allowlist and all reviewed register/clock/cache/ownership
behavior are unchanged. L010/L011/L012/L083 are still excluded; no geometry or
capacity has been inferred or newly qualified. The real storage example source is
unchanged from the accepted packet and is only compiled/linked, never run. Its
Cargo.lock is refreshed offline to include current main's serde_json build
dependency and seven newly required lock entries; existing versions are unchanged.

## Alias terminology correction

Generic family profiles have no memory metadata. In contrast, the four F030
package-neutral compatibility profiles already have verified capacities:
CW32F030C8/K8/F8 have 65536 bytes and CW32F030F6 has 32768 bytes. Those values are
read from existing metadata and are unchanged. These aliases are not qualified
for this storage API, whose current boundary is exact ordering codes. They
continue to report FLASH_SIZE=None and the existing Error::UnknownCapacity at
region construction. That result expresses API qualification for aliases, not
an assertion that their physical array size is unknown. This is a wording-only
correction, with no widening or change of runtime behavior.

The original source review and pre-flattening matrix remain historical evidence.
Current file identities and the rebase review are recorded separately in
flash-breadth-flat-review.json; current normal builds and real-example ELF
inspection are in flash-breadth-flat-verification.json. No HAL unit, integration,
compile-negative, synthetic link or model harness is added or restored.
