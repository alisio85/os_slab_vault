# Safety Notes (Auditing Guide)

This document explains the internal `unsafe` usage in `os_slab_vault` and the invariants that
make it sound. The public API is safe.

## 1. Core invariants

For every slot `i` in `0..N`:

1. **Initialization invariant**
   - If `occupied[i] == true`, then `values[i]` contains a fully initialized `T`.
   - If `occupied[i] == false`, then `values[i]` is treated as uninitialized and must not be read
     or dropped.

2. **Drop invariant**
   - Every `T` written into a slot is dropped **exactly once**, either by:
     - `remove`
     - `clear`
     - `Drop for Slab`

3. **Key validity invariant**
   - A `Key { idx, gen }` is valid iff:
     - `idx < N`
     - `occupied[idx] == true`
     - `generation[idx] == gen`

## 2. Unsafe operations used

The implementation uses `MaybeUninit<T>` for storage. That requires unsafe operations in
well-defined places.

### 2.1 Insertion (write into an uninitialized slot)

Operation:

- `values[i].as_mut_ptr().write(value)`

Safety argument:

- we only do this after taking an index from the free-list
- the free-list contains only vacant slots
- we set `occupied[i] = true` after initializing

### 2.2 Immutable access (reference to initialized slot)

Operation:

- `values[i].assume_init_ref()`

Safety argument:

- access checks `occupied[i] == true` and generation match first

### 2.3 Mutable access (mutable reference)

Operation:

- `values[i].assume_init_mut()`

Safety argument:

- same checks as immutable access
- borrowing rules are enforced by Rust because the method requires `&mut self`

### 2.4 Removal (read initialized value)

Operation:

- `values[i].assume_init_read()`

Safety argument:

- same checks as access
- after reading we mark slot vacant, bump generation, and push it onto free-list
- the value is moved out exactly once

### 2.5 Clear / Drop (drop initialized value)

Operation:

- `values[i].assume_init_drop()`

Safety argument:

- performed only when `occupied[i] == true`
- slot is then marked vacant
- ensures a value is dropped exactly once

## 3. Free-list correctness

The free-list is a classic singly-linked list:

- `free_head` is `u32::MAX` when empty
- otherwise it holds an index in `0..N`
- `free_next[idx]` is the next element or `u32::MAX`

Correctness relies on:

- only vacant slots are ever pushed to the free-list
- insertion pops exactly one slot, making it no longer free

## 4. Generation wrap-around

Generations are `u32` and increment with wrapping arithmetic.

We treat generation `0` as reserved and normalize `0` back to `1`. This reduces the chance that a
zeroed `Key` becomes accidentally valid.

## 5. Auditing checklist

When reviewing changes to this crate:

- Ensure every path that sets `occupied[i] = true` also initializes `values[i]` exactly once.
- Ensure every path that sets `occupied[i] = false` also prevents later reads/drops.
- Ensure free-list operations preserve “only vacant slots are free”.
- Ensure generation bump happens on successful removals and on clear.

