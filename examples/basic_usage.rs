use growlock::grow_lock;

fn main() {
    // initialize the lock with a capacity of 10 and elements `1, 2, 3`
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
