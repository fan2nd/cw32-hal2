# Combined schema extension review

Independent technical correspondence review completed on 2026-10-08.
The accepted live `cw32-data-serde/src/lib.rs` hash is
`7c1c569ee508dba0e77ce8f9f9eb3f049ad48f99aeb1813ec76bc9edf4ed8416`.

Removing only nine exact additive runs from the separately accepted SPI,
electrical, GPIO interrupt, RCC control and UART register-write changes
reconstructs the independently authored replacement baseline byte-for-byte:
`234455fedd3606505df6eeb5cc0ccddb3fec01d4e1d68bf10ee0ad6a8a06e812`.
No predecessor implementation body or unexplained edit was reintroduced.
The separate new `register_write.rs` exactly matches its authored UART packet:
`a09e62641b7eed05db9c7e92745fcb977c03c126d4284a2fbe427645e4c36e4d`.

Eight optional Peripheral fields default to None and omit when absent. The
original fields, widths, defaults, order and enums are unchanged. Legacy JSON
compatibility is retained; downstream Rust struct literals must supply the new
fields. The new standalone register-write model rejects unknown fields without
altering decoding of the original chip model.

This records technical compatibility and factual source lineage. It is not a
legal clearance opinion, and it does not erase the retained upstream ancestry
or the original package-license uncertainty. Runtime/data verification is
recorded separately in the current checkpoint logs.
