# Stage 13 source checkpoint

Included: ATIM polling counter/main-output PWM on its 11 families; reserved
Flash storage on nine families/25 exact ordering codes; source-backed electrical
metadata; central Embassy-style RCC control; direct typed UART/SPI registers and
source-backed R1W0 command constructors. HAL tests remain deleted. Single-file
modules are flat, and owned HAL/PAC sources contain no path attributes.

Final normal compilation completed 2026-10-08 16:29 UTC: all 108 ARM release
variants across 54 selections/13 families and 49 real firmware binaries, zero
warnings, 1,190 inputs unchanged. See
`verification-logs/stage13-architecture/hal-final/matrix-summary.json`.

Retained data/PAC/source validation completed through the recorded prefix and
resumed tails. Two stale evidence records were reconciled explicitly: the RTC
metadata-addition ledger and seven UART canonical hashes after reviewed enum
additions. RTC register facts and IP sharing assignments did not change. The
final tail and complete deterministic generation/PAC check passed at 17:01 UTC
on 1,472 unchanged inputs; see `data-final-tail2/summary.json` in the same log
directory. Earlier failed receipts are retained rather than relabeled successful.
The combined schema extension passed independent correspondence review;
`combined-schema-extension-review.md` preserves the exact accepted identity.

Independent reviews accepted electrical facts, central RCC control, EXTI
ownership and typed UART/SPI sequencing. No firmware was flashed or executed.
Compilation does not establish silicon, electrical or whole-peripheral coverage.

## Still incomplete

Existing serial, ADC and timer drivers expose bounded subsets. DMA remains an
experimental unsafe x030-only path, without safe cancellation or peripheral DMA
integration. RTC is L011/L012 blocking calendar only. Remaining Flash families,
comparators/LVD, all-family IR (including SYSCTRL-contained IR), AWT/LPTIM, LCD,
AUTOTRIM, L012 DAC/OPA/HALLTIM/CORDIC/EAU, L083 AES/TRNG, RAM parity management,
power/wakeup, additional clock sources and Embassy time-driver remain incomplete.
Further typed-register migration outside UART/SPI is ongoing. The reviewed I2C
candidate is outside this snapshot. RF work is explicitly deferred by the user.
This is a progress checkpoint, not all-chip/all-peripheral completion.
