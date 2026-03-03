//! # os_slab_vault
//!
//! A **dependency-free**, **`no_std`-first** generational slab / object pool.
//!
//! ## What problem does it solve?
//!
//! OS kernels frequently need to store "kernel objects" (tasks, threads, IPC endpoints, driver
//! instances, capabilities, timers, file descriptors…) in a way that is:
//!
//! - **Deterministic**: fixed upper bounds, predictable performance.
//! - **Allocation-free**: often the kernel cannot (or does not want to) rely on a heap.
//! - **Safe**: users of the structure should not accidentally dereference freed objects.
//!
//! `os_slab_vault` provides a fixed-capacity `Slab<T, N>` with **generation-checked handles**
//! (`Key`). Handles become **stale** when the referenced slot is freed and later reused,
//! preventing a classic "use-after-free via index reuse" bug.
//!
//! ## Design overview
//!
//! The slab stores up to `N` values of type `T`. Each slot has:
//!
//! - An **occupied bit**
//! - A **generation counter**
//! - Storage for `T` (using `MaybeUninit<T>` to avoid requiring `T: Default`)
//!
//! A **free-list** is maintained for O(1) insertion into the next vacant slot.
//!
//! ## Feature flags
//!
//! - `std`: enables `std` for host-side tests/examples. The library remains `no_std` at its core.
//!
//! ## Safety
//!
//! This crate uses a small amount of `unsafe` internally to manage `MaybeUninit<T>` safely.
//! Every `unsafe` block is documented with the invariants that make it correct.
//! The public API is safe.
//!
//! For a deeper dive, see the project docs in `docs/`, especially `docs/SAFETY.md`.

#![cfg_attr(not(feature = "std"), no_std)]
#![deny(missing_docs)]
#![forbid(unsafe_op_in_unsafe_fn)]

mod slab;

pub use slab::{InsertError, Key, Slab};
