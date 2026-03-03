//! Slab implementation.
//!
//! This module intentionally keeps all implementation details private, exposing a small, safe
//! surface area suitable for kernel usage.

use core::fmt;
use core::hash::{Hash, Hasher};
use core::marker::PhantomData;
use core::mem::MaybeUninit;

/// A stable handle to an element stored inside a [`Slab`].
///
/// ## Why not use a raw index?
///
/// Many kernel data structures reuse indices when objects are destroyed and new objects are
/// created. If callers keep an old index, they might accidentally access a *different* object
/// that happens to be stored at the same index later (a form of use-after-free).
///
/// `Key` prevents that by carrying:
///
/// - **index**: which slot inside the slab
/// - **generation**: a counter that changes every time that slot is freed
///
/// A `Key` is valid only if both fields match the current slot state.
#[derive(Copy, Clone)]
pub struct Key {
    idx: u32,
    generation: u32,
}

impl Key {
    /// Creates a `Key` from raw parts.
    ///
    /// This is mainly useful for serialization / debugging. The caller is responsible for
    /// ensuring the parts come from the same slab instance.
    #[inline]
    pub const fn from_parts(index: u32, generation: u32) -> Self {
        Self {
            idx: index,
            generation,
        }
    }

    /// Returns the slot index.
    #[inline]
    pub const fn index(self) -> u32 {
        self.idx
    }

    /// Returns the generation counter.
    #[inline]
    pub const fn generation(self) -> u32 {
        self.generation
    }

    /// Returns the raw `(index, generation)` pair.
    #[inline]
    pub const fn into_parts(self) -> (u32, u32) {
        (self.idx, self.generation)
    }
}

impl fmt::Debug for Key {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Key")
            .field("idx", &self.idx)
            .field("generation", &self.generation)
            .finish()
    }
}

impl PartialEq for Key {
    fn eq(&self, other: &Self) -> bool {
        self.idx == other.idx && self.generation == other.generation
    }
}

impl Eq for Key {}

impl Hash for Key {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.idx.hash(state);
        self.generation.hash(state);
    }
}

/// Returned by [`Slab::insert`] when the slab is full.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct InsertError;

/// A fixed-capacity generational slab storing up to `N` values of type `T`.
///
/// ## `no_std` / allocation-free
///
/// `Slab` does not allocate and does not depend on `alloc`. Storage is a fixed-size array.
///
/// ## Complexity
///
/// - `insert`: O(1)
/// - `remove`: O(1)
/// - `get` / `get_mut`: O(1)
/// - iteration: O(N)
///
/// ## Concurrency
///
/// This type performs no internal locking. If multiple cores/threads can access it concurrently,
/// wrap it in your kernel synchronization primitive (spinlock, mutex, RwLock, etc.).
pub struct Slab<T, const N: usize> {
    // Values stored in-place. Only slots with `occupied[i] == true` contain initialized values.
    values: [MaybeUninit<T>; N],
    // Whether a slot is currently holding a live `T`.
    occupied: [bool; N],
    // Generation counter per slot; incremented on each successful removal.
    generation: [u32; N],
    // Singly-linked free-list using indices. Only meaningful for slots that are *not* occupied.
    free_next: [u32; N],
    // Head of the free-list, or u32::MAX if none.
    free_head: u32,
    // Number of occupied slots.
    len: usize,
}

impl<T, const N: usize> Slab<T, N> {
    /// Creates a new empty slab.
    ///
    /// `N` may be zero; a zero-capacity slab is always full and always empty at the same time.
    pub fn new() -> Self {
        // We build the free-list 0 -> 1 -> 2 -> ... -> N-1.
        // For N == 0 this will be an empty structure.
        let mut free_next = [0u32; N];
        let mut i = 0usize;
        while i < N {
            free_next[i] = if i + 1 < N { (i + 1) as u32 } else { u32::MAX };
            i += 1;
        }

        Self {
            values: core::array::from_fn(|_| MaybeUninit::uninit()),
            occupied: [false; N],
            generation: [1u32; N],
            free_next,
            free_head: if N == 0 { u32::MAX } else { 0 },
            len: 0,
        }
    }

