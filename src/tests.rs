// > NOTE: using wildcard allows `miri` to tell the exact line
// > that causes UB. This is because the [`AtomicVec`] is
// > instantly dropped.

use {
    crate::{GrowLock, cap::Cap, grow_lock},
    std::{
        alloc::System,
        sync::{
            Arc,
            atomic::{AtomicUsize, Ordering},
        },
        thread,
        time::Duration,
    },
};

/// Helper struct
struct AddOnDrop<'a>(&'a AtomicUsize);
impl Drop for AddOnDrop<'_> {
    fn drop(&mut self) {
        self.0.fetch_add(1, Ordering::Relaxed);
    }
}

// ------------------- constructors -------------------

/// Tests constructors and [`GrowLock::drop`] with different kind of types and
/// capacities.
#[test]
fn new_empty_drop_primitive() {
    let _ = GrowLock::<u32>::try_with_capacity(0);
    let _ = GrowLock::<char>::with_capacity(1 << 20);
    let _ = GrowLock::<(i64, *mut char)>::with_capacity(12);
    let _ = GrowLock::<bool, _>::with_capacity_in(5, System);
    let _ = GrowLock::<[i8; 12], _>::try_with_capacity_in(23, System);
}

/// Tests constructors and [`GrowLock::drop`] with more complicated types
#[test]
fn new_empty_drop_heap() {
    use std::{collections::HashMap, rc::Rc, sync::Arc};

    let _ = GrowLock::<String>::try_with_capacity(0);
    let _ = GrowLock::<Vec<u16>>::with_capacity(3);
    let _ = GrowLock::<HashMap<u32, &'static str>>::with_capacity(1 << 30);
    let _ = GrowLock::<Arc<u64>>::with_capacity(46);
    let _ = GrowLock::<Rc<i64>>::with_capacity(46);
}

/// Tests constructors and [`GrowLock::drop`] with ZSTs
///
/// > NOTE: capacity is automatically set as 0 for ZSTs
#[test]
fn new_empty_drop_zst() {
    struct MyZST;
    let _ = GrowLock::<()>::with_capacity(0);
    let _ = GrowLock::<MyZST>::try_with_capacity(1 << 60);
    let _ =
        GrowLock::<(), _>::try_with_capacity_in(isize::MAX as usize, System);
    let v = GrowLock::<MyZST, _>::with_capacity_in(usize::MAX, System);
    assert_eq!(v.capacity(), usize::MAX);
    assert_eq!(v.buf.raw_cap(), Cap::ZERO);
}

#[test]
fn from_vec() {
    let vec = vec![1u32, 2, 3, 4, 5];
    let atomic_vec = GrowLock::from(vec);
    assert_eq!(&atomic_vec[..], &[1, 2, 3, 4, 5]);
}

// ------------------- macro init -------------------

#[test]
fn empty_macro() {
    let vec: GrowLock<String> = grow_lock![];

    assert_eq!(vec.as_slice(), &[] as &[String]);
    assert!(vec.is_empty());
    assert_eq!(vec.capacity(), 0);
    let mut guard = vec.write().unwrap();
    assert!(guard.try_push("hello world".to_owned()).is_err());

    assert_eq!(vec, GrowLock::<String>::with_capacity(0));
}
#[test]
fn array_macro() {
    let vec: GrowLock<char> = grow_lock!(10, ['a', 'b', 'c']);

    assert_eq!(&vec, &['a', 'b', 'c']);

    let mut guard = vec.write().unwrap();
    for _ in 0..7 {
        guard.push('_');
    }
    assert!(vec.is_full());
}
#[test]
fn repeat_macro() {
    let vec: GrowLock<String> = grow_lock!(15, ["hello".to_owned(); 4]);
    for str in &vec[..4] {
        assert_eq!(str, "hello");
    }
    let mut guard = vec.write().unwrap();
    for _ in 0..11 {
        guard.push("world".to_owned());
    }
    assert!(vec.is_full());
}

#[test]
fn array_full_macro() {
    let vec: GrowLock<char> = grow_lock!['a', 'b', 'c'];
    assert_eq!(&vec, &['a', 'b', 'c']);
    assert!(vec.is_full());
}

#[test]
fn repeat_full_macro() {
    let vec: GrowLock<String> = grow_lock!["hello".to_owned(); 4];
    for str in &vec[..4] {
        assert_eq!(str, "hello");
    }
    assert!(vec.is_full());
}

