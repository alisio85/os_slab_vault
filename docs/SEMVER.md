# SemVer policy

This crate follows semantic versioning:

- **Patch**: bug fixes, internal refactors, documentation, no API changes.
- **Minor**: backward-compatible API additions.
- **Major**: breaking changes.

## What counts as the public API?

- All items reachable from the crate root (`os_slab_vault::...`) that are not explicitly marked
  as private implementation details.

## MSRV and Rust edition

The crate is Rust 2024 edition.

MSRV is not strictly pinned in this repository, but CI is expected to run on a recent stable
toolchain. If you need a strict MSRV, pin it at the workspace level and run CI with that version.

