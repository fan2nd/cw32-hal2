# CW32W031 target retirement

CW32W031 and CW32W031R8U6 are no longer supported build targets. Their HAL,
PAC and example Cargo features are removed, rather than aliased to CW32L031.
CW32L031 and CW32R031 remain supported with their existing independently
qualified package pins, memory and electrical limits.

The vendor identifies CW32W031 as a SiP combining **CW32L031 + PAN3028**:
[Wuhan Xinyuan product announcement](https://www.whxy.com/gongsixinwen/chixushengengwulianwanglingyuwuhanxinyuanbandaotifabuSub1GxiliechanpinCW32W031.html).
This establishes the MCU implementation relationship, not a pin-compatible or
electrically interchangeable standalone package. An existing W031 board must
not be built as L031 on that basis. Select an actual supported L031 part only
when the board uses that part and satisfies its package-specific requirements.

Vendor catalog entries, source-bound facts and historical qualification/review
receipts remain evidence of past work. They are not the current supported-target
roster and are not rewritten to pretend W031 was never validated. Current
support is the retained roster minus exactly CW32W031 and CW32W031R8U6.
The private RCC backend directory still has the historical name
`l031_r031_w031`; that name is not an exposed device feature.

This retirement does not add RF support, broaden L031 voltage or pin admission,
or establish any new hardware validation. Low-power and RF work remain deferred.

The two JSON files in `docs/retired-w031/` are byte-preserved historical
source-audit snapshots, not active generated metadata, PAC targets or aliases.
Source checks validate their recorded hashes while independently requiring both
retired names to be absent from the 52-target live output. The exact retirement
authority is `cw32-data-serde/src/retired-targets.txt`, packaged with the shared
admission implementation rather than inferred from missing files.
