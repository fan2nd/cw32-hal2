# LSE SYSCLK and the same oscillator for RTC

Two ordinary bare-metal firmware examples for CW32F020C6U7, CW32F030C8T7,
CW32A030C8T7, CW32L031C8T6, CW32L031C8U6, CW32L031F8U6, CW32R031C8U6,
CW32W031R8U6, CW32L052C8T6, CW32L052R8S6, CW32L052R8T6, CW32L083RBT6,
CW32L083RCT6, CW32L083RCS6, CW32L083MCT6 and CW32L083VCT6: sixteen exact
packages in total. Family aliases and other packages are excluded. Build with
one exact feature, for example:

```
cargo build --release --locked --manifest-path examples/lse-sysclk/Cargo.toml --target thumbv6m-none-eabi --no-default-features --features cw32f020c6u7,defmt --bin crystal
cargo build --release --locked --manifest-path examples/lse-sysclk/Cargo.toml --target thumbv6m-none-eabi --no-default-features --features cw32f030c8t7,defmt --bin bypass
cargo build --release --locked --manifest-path examples/lse-sysclk/Cargo.toml --target thumbv6m-none-eabi --no-default-features --features cw32l052r8s6,defmt --bins
cargo build --release --locked --manifest-path examples/lse-sysclk/Cargo.toml --target thumbv6m-none-eabi --no-default-features --features cw32l083rbt6,defmt --bins
```

These are board declarations, not measured qualification. Both examples assert
VDD 3.0–3.6 V, ambient −20…70 °C and nominal 32768 Hz with every-cycle frequency
between 32766 and 32770 Hz over supply, load, temperature, aging and short-term
variation. Replace these with the actual qualified board data. HSI /6 and AHB/APB
/1 retain the existing defaults; HSI and factory detector LSI remain enabled.

`crystal` requires the board's 32768 Hz crystal, load capacitors and layout on
PC14/PC15. Their physical positions are 3/4 on the classic and L031 48-pin
packages, 1/2 on L031 QFN20, 2/3 on R031 QFN48 and 61/62 on W031 QFN64. On
all three L052 packages, including both 64-pin R8 packages, PC14/PC15 are
physical pins 3/4. The own L083 package metadata places PC14/PC15 at pins 3/4
on RBT6/RCT6/RCS6/MCT6 and 8/9 on VCT6. The actual crystal and board must
qualify Strong drive, Normal amplitude and the selected startup
count. L052 additionally declares independent startup drive Normal and startup
amplitude Large, deliberately distinct from run drive Strong and run amplitude
Normal. Both pairs and the 16384-cycle startup count require board-specific
qualification; these values are demonstration assertions, not a recommended
universal preset. All four analog fields are installed before enable, never
rewritten at STABLE; the precise hardware phase-switch instant is not promised.
Bypass keeps the same explicit fields for exact-source reuse. L083 has one
analog bank: the shared configuration supplies only run drive/amplitude and
WAITCYCLE there, with no L052 startup-drive/startup-amplitude fields.

`bypass` requires an external digital clock on PC14. On the classic
three parts the reviewed input limits are: high level
0.7×VDDIO…VDDIO, low level VSS…0.3×VDDIO, high and low pulse widths at least
450 ns and rise/fall times at most 50 ns, plus the device's full I/O requirements.
The classic source must retain 45–55% duty and satisfy those limits every cycle.
For the five L031/R031/W031 parts, qualify the corresponding own-datasheet
waveform and operating-condition rows bound by their source contract.
For L052, qualify VDD 1.65–5.5 V with VDDA=VDD and ambient −40…85 °C; the
example asserts the narrower interval above. Its own bypass limits are high
0.7×VDDIOx…VDDIOx, low VSS…0.3×VDDIOx, high/low pulses at least 450 ns,
rise/fall at most 50 ns and 45–55% duty, throughout every declared condition.
The own L083 limits are high 0.7×VDDIOx…VDDIOx, low VSS…0.3×VDDIOx,
high/low pulses at least 450 ns, rise/fall at most 50 ns and 45–55% duty.
Its auxiliary source envelope is 1.65–5.5 V with VDDA=VDD and −40…85 °C;
selecting LSE as SYSCLK requires a declared minimum VDD of at least 1.8 V.
The unchanged example conditions above satisfy this narrower system-target rule.
The 1 MHz input ceiling does not change this example's nominal 32768 Hz source.
PC15 is not consumed by bypass; inherited reservations still apply.