    /// Returns the maximum number of elements the slab can hold.
    #[inline]
    pub const fn capacity(&self) -> usize {
        N
    }

    /// Returns the number of live elements currently stored.
    #[inline]
    pub const fn len(&self) -> usize {
        self.len
    }

    /// Returns `true` if the slab holds no elements.
    #[inline]
    pub const fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Returns `true` if the slab cannot accept more inserts.
    #[inline]
    pub const fn is_full(&self) -> bool {
        self.len == N
    }

    /// Inserts a new value and returns a fresh generation-checked [`Key`].
    ///
    /// ## Errors
    ///
    /// Returns [`InsertError`] if the slab is full.
    #[inline]
    pub fn insert(&mut self, value: T) -> Result<Key, InsertError> {
        let idx = self.pop_free().ok_or(InsertError)?;
        let i = idx as usize;

        debug_assert!(!self.occupied[i]);

        // SAFETY:
        // - `i` is within bounds.
        // - Slot `i` is currently marked as vacant (`occupied[i] == false`),
        //   so `values[i]` is uninitialized and can be written exactly once.
        unsafe { self.values[i].as_mut_ptr().write(value) };

        self.occupied[i] = true;
        self.len += 1;

        Ok(Key::from_parts(idx, self.generation[i]))
    }

    /// Returns an immutable reference to the value identified by `key`, or `None` if:
    ///
    /// - The index is out of bounds, or
    /// - The slot is vacant, or
    /// - The slot's generation does not match (stale key)
    #[inline]
    pub fn get(&self, key: Key) -> Option<&T> {
        let i = key.idx as usize;
        if i >= N {
            return None;
        }
        if !self.occupied[i] {
            return None;
        }
        if self.generation[i] != key.generation {
            return None;
        }

        // SAFETY:
        // - `i` in bounds.
        // - `occupied[i] == true` implies `values[i]` contains an initialized `T`.
        // - We return an immutable reference tied to `&self`.
        Some(unsafe { self.values[i].assume_init_ref() })
    }

    /// Returns a mutable reference to the value identified by `key`, or `None` if the key is
    /// invalid or stale. See [`Slab::get`] for the validity rules.
    #[inline]
    pub fn get_mut(&mut self, key: Key) -> Option<&mut T> {
        let i = key.idx as usize;
        if i >= N {
            return None;
        }
        if !self.occupied[i] {
            return None;
        }
        if self.generation[i] != key.generation {
            return None;
        }

        // SAFETY:
        // - Same as `get`, but we return a mutable reference tied to `&mut self`.
        Some(unsafe { self.values[i].assume_init_mut() })
    }

    /// Returns `true` if `key` currently refers to a live element.
    #[inline]
    pub fn contains_key(&self, key: Key) -> bool {
        self.get(key).is_some()
    }

    /// Removes and returns the value identified by `key`.
    ///
    /// If `key` is stale or invalid, returns `None` and leaves the slab unchanged.
    ///
    /// On success, the slot generation is incremented so that the key becomes stale.
    pub fn remove(&mut self, key: Key) -> Option<T> {
        let i = key.idx as usize;
        if i >= N {
            return None;
        }
        if !self.occupied[i] {
            return None;
        }
        if self.generation[i] != key.generation {
            return None;
        }

        self.occupied[i] = false;
        self.len -= 1;

        self.bump_generation(i);
        self.push_free(key.idx);

        // SAFETY:
        // - Slot `i` was occupied up to this point and contained an initialized `T`.
        // - We just marked it vacant and will not read it again without re-initializing.
        Some(unsafe { self.values[i].assume_init_read() })
    }

