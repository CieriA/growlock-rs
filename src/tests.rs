use {
    crate::{AtomicVec, cap::Cap},
    std::alloc::System,
};
// > NOTE: using wildcard allows `miri` to tell the exact line
// > that causes UB. This is because the [`AtomicVec`] is
// > instantly dropped.

// ------------------- constructors -------------------

/// Tests constructors and [`AtomicVec::drop`] with different kind of types and
/// capacities.
#[test]
fn new_empty_drop_primitive() {
    let _ = AtomicVec::<u32>::try_with_capacity(0);
    let _ = AtomicVec::<char>::with_capacity(1 << 20);
    let _ = AtomicVec::<(i64, *mut char)>::with_capacity(12);
    let _ = AtomicVec::<bool, _>::with_capacity_in(5, System);
    let _ = AtomicVec::<[i8; 12], _>::try_with_capacity_in(23, System);
}

/// Tests constructors and [`AtomicVec::drop`] with more complicated types
#[test]
fn new_empty_drop_heap() {
    use std::{collections::HashMap, rc::Rc, sync::Arc};

    let _ = AtomicVec::<String>::try_with_capacity(0);
    let _ = AtomicVec::<Vec<u16>>::with_capacity(3);
    let _ = AtomicVec::<HashMap<u32, &'static str>>::with_capacity(1 << 30);
    let _ = AtomicVec::<Arc<u64>>::with_capacity(46);
    let _ = AtomicVec::<Rc<i64>>::with_capacity(46);
}

/// Tests constructors and [`AtomicVec::drop`] with ZSTs
///
/// > NOTE: capacity is automatically set as 0 for ZSTs
#[test]
fn new_empty_drop_zst() {
    struct MyZST;
    let _ = AtomicVec::<()>::with_capacity(0);
    let _ = AtomicVec::<MyZST>::try_with_capacity(1 << 60);
    let _ =
        AtomicVec::<(), _>::try_with_capacity_in(isize::MAX as usize, System);
    let v = AtomicVec::<MyZST, _>::with_capacity_in(usize::MAX, System);
    assert_eq!(v.capacity(), usize::MAX);
    assert_eq!(v.buf.raw_cap(), Cap::ZERO);
}

// ------------------- push panics -------------------
#[test]
#[should_panic(expected = "length overflow")]
fn push_overflow() {
    let vec = AtomicVec::with_capacity(5);
    let mut guard = vec.lock().unwrap();
    for i in 0..6 {
        guard.push(i);
    }
}

#[test]
fn init_drop_on_panic() {
    use std::{
        panic,
        sync::atomic::{AtomicUsize, Ordering},
    };
    struct AddOnDrop<'a>(&'a AtomicUsize);
    impl Drop for AddOnDrop<'_> {
        fn drop(&mut self) {
            self.0.fetch_add(1, Ordering::Relaxed);
        }
    }

    let counter = AtomicUsize::new(0);
    let result = panic::catch_unwind(|| {
        let vec = AtomicVec::with_capacity(10);
        let mut guard = vec.lock().unwrap();
        for _ in 0..15 {
            guard.push(AddOnDrop(&counter));
        }
    });

    assert!(result.is_err());
    // 10 elements are pushed in the vec, the last is dropped when trying to
    // push it.
    assert_eq!(counter.load(Ordering::Relaxed), 11);
}

// ------------------- test drop -------------------

#[test]
fn initialized_drop() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    struct AddOnDrop<'a>(&'a AtomicUsize);
    impl Drop for AddOnDrop<'_> {
        fn drop(&mut self) {
            self.0.fetch_add(1, Ordering::Relaxed);
        }
    }

    let counter = AtomicUsize::new(0);
    {
        let vec = AtomicVec::with_capacity(200);
        let mut guard = vec.lock().unwrap();
        for _ in 0..100 {
            guard.push(AddOnDrop);
        }
        // here `vec` is dropped
    }
    assert_eq!(counter.load(Ordering::Relaxed), 100);
}

// ------------------- write -------------------

#[test]
fn write_contention() {
    use std::{sync::Arc, thread};
    const THREADS: usize = 10;
    const CAP: usize = 1000;

    let vec = Arc::new(AtomicVec::with_capacity(CAP));
    let mut handles = Vec::with_capacity(THREADS);
    for t in 0..THREADS {
        let v = Arc::clone(&vec);
        handles.push(thread::spawn(move || {
            for i in 0..(CAP / THREADS) {
                let mut guard = v.lock().unwrap();
                guard.push(t * (CAP / THREADS) + i);
            }
        }));
    }
    for handle in handles {
        handle.join().unwrap();
    }

    assert_eq!(vec.len(), CAP);
}

// ------------------- read -------------------

#[test]
fn read_while_locked() {
    let vec = AtomicVec::with_capacity(5);
    {
        let mut guard = vec.lock().unwrap();
        guard.push("hi");
        guard.push("there");
        assert_eq!(&vec[0..2], ["hi", "there"]);
        guard.push("still locked");
    }
    assert_eq!(vec.len(), 3);
}
