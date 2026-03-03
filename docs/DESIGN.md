# Design Rationale

## Why fixed capacity?

In kernel / bare-metal settings you often want:

- deterministic memory usage
- compile-time memory budgeting
- zero dependency on a global allocator

Fixed capacity matches those constraints. If you need growth, that is a higher-level policy and
should be implemented using your kernel’s allocation strategy, not inside this container.

## Why `Key` carries `(index, generation)`?

This is the simplest robust pattern that:

- avoids pointer exposure
- avoids accidental slot reuse bugs
- keeps handles small and `Copy`

It is also easy to serialize and to log for diagnostics.

## Why generation starts at 1?

Many kernels or boot code paths involve zeroed memory. If a `Key` is accidentally read from a
zeroed structure, `(idx=0, gen=0)` is unlikely to match a real entry, reducing bug impact.

## Why a free-list instead of linear scan?

Linear scan insertion has O(N) worst case and can create unpredictable latency spikes.

Free-list insertion is O(1) and stable.

## Why not include synchronization?

Locking strategies differ across kernels:

- global lock
- per-table lock
- per-core ownership
- lock-free structures

Embedding one choice inside the container would force policy on users. This crate aims to be a
small primitive, so it stays lock-free and expects the kernel to provide synchronization.

