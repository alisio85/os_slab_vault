# Manual: `os_slab_vault`

This manual is intentionally **ultra-detailed**. It targets OS developers who want a small,
auditable, allocation-free container for kernel objects, together with stable handles that
detect stale access.

> Crate: `os_slab_vault` (Rust 2024, dependency-free, `no_std`-first)

## 1. Why a generational slab in an OS?

### 1.1 The recurring kernel pattern

Many kernels have *tables of objects*:

- task/thread tables
- process tables
- file descriptor tables
- capability tables
- interrupt handler registries
- timer wheels / timer registries
- port / endpoint registries
- device instance registries

Those tables typically store objects in fixed-size arrays because:

- fixed memory usage is predictable
- the heap might be unavailable early in boot
- fragmentation or allocation latencies can be undesirable

### 1.2 The index-reuse bug class

If you identify objects solely by array index, you can easily end up with:

1. object A stored at index `7`
2. someone stores "handle = 7"
3. object A is destroyed, slot `7` is free again
4. new object B is allocated and reuses slot `7`
5. stale handle `7` now accidentally refers to object B

This is a *logic-level use-after-free* that can become a security issue.

### 1.3 Generations fix stale handles

With a generation counter:

- handle is `(index, generation)`
- on every free, generation increments
- a stale handle's generation won't match anymore

This makes it cheap to check validity on every access.

## 2. Crate overview

### 2.1 Main types

- `Slab<T, const N: usize>`: fixed-capacity storage for up to `N` items.
- `Key`: a small, `Copy` handle that carries `(index, generation)`.
- `InsertError`: returned when the slab is full.

### 2.2 `no_std` model and feature flags

By default the crate is `no_std`.

- `--no-default-features`: pure `core` usage
- `--features std`: enables only things that require `std` (currently: a nicer `Debug` impl and
  host-side tests)

The data structure and safety model are independent of `std`.

## 3. API guide

### 3.1 Creating a slab

Choose a capacity that matches your kernel’s static limits:

```rust
use os_slab_vault::Slab;

// A table that can store up to 256 kernel objects.
let mut table: Slab<u32, 256> = Slab::new();
```

### 3.2 Insertion

```rust
let key = table.insert(42).unwrap();
```

Insertion is O(1). It uses an internal free-list.

If the slab is full:

```rust
let mut s: Slab<u32, 1> = Slab::new();
let _ = s.insert(1).unwrap();
assert!(s.insert(2).is_err());
```

### 3.3 Access

```rust
if let Some(v) = table.get(key) {
    // v: &u32
}
```

`get` returns `None` if the key is:

- out-of-bounds
- points to a vacant slot
- stale (generation mismatch)

Mutable access:

```rust
if let Some(v) = table.get_mut(key) {
    *v += 1;
}
```

### 3.4 Removal

```rust
let value = table.remove(key);
```

On success:

- the value is returned
- the slot becomes free
- the slot generation increments
- the old key becomes stale

### 3.5 Clearing

```rust
table.clear();
```

This drops all live values and rebuilds internal state.

## 4. Internal model (for auditing)

### 4.1 Storage: `MaybeUninit<T>`

Slots are stored as `MaybeUninit<T>` so that:

- we do not require `T: Default`
- we can have an array of `N` slots without allocating

An `occupied[i]` boolean determines whether slot `i` is initialized.

### 4.2 Free-list

Vacant slots are linked in a singly-linked list:

- `free_head` is the first free slot
- `free_next[i]` is the next free slot after `i`
- `u32::MAX` represents “end of list / none”

This yields O(1) insertion into a free slot.

### 4.3 Generation counters and wrap-around

Each slot has a `generation[i]: u32`.

Rules:

- generation starts at **1** (not 0)
- on every successful removal, generation increments with wrapping arithmetic
- if it wraps to 0, it is bumped to 1 again

Reasoning:

- generation 0 is treated as “reserved” so that an all-zero memory key is unlikely to be valid

## 5. Safety model (how `unsafe` stays correct)

The crate has `unsafe` in exactly these categories:

1. writing a newly inserted value into `MaybeUninit<T>`
2. creating references to initialized values (`assume_init_ref`, `assume_init_mut`)
3. reading/dropping initialized values on remove/clear/drop

Key invariants:

- If `occupied[i] == true`, then `values[i]` is initialized.
- If `occupied[i] == false`, then `values[i]` must not be read/dropped (it is uninitialized).
- Every inserted value is dropped exactly once (via `remove`, `clear`, or `Drop`).

See `docs/SAFETY.md` for a line-by-line checklist.

## 6. OS integration patterns

### 6.1 Store the key in other structures

Example: a scheduler queue might store `Key`s rather than raw pointers.

This yields:

- predictable memory usage
- stale handle checks at the boundary

### 6.2 Concurrency

`Slab` is not synchronized.

In an SMP kernel, use:

- a per-core slab to avoid locking, or
- a lock around the slab (spinlock/mutex)

### 6.3 Deterministic failure handling

When `insert` fails with `InsertError`, you can:

- fail the operation (e.g. deny creating a new task)
- recycle entries (policy decision)
- increase capacity (compile-time change)

This matches kernel design where out-of-resources is a normal error case.

## 7. Testing

The crate provides host-side unit tests.

Suggested CI commands:

- `cargo fmt -- --check`
- `cargo build --no-default-features`
- `cargo build --features std`
- `cargo test --features std`
- `cargo clippy --all-targets --all-features -- -D warnings`
- `RUSTDOCFLAGS="-D warnings" cargo doc --no-deps --all-features`

## 8. Next steps

- Read `docs/COOKBOOK.md` for practical recipes.
- Read `docs/HANDLES_AND_GENERATIONS.md` if you want a deeper conceptual explanation.

