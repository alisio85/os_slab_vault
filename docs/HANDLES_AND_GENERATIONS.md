# Handles and Generations (Deep Dive)

This document explains generational handles in a kernel-friendly way.

## 1. Handle structure

A handle is a small value passed around by code:

- stored in queues
- returned from syscalls
- placed in capability tables
- embedded in other objects

`os_slab_vault::Key` is a handle consisting of:

- `index`: which slot in the slab
- `generation`: a counter that changes each time that slot is freed

Together they form an identity that is stable while the object is alive, and becomes invalid
after the object is destroyed.

## 2. The stale-handle problem (step-by-step)

Consider a task table:

1. Task A is inserted in slot 5 → you give user space handle `5`
2. Task A exits → slot 5 becomes free
3. Task B starts → slot 5 is reused
4. User space still holds handle `5` and tries to operate on it

If the kernel interprets the handle as a raw index, user space can now target Task B.

With generation checking:

1. Task A inserted → `(index=5, gen=17)`
2. Task A removed → slot generation bumps to 18
3. Task B inserted → `(index=5, gen=18)`
4. Old handle `(5,17)` no longer matches and is rejected

## 3. Why generation is per-slot

The generation counter belongs to the slot, not the object, because:

- the slot is the "identity namespace" being reused
- on reuse you want a cheap way to invalidate previous identities

## 4. Why generation starts at 1

In low-level environments it's common to see:

- zeroed memory after BSS init
- zeroed structs for “default” state

If a handle is accidentally read from a zeroed location, it would be `(0,0)`. By avoiding
generation `0`, we reduce the chance of an accidental match.

## 5. Wrap-around considerations

Generations are `u32` and wrap on overflow.

The wrap-around point requires \(2^{32}\) frees of the *same* slot, which is unrealistic for most
systems. Still, the implementation normalizes generation `0` back to `1` so that `0` remains
reserved.