    /// Removes all elements from the slab.
    ///
    /// This runs in O(N) and drops all contained values.
    pub fn clear(&mut self) {
        // Drop values in-place and rebuild the free-list from scratch.
        let mut i = 0usize;
        while i < N {
            if self.occupied[i] {
                // SAFETY:
                // - Slot is occupied, therefore initialized.
                // - We are clearing; we will not access the value again.
                unsafe { self.values[i].assume_init_drop() };
                self.occupied[i] = false;
                self.bump_generation(i);
            }
            i += 1;
        }

        self.len = 0;

        // Rebuild free-list: 0 -> 1 -> ... -> N-1.
        let mut j = 0usize;
        while j < N {
            self.free_next[j] = if j + 1 < N { (j + 1) as u32 } else { u32::MAX };
            j += 1;
        }
        self.free_head = if N == 0 { u32::MAX } else { 0 };
    }

    /// Returns an iterator over `(Key, &T)` pairs for all live elements.
    #[inline]
    pub fn iter(&self) -> Iter<'_, T, N> {
        Iter { slab: self, i: 0 }
    }

    /// Returns an iterator over `(Key, &mut T)` pairs for all live elements.
    #[inline]
    pub fn iter_mut(&mut self) -> IterMut<'_, T, N> {
        IterMut {
            slab: self as *mut Slab<T, N>,
            i: 0,
            _pd: PhantomData,
        }
    }

    #[inline]
    fn pop_free(&mut self) -> Option<u32> {
        if self.free_head == u32::MAX {
            return None;
        }
        let idx = self.free_head;
        let next = self.free_next[idx as usize];
        self.free_head = next;
        Some(idx)
    }

    #[inline]
    fn push_free(&mut self, idx: u32) {
        let i = idx as usize;
        self.free_next[i] = self.free_head;
        self.free_head = idx;
    }

    #[inline]
    fn bump_generation(&mut self, i: usize) {
        // Generation 0 is treated as "reserved" to make it easier to spot bugs when a Key is
        // default-initialized from zeroed memory (common in kernels).
        let mut g = self.generation[i].wrapping_add(1);
        if g == 0 {
            g = 1;
        }
        self.generation[i] = g;
    }
}

impl<T, const N: usize> Default for Slab<T, N> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T, const N: usize> Drop for Slab<T, N> {
    fn drop(&mut self) {
        // Ensure all live values are dropped.
        let mut i = 0usize;
        while i < N {
            if self.occupied[i] {
                // SAFETY:
                // - Slot is occupied, therefore initialized.
                // - We are in Drop; value must be dropped exactly once.
                unsafe { self.values[i].assume_init_drop() };
                self.occupied[i] = false;
            }
            i += 1;
        }
    }
}

/// Immutable iterator over live slab elements.
pub struct Iter<'a, T, const N: usize> {
    slab: &'a Slab<T, N>,
    i: usize,
}

impl<'a, T, const N: usize> Iterator for Iter<'a, T, N> {
    type Item = (Key, &'a T);

    fn next(&mut self) -> Option<Self::Item> {
        while self.i < N {
            let i = self.i;
            self.i += 1;

            if self.slab.occupied[i] {
                let k = Key::from_parts(i as u32, self.slab.generation[i]);
                // SAFETY: occupied implies initialized.
                let v = unsafe { self.slab.values[i].assume_init_ref() };
                return Some((k, v));
            }
        }
        None
    }
}

/// Mutable iterator over live slab elements.
pub struct IterMut<'a, T, const N: usize> {
    slab: *mut Slab<T, N>,
    i: usize,
    _pd: PhantomData<&'a mut Slab<T, N>>,
}

