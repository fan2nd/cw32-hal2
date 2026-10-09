# L083 HSI-fed PLL examples

Normal standalone ARM firmware using the HAL's one-time clock ownership, UART1 TX on PA8 and a GPIO activity output on PB0. Connect a UART receiver that tolerates the board's I/O voltage and an appropriate PB0 LED circuit. No external oscillator is required. Use one exact package feature; all five modeled L083 packages expose these pads.

`pll-uart` uses factory HSI /6 ×7, nominal 56 MHz SYSCLK/HCLK/PCLK with 54.88–57.12 MHz rate bounds. The declared board envelope is 3.0–3.6 V and −20…70°C. `fractional` selects HSI /10 ×12, nominal 57.6 MHz with 56.448–58.752 MHz bounds. `low-voltage` declares 1.65–1.79 V and adds AHB /4, giving nominal 14 MHz (or 14.4 MHz with fractional); the independent raw PLL limit still applies. Do not choose a declaration the actual board cannot guarantee.

`pll-time`, enabled by `embassy-time`, drives the same output through Embassy timers and owns GTIM1. The default 56 MHz and low-voltage 14 MHz choices exactly divide to nominal 1 MHz. Fractional choices fail the existing exact time-driver admission before acquiring the device. Tick bounds represent clock-rate qualification, not precision timekeeping or a cycle-jitter guarantee.

Example commands, after `./d gen-all` from the repository root:

- `cargo build --locked --manifest-path examples/pll-clock/Cargo.toml --target thumbv6m-none-eabi --no-default-features --features cw32l083rct6 --bin pll-uart`
- Add `,fractional` or `,low-voltage` for the other UART clock configurations.
- Select `cw32l083rct6,embassy-time` with `--bin pll-time` for Embassy timing.

Initialization requires stable, legal incoming clocks/Flash, quiescent DMA/peripheral/interrupt/MCO/PLL_OUT consumers and continuous availability of the active source. It may stop an inherited PLL. On failure, reset before retrying; no requested clocks are published. This example does not enter DeepSleep or perform runtime retuning. L083 ADC strict acquisition/duration APIs are not qualified for the new PLL rate-only source and return a structured constructor error. Direct HSI/HSE ADC behavior is unchanged. See `docs/l083-hsi-pll.md` for original source pages and limitations.

Compilation and strong linked ARM ELF symbols verify the source/build integration only. These examples were not executed on silicon.