// ------------------- representation -------------------
#[test]
fn alignment() {
    #[repr(align(64))]
    #[allow(dead_code, reason = "We need a field to make `Aligned` non-ZST")]
    struct Aligned(u64);

    let vec = GrowLock::with_capacity(10);
    let mut guard = vec.write().unwrap();
    for i in 0..10 {
        guard.push(Aligned(i));
    }
    let addr = vec.as_ptr().addr();
    assert_eq!(addr % 64, 0);

    let vec: GrowLock<Aligned> = grow_lock![];
    let addr = vec.as_ptr().addr();
    assert_eq!(addr % 64, 0);
}

// ------------------- push panics -------------------
#[test]
#[should_panic(expected = "length overflow")]
fn push_overflow() {
    let vec = GrowLock::with_capacity(5);
    let mut guard = vec.write().unwrap();
    for i in 0..6 {
        guard.push(i);
    }
}
#[test]
fn try_push_overflow() {
    let vec = GrowLock::with_capacity(5);
    let mut guard = vec.write().unwrap();
    for i in 0..5 {
        assert!(guard.try_push(i).is_ok());
    }
    assert!(guard.try_push(6).is_err());
}

#[test]
fn init_drop_on_panic() {
    use std::panic;

    let counter = AtomicUsize::new(0);
    let result = panic::catch_unwind(|| {
        let vec = GrowLock::with_capacity(10);
        let mut guard = vec.write().unwrap();
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
    let counter = AtomicUsize::new(0);
    {
        let vec = GrowLock::with_capacity(200);
        let mut guard = vec.write().unwrap();
        for _ in 0..100 {
            guard.push(AddOnDrop(&counter));
        }
        // here `vec` is dropped
    }
    assert_eq!(counter.load(Ordering::Relaxed), 100);
}

#[test]
fn zst_drop() {
    static ZST_COUNTER: AtomicUsize = AtomicUsize::new(0);
    struct AddZST;
    impl Drop for AddZST {
        fn drop(&mut self) {
            ZST_COUNTER.fetch_add(1, Ordering::Relaxed);
        }
    }
    {
        let vec = GrowLock::with_capacity(200);
        let mut guard = vec.write().unwrap();
        for _ in 0..150 {
            guard.push(AddZST);
        }
        // here `vec` is dropped
    }
    assert_eq!(ZST_COUNTER.load(Ordering::Relaxed), 150);
}

// ------------------- write -------------------

#[test]
fn write_contention() {
    const THREADS: usize = 10;
    const CAP: usize = 1000;

    let vec = Arc::new(GrowLock::with_capacity(CAP));
    let mut handles = Vec::with_capacity(THREADS);
    for t in 0..THREADS {
        let v = Arc::clone(&vec);
        handles.push(thread::spawn(move || {
            for i in 0..(CAP / THREADS) {
                let mut guard = v.write().unwrap();
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
    let vec = GrowLock::with_capacity(5);
    {
        let mut guard = vec.write().unwrap();
        guard.push("hi");
        guard.push("there");
        assert_eq!(&vec[0..2], ["hi", "there"]);
        guard.push("still locked");
    }
    assert_eq!(vec.len(), 3);
}

#[test]
fn slow_write() {
    let vec = Arc::new(GrowLock::with_capacity(10));
    {
        let mut guard = vec.write().unwrap();
        guard.extend(["hi", "hello", "world"]);
    }
    let vec_clone = Arc::clone(&vec);
    let handle = thread::spawn(move || {
        let mut guard = vec_clone.write().unwrap();
        guard.push("foo");
        thread::sleep(Duration::from_millis(300));
        guard.push("bar");
    });

    // we wait for the writer to take the lock
    // (20millis is overkill, but we never know)
    thread::sleep(Duration::from_millis(20));

    assert!(vec.len() >= 3);
    // while `handle` is writing, we still can read initialized elements.
    assert_eq!(&vec[..3], &["hi", "hello", "world"]);
    // here, 4th element could be (and probably is) already initialized
    if let Some(&fourth) = vec.get(3) {
        dbg!(fourth);
        assert_eq!(fourth, "foo");
    }

    handle.join().unwrap();
    // at this point all the elements are already pushed
    assert_eq!(vec.len(), 5);
    assert_eq!(&vec[3..], &["foo", "bar"]);
}
