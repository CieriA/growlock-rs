# GrowLock

This library provides the `GrowLock<T>` type: a contiguous array type that
can have simultaneously any number of readers **and** one writer.

This is possible because after an element is pushed onto the `GrowLock`,
it can no longer be modified nor removed. The only way the writer can modify
the `GrowLock` is by pushing an element at the end of it.

If you want to modify elements of the array and/or have a dynamical capacity,
you should use `RwLock<Vec<T>>` instead.

# Examples
```rust
use growlock::grow_lock;

fn main() {
    let lock = grow_lock!(10, [1, 2, 3]);

    // you can read directly from the lock.
    let r1 = lock[0];
    let r2 = lock[1];
    assert_eq!(r1, 1);
    assert_eq!(r2, 2);

    // only one write lock may be held
    let mut w = lock.write().unwrap();
    w.push(4);

    // we can still read, however
    let r3 = lock[2];
    assert_eq!(r3, 3);

    // even while pushing
    w.push(5);
}
```

You can find more examples in the [examples directory](examples).

# Usage

This library is not yet available on [`crates.io`][crates.io].

If you want to use it anyway, clone it inside your project using
```bash
git clone https://github.com/CieriA/growlock-rs <PATH>
```
And then add in your `Cargo.toml`
```toml
[dependencies]
growlock = { path = "<PATH>", version = "0.1.0" }
```
(Substituting `<PATH>` with the path where you stored this library).

[crates.io]: https://crates.io

# License

This project is licensed under the [MIT license](LICENSE).

## Contributing

Unless you explicitly state otherwise, any contribution intentionally
submitted for inclusion in GrowLock by you shall be licensed as MIT, without
any additional terms or conditions.
