# Stage17: owned DMA and AUTOTRIM counter

Adds an x030 owned static-buffer DMA copy path with an explicit unsafe constructor, and L052/L083 AUTOTRIM polling counters using MD=3/AUTO=0 and qualified raw HSIOSC bounds. Both production files match independently accepted snapshots.

Final scoped verification passed 44 affected ARM builds, nine other-family checks and nine real firmware links, with zero warnings. Full 54-selection data/PAC regeneration and metadata checks passed. No firmware was executed and no HAL test harness was added.

DMA ownership does not make construction unconditionally safe: entry quiescence and ongoing hardware exclusivity are caller obligations. Clean documented completion returns buffers/channel; errors reserve resources, forgetting leaks them and Drop may hang. There is no early-abort or safe borrowed-buffer/peripheral DMA wrapper.

AUTOTRIM calibration, undocumented FCAP measurement protocols, external references and low-power operation remain outside this counter API. The release also adds selective ICR command seeds and corrects three L083 result/limit registers to read-only.