impl<'a, T, const N: usize> Iterator for IterMut<'a, T, N> {
    type Item = (Key, &'a mut T);

    fn next(&mut self) -> Option<Self::Item> {
        while self.i < N {
            let i = self.i;
            self.i += 1;

            // SAFETY:
            // - `self.slab` comes from `Slab::iter_mut(&mut self)` and is valid for `'a`.
            // - The iterator never outlives the borrow of the original slab.
            // - We only create a temporary `&mut Slab` here and never let it escape.
            let slab = unsafe { &mut *self.slab };

            if slab.occupied[i] {
                let k = Key::from_parts(i as u32, slab.generation[i]);

                // SAFETY:
                // - The iterator yields each index at most once, so we never create two `&mut T`
                //   pointing to the same slot.
                // - We have exclusive access to the slab for the lifetime `'a`.
                // - `occupied[i] == true` implies the slot is initialized.
                let v = unsafe { slab.values[i].assume_init_mut() };
                return Some((k, v));
            }
        }
        None
    }
}

#[cfg(feature = "std")]
impl<T: fmt::Debug, const N: usize> fmt::Debug for Slab<T, N> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut ds = f.debug_struct("Slab");
        ds.field("len", &self.len).field("capacity", &N);
        ds.field("items", &DebugItems { slab: self });
        ds.finish()
    }
}

#[cfg(feature = "std")]
struct DebugItems<'a, T, const N: usize> {
    slab: &'a Slab<T, N>,
}

#[cfg(feature = "std")]
impl<'a, T: fmt::Debug, const N: usize> fmt::Debug for DebugItems<'a, T, N> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut list = f.debug_list();
        for (k, v) in self.slab.iter() {
            list.entry(&(k, v));
        }
        list.finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn insert_get_remove_roundtrip() {
        let mut s: Slab<&'static str, 2> = Slab::new();
        assert!(s.is_empty());
        assert!(!s.is_full());

        let a = s.insert("a").unwrap();
        let b = s.insert("b").unwrap();
        assert!(s.is_full());
        assert_eq!(s.len(), 2);

        assert_eq!(s.get(a), Some(&"a"));
        assert_eq!(s.get(b), Some(&"b"));

        let out = s.remove(a).unwrap();
        assert_eq!(out, "a");
        assert!(s.get(a).is_none());
        assert_eq!(s.len(), 1);

        // Slot reuse must create a different generation.
        let a2 = s.insert("a2").unwrap();
        assert_ne!(a.into_parts(), a2.into_parts());
        assert!(s.get(a).is_none());
        assert_eq!(s.get(a2), Some(&"a2"));
    }

    #[test]
    fn full_returns_error() {
        let mut s: Slab<u32, 1> = Slab::new();
        let _ = s.insert(10).unwrap();
        assert_eq!(s.insert(11), Err(InsertError));
    }

    #[test]
    fn clear_drops_and_resets() {
        let mut s: Slab<String, 4> = Slab::new();
        let _ = s.insert("x".to_string()).unwrap();
        let _ = s.insert("y".to_string()).unwrap();
        s.clear();
        assert_eq!(s.len(), 0);
        assert!(s.insert("z".to_string()).is_ok());
    }

    #[test]
    fn iter_yields_all_live() {
        let mut s: Slab<u32, 4> = Slab::new();
        let k1 = s.insert(1).unwrap();
        let k2 = s.insert(2).unwrap();
        let k3 = s.insert(3).unwrap();
        let _ = s.remove(k1).unwrap();

        let mut keys = s.iter().map(|(k, _)| k).collect::<Vec<_>>();
        keys.sort_by_key(|k| k.index());
        assert_eq!(keys, vec![k2, k3]);
    }

    #[test]
    fn iter_mut_can_modify() {
        let mut s: Slab<u32, 3> = Slab::new();
        let _ = s.insert(1).unwrap();
        let _ = s.insert(10).unwrap();

        for (_k, v) in s.iter_mut() {
            *v += 1;
        }

        let values = s.iter().map(|(_, v)| *v).collect::<Vec<_>>();
        assert!(values.contains(&2));
        assert!(values.contains(&11));
    }
}
