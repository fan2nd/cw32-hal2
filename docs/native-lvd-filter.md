# Native LVD SYSCLK filtering

`new_supply_filtered` and `new_pin_filtered` accept `DigitalFilter`, with fifteen
documented divider/consecutive-sample combinations on L010/L011/L012. Existing unfiltered
constructors remain unchanged. Clock initialization and idle LVD admission are
required. FLTTIME and FLTCLK=SYSCLK are written/read back while disabled, before
the existing source/threshold/enable sequence. No live filter change is exposed.

The pinned L012 CN1.4 manual defines consecutive-equal-sample filtering in
§28.3.3 p637, initialization in §28.5 p639, FLTCLK in §28.7.1 p641 and the full
FLTTIME table in §28.7.2 p642 (PDF pages printed +26). Its SHA-256 remains
`a9e54694a26f03c1f3e3041f40844900168ca2b8e6142f3328671b92e6b7a340`.
L010 CN1.2 independently specifies the same filter in §22.3.3 p542,
initialization p544, FLTCLK p546 and FLTTIME p547. L011 CN1.1 gives these
at p544, p546, p548 and p549. Their PDF pages are printed +1; pinned hashes
are in `sources/evidence-sources.json`. For example, code15 samples SYSCLK/32
and requires eight equal values on each family.

Readback failure requests EN=0, removes the filter and restores the inherited
clock selector. Normal Drop retires any edge wait, disables EN, removes the
owned filter and restores that selector. No reset/level action, timer break,
shared analog state or new clock request is configured. `sample` reads filtered
FLTV; existing edge waits change only their own selectors/IE and preserve filter
configuration. No additional event is acknowledged by filter construction.

SYSCLK must run continuously. No LSI source, low-power wake, fail-safe reset,
startup-output state, analog readiness or rejection duration in seconds is
qualified. Existing board-voltage requirements and L012 Vcore=1.6 V threshold assumptions
remain. All three families support the existing typed edge wait.
The enum, constructors and example are source-reviewed but uncompiled/untested;
readback failure, cleanup, edge coexistence and hardware acceptance are pending.

L010/L011 edge-wait qualification: each own §22.4 (printed p543/p545) defines
ACTION=0, IE and voltage RISE/FALL polarity; §22.7.3 (p548/p550) defines sole
W0C INTF, read-only FLTV and reserved zeros. Own §5.4 (p88/p86) and approved
CMSIS headers both assign LVD IRQ1. The L012 wait/retirement algorithm is reused
unchanged. This is event coalescing, not a crossing count or analog readiness.
L010/L011 IRQ acceptance remains pending.
