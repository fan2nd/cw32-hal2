# Peripheral clock management

The HAL uses the Embassy `SealedRccPeripheral` / `RccPeripheral` / `RCC_INFO`
boundary. The reference is `embassy-stm32/src/rcc/mod.rs` and its `build.rs` at
pinned Embassy commit `f16efeffe37581092ec184718e6fdb1620393214`.

`cw32-data/clock` remains the authored, source-located control inventory.
Data generation independently validates and projects its controller facts into
`Peripheral.rcc_control`; metapac exposes the typed static record. HAL build
uses that record and the selected SYSCTRL IR to generate one `RccInfo` per
peripheral. Register alignment, access, width, field offset and width, key
placement, and agreement with complete legacy `rcc` records are checked.
Driver bodies do not choose a gate register or reconstruct a mask from an
instance number or family.

The central RCC owner performs gate writes, optional write keys, readback,
and active-low reset pulses. UART, SPI, I2C, GTIM, ATIM and BTIM now consume its
operations directly. Type-erased serial drivers retain their generated
`RccInfo`; timer owners use their RCC-bound singleton type. Existing UART
private lifecycle methods still guard the shared interrupt state.

## Deliberate CW32 differences

- CW32 protected gate writes replace the key field while preserving neighboring
  gate bits. Reset fields are active-low and unkeyed. An independent reset pulse
  occurs after gate enable/readback, preserving the established CW32 ordering.
- Upstream STM32 counts shared gates and can reset the first user or disable the
  last. That cannot establish exclusive ownership of CW32 BTIM siblings or
  external/analog owners. Shared controller groups are retained: their reset is
  never asserted and their gate is never disabled by these operations.
- No STM32 Stop-mode, low-power accounting or multi-core behavior is imported.
- Bus frequency and bounds are independent of kernel frequency. Kernel queries
  return `None` for unqualified or peripheral-local/multiple source selections.
  UART and L012 I2C constructors preflight the qualified bus source because they
  explicitly program PCLK afterward. Queries do not probe unclocked registers.
- Caller-specified readback operations support retained analog/watchdog/RTC
  lifecycle contracts without reintroducing controller register routing.

## Preserved serial/timer controller sequences

`R` and `W` below denote one volatile register read or write. Every gate update
is `R(gate), W(gate)`; a keyed write replaces only the key field and owned bit.
A pulse is `R(reset), W(asserted), R(reset), W(released)`.

| Owner | Enable acknowledgement | Reset after acknowledgement | Disable acknowledgement |
| --- | --- | --- | --- |
| UART | At most 100,000 reads, no inserted spin hint | Pulse, no extra read | At most 100,000 reads |
| SPI | At most 100,000 reads, spin after unsuccessful read | Pulse, then one read | None |
| Classic I2C | At most 100,000 reads, spin after unsuccessful read | Pulse, then one read | None |
| L012 I2C | At most 32 reads, spin after unsuccessful read | Pulse, then one read | One read |
| GTIM/ATIM | One read | Pulse, then one read | One read |
| BTIM | One read | Never | No write or read |

L012 I2C still returns its existing `ClockGateTimeout` error. UART/SPI/classic
I2C retain their prior panic behavior when the gate fails to acknowledge. UART
activation remains after configuration; split-half teardown and shared-vector
partner safety are unchanged. L012 I2C still leaves its clock running when
recovery cannot establish a safe stopped state.

## Verification scope

An independent correspondence audit of all 54 chip features checks 698 serial
and timer instances: 169 UART, 86 SPI, 83 I2C, 151 GTIM, 47 ATIM and 162 BTIM.
It compares the previously reviewed routes with the authored controller records
and selected register IR, checks active polarity/key/sharing, and verifies
neighbor-preserving gate updates with 138 register-word samples per state,
including every basis bit. This is a source/metadata and operation-order audit,
not execution of a HAL test harness or evidence of silicon behavior.

Normal Cortex-M0+ release builds with `rt,defmt` passed for exact F030, F002,
F003, L010, L011, L012, L031, L052 and L083 package features. Genuine ATIM
counter/PWM examples linked for F030, L012 and L083; existing F030 blocking and
async GPIO examples also linked. The resulting binaries are ARM EABI5 ELF32
executables. Three typed-control projection tests and six existing clock
metadata-contract tests passed. These checks supplement the final integration
matrix and do not replace it. No firmware was flashed or run.
