# F020/F030/A030 factory-HSI system PLL

This API configures the PLL once during initialization, only when selected as
SYSCLK. `Config.pll` defaults to `None`; direct HSI/HSE and the existing LSE,
RTC and DMA ownership contracts remain supported. There is no HSE-fed PLL,
independent PLL output, runtime retune, DeepSleep restoration or guaranteed
clock-fault recovery API.

## Own-source qualification

The authored facts are in `cw32-data/pll-qualified.yaml` and `electrical.yaml`;
the original identities, versions, pages and conflicts are in
`sources/x030-f020-hsi-pll-source-receipt.json`. Generated metadata and PAC are
build outputs, not source authorities. F020 uses the selected current Rev1.3
datasheet, SHA-256
`1e330d800f10114c654b97cbd45b64d56a99c3cb39138d4ef20de3c12968dab0`.
The historical same-named file prints Rev1.2 and is not silently substituted.
The common x030 manual explicitly covers F030/A030; A030 uses its own datasheet.

Factory HSI is 48 MHz, ±5% on F020 and ±2% on F030/A030, over 1.65–5.5 V and
−40…105 °C. The HSI reference is after the configured `HSI.DIV`; undocumented
SDK /3 encoding7 remains excluded. PLL input must fit 4–24 MHz and one analog
input bin. The literal multiplier is 2–12. The entire multiplied output
must fit one analog output bin and the separate own datasheet cap: F020
12–48 MHz, F030/A030 12–64 MHz. The 12 MHz lower admission limit intersects
the datasheet's 8 MHz lower limit with the lowest documented analog bin.

| Family | Admitted HSI /6 multipliers | Admitted HSI /10 multipliers |
| --- | --- | --- |
| F020 | 2, 4, 5 | 3, 4, 6, 7, 8, 9 |
| F030/A030 | 2, 4, 5, 7 | 3, 4, 6, 7, 8, 9, 11, 12 |

Other documented hardware combinations are unqualified by this conservative
policy, not asserted electrically impossible. Both endpoints use exact rational
arithmetic. A shared exact endpoint selects the first fitting closed bin;
nonzero HSI tolerance must fit in that bin. Voltage, temperature and each
actual bus maximum are checked separately. Both buses must stay ≤24 MHz below
1.8 V; above it F020 permits ≤48 MHz and F030/A030 ≤64 MHz. Dividing the buses
cannot legalize an excessive raw PLL output.

For example, HSI /6 ×4 is nominal32 MHz. F020's actual rate interval is
30.4–33.6 MHz; F030/A030's is31.36–32.64 MHz. At VDD≥1.8 V, AHB/APB /1 use
Flash WAIT1. AHB /4 permits the full low-voltage range with WAIT0. The unchanged
classic Embassy time driver admits an exact nominal1 MHz division only when
its GTIM divisor is a power of two, so these32 MHz and8 MHz buses are suitable.
Other qualified PLL rates can be rejected by that independent timer constraint.

## Initialization and retained sources

Entry already requires a legal stable source, buses, voltage and Flash wait
state. DMA, ordinary peripheral activity, IRQ/NMI clock users, MCO and dedicated
PLL_OUT consumers must be quiescent, and incoming sources must remain available.
This is not recovery from a stopped CPU clock. Factory calibration is read from
the own documented address, not an SDK private-debug address or L083 address.
Retained AWT raw-HSI and LVD-filter owners prevent required HSI retrim; retained
RTC/AWT HSE owners require matching ready HSE and pads before reuse.

The existing RCC path raises Flash to WAIT2 and establishes monotonic AHB/4,
APB/8 guards, preserving stronger inherited dividers. It enables unchanged HSI,
waits for STABLE, selects HSI and acknowledges the mux. Only then does it stop
PLL and observe both PLLEN=0 and PLL.STABLE=0 before any reference/divider or
PLL parameter changes. Required factory retrim uses the existing unchanged
legal LSI bridge after owner checks. LSI remains enabled where external-source
CCS requires it; no retained RTC/LSE owner is reset.

Typed PAC modification programs SOURCE3, literal MUL, qualified input/output
bins and WAITCYCLE7, preserving reserved/debug and upper reserved bits. A
nondefault inherited debug field is rejected before clock mutation. Manual
reset0x00053483 establishes debug0x5; the incomplete SVD reset0 and SDK full
register writes are not used as authority. Parameters are read back before
enable; the real RO STABLE latch, not clearable PLLRDY, acknowledges startup.
The selected PLL is ready before SYSCLK changes. HSI remains enabled.

Final mux/dividers, HSI trim/divider/readiness, PLL source/parameters/readiness,
mandatory CCS, retained external-source state/faults, LSI and Flash settings are
checked before frozen clocks are published. The new PLL path retains inherited
Flash CACHE/FETCH settings; existing `pll=None` paths keep their established
enable-both policy. Flash latency covers both final HCLK and configured HSI
under the final buses. No readiness or IRQ flag is cleared as a lock handshake.

Each wait uses a nonzero finite poll budget. The characterized200 μs analog
lock figure does not bound WAITCYCLE16384 or the software sequence. Errors may
leave HSI, LSI, a disabled/enabled PLL, gates or other partial state changed;
no frozen clocks are published and reset is required before retry. There is
no fabricated rollback or continuous lock-loss guarantee.

## Consumer contract

`ClockBounds::multiplied_by` carries original source numerators and marks the
result rate-only. Every system/bus/kernel/local division retains that qualifier.
The300 ps cycle-to-cycle jitter limit does not establish absolute cycle,
pulse-duration or finite-window phase bounds. Public strict duration helpers
keep their prior checked qualification requirement.

Classic ADC's shared timing admission rejects rate-only before ADC/RCC writes
for blocking and async constructors, reconfiguration and software-triggered
sequences. Classic hardware-trigger/DMA acquisition remains excluded; the
separate L010/L011 triggered owner is not compiled for these three families.
F030/A030 classic complementary PWM rejects immediately after obtaining bounds,
before counter/dead-time synthesis or ATIM/RCC enable, including zero requested
dead time. Its pin wrappers already own and disconnect pads; construction and
error cleanup retain that documented GPIO behavior. No zero-earlier-GPIO-write
promise or extra ownership adapter is introduced.

Classic I2C and SPI remain rate-divider APIs without absolute pulse/setup/hold
claims. USART divider error remains nominal and excludes source error. GTIM,
simple PWM, QEI and capture/trigger expose rates or raw counts. Time-driver
interrupt budgets remain actual ticks, not strict wall-time bounds. Window
watchdog intervals remain nominal PCLK cycles; IWDT uses its independent RC10K.
AWT raw HSI and RTC source capabilities remain separate; RTC CPU delays are
rate-derived spacing between observed access-window checks. LPI2C waveform
proof, buffered complementary PWM, AUTOTRIM, HALLTIM and LPTIM are not newly
qualified for these families.

The firmware in `examples/pll-clock` exercises normal UART/GPIO and Embassy
GTIM applications for one exact package in each new family, alongside the
existing L083 selections. Compilation, source review and linked ELF inspection
can validate software integration; they do not establish on-silicon startup,
frequency, timing or recovery behavior.