Both call only `Config.lse` plus `Sysclk::LSE`, then borrow the same source with
`LseClock` for RTC. The example epoch is deliberate demonstration provisioning;
replace it with intended wall time. A compatible running RTC is preserved by
`initialize_if_unset`, but fresh LSE admission still requires its documented RTC
reset image. Arbitrary warm handover is not supported.

No fixed 1 MHz time driver is selected. Poll budgets are iterations, not elapsed
startup deadlines. After LSE loss/fallback, published LSE bounds and downstream
timing assumptions are invalid. Errors can leave partial state and require reset;
ordinary reset may retain LSE and POR may be needed. Whole-bank GPIO inspection
can advance sampling/filter/events. Matching cold LSI on L031/R031/W031 can
also resume parked direct consumers without a TRIM/WAIT write. See the
[classic contract](../../docs/classic-lse-sysclk.md) and the separate
[five-package contract](../../docs/l031-r031-w031-lse-sysclk.md).

On L052, admission can run the whole GPIOA/B/C/D/F banks. Matching stopped LSI
can resume UART1..3 SOURCE3 (native SORCE), an admitted manual AUTOTRIM timer
SRC1, GPIO FLTCLK5, MCO SOURCE4, PC4/AF6 on R8S6/R8T6 only, and enabled,
already work-ungated LPTIM ICLKSRC3 or LCD CLKCS0. Closed LCD/LPTIM work gates
stay closed. Newly resumed RTC SOURCE2 is excluded by the accepted state
combinations: cold LSE needs pristine RTC SOURCE0, while reused LSE already
needs ready LSI. The existing pre-Rust quiescent bus-master entry model remains
in force. Configured HSI must be legal at final bus dividers; the separate fixed
fallback budget uses undivided 8.16 MHz and assumes no retained divider values.
See [the exact-three L052 contract](../../docs/l052-lse-sysclk.md).

On the five L083 parts, admission independently covers the configured HSI at
final bus dividers and conservative raw factory-HSI fallback at 48.96 MHz.
The fallback bound grants no HSI/AHB/APB divider credit, even with CLKCCS off;
Flash WAIT2 remains installed after LSE selection. The source declaration's
historical 1.65 V minimum is unchanged for auxiliary LSE use. This new system
target additionally requires the inherited unchanged HSI to be independently
legal for the handoff, even if it was idle, and all affected outputs/consumers
to be quiescent. An inherited enabled PLL escapes through that unchanged ready
HSI, then is intentionally disabled; both PLLEN=0 and PLL.STABLE=0 must be
observed before its reference can change. This stops dedicated PLL outputs
while preserving PLL configuration. It supplies no independent PLL-output
lifecycle, runtime switching or fault recovery. No CR0 write follows the final
intentional LSE selection; electrical fallback headroom does not preserve RTC
or peripheral timing. See the [L083 system contract](../../docs/l083-lse-sysclk.md)
and [own source and package facts](../../docs/qualified-l083-lse.md).

The local script `ci/check-lse-sysclk.sh` declares a future check matrix of
seventeen ordinary libraries and sixteen crystal/bypass package pairs (32
system-clock firmware binaries), plus the existing auxiliary/other-source
regression commands. These counts describe script scope, not executed results.
Record the exact executed subset and its outcomes separately; no hosted CI or
hardware run follows from adding these commands.

The build script obtains exact memory limits from generated metadata and supplies
`-Tlink.x` explicitly. Compilation/link/ELF inspection do not run this firmware or
qualify physical startup, detection windows, board timing, fault recovery or RF.
