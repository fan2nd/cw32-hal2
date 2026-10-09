# Peripheral helper organization

The pinned Embassy STM32 UART implementation keeps private configuration and baud helpers in its USART module. It does not have a separate config.rs. This project now follows that organization: configuration and the CW32-specific shared-vector implementation live in one flat usart.rs, alongside its public constructor and configuration API. Public APIs and helper bodies are preserved.

The two ADC sequence clock helpers also live with their owning sequence implementation, instead of an extra registers.rs file that contained no independent register interface. The triggered mode remains separate because it exposes a distinct ownership and cancellation API.

This is not a rule that every free function must become a method. Pure calculations and module-private helpers remain where they improve the implementation. Substantial waveform timing solvers, calendar types and genuine hardware register variants retain meaningful boundaries. Exact moved-body hashes are recorded in hal-helper-layout-refactor.json. No HAL tests were added.

CRC is likewise one flat crc.rs. Its existing Crc owner now performs PAC operations directly; the single register-holder backend and forwarding methods are removed. Fallible reset/clock checks still precede construction, reset still writes CR unconditionally, and native byte/halfword/word transactions retain their widths.
