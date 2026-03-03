# FAQ

## Is this crate `no_std`?

Yes. By default it uses only `core`.

For host-side tests or nicer debug formatting you can enable `std`:

- `cargo test --features std`

## Does it allocate?

No. Storage is a fixed-size array inside the slab itself.

## Can I store non-`Copy` types?

Yes. `T` can be any type (including types with destructors). The slab drops live values on
`remove`, `clear`, and `Drop`.

## Is the API safe?

Yes. Internally it uses a small amount of `unsafe` to manage `MaybeUninit<T>`; all unsafe blocks
are documented. See `docs/SAFETY.md`.

## Is it thread-safe?

It has no internal locking. Whether it is safe to share between threads depends on `T` and on
how you synchronize access. In kernels, wrap it in your lock or use per-core slabs.

## What happens on generation overflow?

Generations use wrapping arithmetic. If a generation wraps to 0, it is normalized to 1.

In practice, reaching the wrap-around would require freeing the same slot \(2^{32}\) times.

