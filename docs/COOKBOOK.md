# Cookbook

Practical patterns for using `os_slab_vault` in OS code.

## 1) Task table with stable IDs

```rust
use os_slab_vault::{Key, Slab};

#[derive(Debug)]
struct Task {
    state: u32,
}

type TaskTable = Slab<Task, 1024>;

fn create_task(tasks: &mut TaskTable) -> Result<Key, ()> {
    tasks.insert(Task { state: 0 }).map_err(|_| ())
}
```

## 2) Validate handles at syscall boundary

```rust
fn syscall_set_state(tasks: &mut Slab<u32, 64>, task: Key, new_state: u32) -> Result<(), ()> {
    let slot = tasks.get_mut(task).ok_or(())?;
    *slot = new_state;
    Ok(())
}
```

## 3) Use `Key` as a map key

`Key` implements `Hash`, `Eq`, and is small and `Copy`.

In a `no_std` kernel you might have your own hash table implementation; the handle type is ready.

## 4) Iterate live objects

```rust
use os_slab_vault::Slab;

let mut s: Slab<u32, 4> = Slab::new();
let _ = s.insert(1).unwrap();
let _ = s.insert(2).unwrap();

for (k, v) in s.iter() {
    let _idx = k.index();
    let _val = *v;
}
```

