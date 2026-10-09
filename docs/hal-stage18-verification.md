# Stage18: FLASH and RTC family breadth

Reserved-region FLASH storage now covers all 13 families and 37 qualified exact parts. It uses direct typed PAC operations, generated limits and central RCC management. Generic/legacy aliases do not gain an invented storage capacity. L01x excludes the SLIB descriptor page and checks active SDK regions; L083 supports all 64 lock groups. Only ReadNorFlash is implemented; inherent write/erase require an unsafe exclusive reserved region and sustained electrical conditions. Post-trigger waiting is intentionally unbounded.

Blocking RTC calendars now cover all 11 RTC families; F002/F003 have no RTC. Classic families preserve and verify already-installed factory LSI trim rather than retuning shared clocks. Their nominal calendar rate is 32800/32768 with the own oscillator tolerance, not exact 1 Hz. L010/L011/L012 use qualified HSIOSC. W031 generic operation uses the conservative 2.0 V floor covering either board power mode. Alarms, async, low-power retention and precision compensation remain absent.

Combined verification passed 108 ARM library builds, 37 FLASH firmware links, 108 RTC firmware links and the full 54-selection data/PAC checks. No HAL tests were added and no firmware was executed. The strict runtime wrapper recorded a source-index-only mismatch; its original result is preserved and a separate exact compile-input acceptance receipt proves all executable inputs unchanged. Historical data-proof pins were narrowly refreshed, preserving previous hashes, before resuming incomplete checks.

The first RTC link command duplicated -Tlink.x already emitted by its build script. The corrected standalone link run passed all 108 firmware binaries; the failed command remains in the records.
