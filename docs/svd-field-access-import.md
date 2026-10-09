# Review-only SVD field-access preservation

`./d import-registers` captures field access before chiptool converts the vendor
SVD into register IR. This affects raw candidate imports only. Normal generation
still reads authored register YAML and the reviewed `cw32-data/field-access.yaml`;
none of the new files is consumed by the PAC emitter.

## Source semantics and provenance

The pinned `svd-parser` parses the original document without property expansion.
Its existing `derive_*` helpers resolve peripheral, cluster, register and field
`derivedFrom` values. A small declaration index checks duplicate paths, reference
kinds, missing bases and cycles, and retains the exact declaring access origin.
Derived values are resolved before containing defaults are applied. Precedence is
field, register, nearest cluster outward, peripheral, then device. A local
property overrides its derived base. Child collections retain their actual
declaring owner even when an intermediate node in a multi-hop derived chain
supplies its own registers, cluster children or fields. Missing access stays null: chiptool's
separate register fallback to RW is labeled compatibility policy, not source fact.

All five SVD modes survive unchanged: `read-only`, `write-only`, `read-write`,
`writeOnce`, `read-writeOnce`. Field and register `modifiedWriteValues`,
`readAction` and `writeConstraint` remain separate properties. They are not
interpreted as access and do not activate command behavior.

Rows identify the input path, SHA-256 and URL, peripheral instance, block,
register/cluster path and offsets, field name and bit span, raw and derived
access, effective access and original declaring path/level. Arrays remain
compact templates with exact count, stride, indices and expanded element names
from the pinned parser. Register, cluster and field dimensions are checked
against the positional IR projection. Peripheral array dimensions remain checked
source metadata; raw import emits no candidate chip projection for them. Shared peripheral descriptions come from the declaration supplying
the register block, so an instance label does not change its shared shape.

The current bounded implementation rejects concrete expanded-array-element
`derivedFrom` targets and enumerated-value `derivedFrom` with their source path.
These forms are absent from the selected official corpus. Unsupported forms,
ambiguous normalized names, missing fields, changed spans and unexplained
post-correction losses fail visibly. They are never silently treated as RW.

## Candidate files and conflicts

The output root contains:

- `registers/`: existing register candidates, with existing reviewed source
  corrections applied.
- `field-access-candidates/<profile>.json`: full source access and provenance.
- `field-access-candidates/maps/<register-version>.json`: access and semantic
  signature of the first source candidate using that version, with its identity.
- `reports/<profile>-field-access.json`: projection-loss accounting, comparisons
  against current curated register access and field restrictions, and every
  incompatible shared-map signature found for that profile.

Signatures include effective access, separate command semantics, post-correction
candidate spans and array identity. Raw spans remain in the source rows; an
already-checked authored width correction does not create an access conflict. Provenance and descriptions do not drive semantic equality. They do
not replace or change the meaning of existing register-shape reuse hashes.
Conflicting later sources retain their own full candidates and difference report;
they cannot overwrite the first map's signature. The CLI continues through all
profiles and returns nonzero when any import fails. `./d import-registers` keeps
these reports in `build/register-candidates` even on rejection. Per-profile import
directories can be used to review all register candidates independently. Existing
candidate-only output may be reused only when its current-run
`candidate-import.json` inventory matches every file. Unknown, modified or
unrecognized content is rejected; per-profile candidate files carry the source
provenance. This inventory is not a historical-source gate. Destinations containing `chips`,
`field-access` or `register-writes` are rejected before any candidate write.
The wrapper preserves its prior reports if Cargo produces no complete candidate report
and copies new reports completely before replacing the previous output tree.

Curated comparisons distinguish agreement, access requiring review, explicit
curated restriction disagreement, missing mappings and changed spans. Existing
manual evidence is shown as evidence, not automatically accepted as a vendor
erratum. Preservation of an already-correct SVD field is not a manual correction.
Any future manual override must be an exact-field, source-hash-qualified record
of a genuine conflict, with the own-family manual page, URL, hash and reason.
This change introduces no such overrides and does not broadly adopt candidates.

The separate [reviewed oscillator promotion](reviewed-status-access.md) consumes
exactly 23 selected captured RO fields as an explicit maintenance operation.
It emits an external candidate for the existing authored access file; neither
capture files nor the evidence selection are consumed by normal generation.

## Verification boundary

Data/parser controls cover inheritance, derived overrides, all access modes,
unspecified values, array projection, independent command semantics, ambiguous
and unsupported input, alias disagreements, report retention and output redirects.
Public importer tests check zero emitted chips or active access restrictions and
preservation of curated files. No HAL tests, production adapter, hardware claim,
new manual erratum or historical reverse-hash gate is introduced.

The current 13 active profiles cover 12 distinct input SVD hashes. Their candidate
capture contains 29,872 per-peripheral field-template rows, including reused
peripheral instances. Every row has explicit access on its source field:
26,281 RW, 3,354 RO and 237 WO; inherited-default and unspecified counts are zero.
The once modes and nonempty write/read semantics are covered by synthetic parser
controls. Seven shared-map/profile pairs disagree in source access (RAM IER.EN,
GPIO IDR fields and GTIM ISR fields). These are review findings, not conclusions
that the hardware differs. Current own-manual review and active restrictions
remain separate from this raw source-preservation boundary. A further raw-span
difference, L031/L083 RAM.ADDR.ADDR at 14 versus 32 bits, is already explained by
an authored width correction; both source fields are RO and the normalized
candidate signatures agree.
