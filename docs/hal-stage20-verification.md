# Stage20: UART flow control and timer inputs

Adds blocking/async RTS+CTS, TX+CTS and RX+RTS constructors using generated package-qualified pin roles. Pins follow their TX/RX half through splitting and drop; existing shared interrupt ownership and typed flag handling are retained. CTS prevents a new frame from starting while an active frame finishes. RTS reflects the receive register, not a software FIFO; this does not guarantee lossless reception.

Adds owned polling input capture and quadrature encoders on buffered GTIM/ATIM instances in L010/L011/L012. Flags and counter snapshots are explicitly coalesced/non-atomic. All instances and pins remain subject to package qualification. When Embassy time reserves GTIM1, the coexistence firmware uses ATIM; the SOP16 L010 package supports channel3 capture but has no qualified ATIM encoder pair.

Combined verification passed 108 ordinary ARM builds, 20 builds with the reserved time driver, and 63 real firmware links with zero warnings. The 869 generated data/PAC files remain byte-identical to Stage19. Source audits verified the timer maps and 877 UART package/alias route projections. Eight newly cited SDK members were pinned and recovered through five existing official archives; 213 acquisition outputs passed offline verification. No HAL tests or firmware execution occurred.
