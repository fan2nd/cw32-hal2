# GPIO ISR access corrections

Review date: 2026-10-08. These narrowly scoped access corrections follow the
[asynchronous GPIO audit](gpio-async-read-only-audit.md). This report itself does
not claim asynchronous HAL capability or hardware validation.

## Corrected sources and consumers

Only ISR at byte offset 0x34 changes from ReadWrite to Read in six authoritative
YAML files. Eight input overrides retain original-access expectations and
independent own-family evidence for five profiles.

| Canonical source | Block | Consumer profiles and instances |
| --- | --- | --- |
| gpio_cw32f002_v1 | GPIO | F002 and F003: GPIOA, GPIOB |
| gpioc_cw32f002_v1 | GPIOC | F002 and F003: GPIOC |
| gpio_cw32l010_v1 | GPIO | L010: GPIOA |
| gpiob_cw32l010_v1 | GPIOB | L010: GPIOB |
| gpio_cw32l011_v1 | GPIO | L011: GPIOA, GPIOB, GPIOC |
| gpio_cw32l052_v1 | GPIO | L052: GPIOA, GPIOB, GPIOC, GPIOD, GPIOF |

The F002/F003 shared source was checked against both families' own manuals and
SVDs. Every derived bank is covered. There are 16 corrected family/bank
combinations across 18 generated chip records, including package selections
and aliases (F002: 3; F003: 4; L010: 4; L011: 3; L052: 4).

## Primary evidence

| Own-family manual | Section | Printed page | PDF page |
| --- | --- | ---: | ---: |
| [F002 CN V1.4](https://www.whxy.com/uploads/files/20240920/CW32F002_UserManual_CN_V1.4.pdf) | 8.6.11 | 112 | 113 |
| [F003 CN V2.3](https://www.whxy.com/uploads/files/20240920/CW32F003_UserManual_CN_V2.3.pdf) | 8.6.11 | 114 | 115 |
| [L010 CN V1.2](https://www.whxy.com/uploads/files/20260626/CW32L010_UserManual_CN_V1.2.pdf) | 8.6.9 | 128 | 129 |
| [L011 CN V1.1](https://www.whxy.com/uploads/files/20260602/CW32L011_UserManual_CN_V1.1.pdf) | 8.6.9 | 128 | 129 |
| [L052 CN V1.5](https://www.whxy.com/uploads/files/20240920/CW32L052_UserManual_CN_V1.5.pdf) | 9.6.12 | 154 | 155 |

The L011 correction was initially deferred. Its newly acquired current own
manual now explicitly marks GPIOx_ISR RO for x=A/B/C. The current 2026-06-02
copy's SHA-256 is b245887e9037caf267f739e25bd3c354c2579d23315fc06389e055f371159f4f.
The older 2025-09-19 copy is not substituted under this URL/hash. No adjacent
family or SDK read operation is used to infer permission.

[Machine-readable evidence](gpio-isr-access-corrections.json) records all PDF
hashes, original SVD hashes and bank lineage, plus exact before/after YAML hashes.
Normal curated generation asserts reviewed access; import mode requires exactly
the original ReadWrite permission before applying a correction.

No fields, widths, register offsets, reserved masks, reset values, IDR access or
ICR semantics change. In particular, the L011 ICR reserved-mask question is
separate. F030/A030/F020 prior GPIO corrections and already-RO GPIO ISR entries
in L012/L031/R031/W031/L083 remain unchanged.

## Verification

```sh
python3 tests/check_gpio_isr_access.py --manual-dir /path/to/acquired/manuals
```

Use `--source-only` before regeneration. After coordinated regeneration, all
five exact manual hashes, eight overrides, six access-only snapshots, original
SVD inheritance and 18 affected chip records pass. Five positive Cargo controls
permit all ISR reads plus ordinary DIR writes/modifies. Thirty-two independent
negative checks reject ISR write/modify with exactly E0599 at the intended call.
No firmware or MMIO is executed.

Current log: [GPIO ISR proof](verification-logs/isr-access/gpio-access.log).
The evidence file records this run and retains the earlier four-family run as
historical verification.
