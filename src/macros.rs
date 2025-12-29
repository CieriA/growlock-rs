#[macro_export]
macro_rules! grow_lock {
    () => {
        $crate::GrowLock::with_capacity(0)
    };
    ($capacity:expr) => {
        $crate::GrowLock::with_capacity($capacity)
    };

    ($capacity:expr, [$($elem:expr),*$(,)?]) => {{
        let __v__ = $crate::GrowLock::with_capacity($capacity);
        {
            let mut __guard__ = __v__.write().unwrap();
            $(
                __guard__.push($elem);
            )*
        }
        __v__
    }};

    ($elem:expr ; $len:expr) => {{
        let __v__ = $crate::GrowLock::with_capacity($len);
        {
            let mut __guard__ = __v__.write().unwrap();
            for _ in 0 .. $len {
                __guard__.push(::std::clone::Clone::clone(&$elem));
            }
        }
        __v__
    }};
    ($capacity:expr, [$elem:expr ; $len:expr]) => {{
        let __v__ = $crate::GrowLock::with_capacity($capacity);
        {
            let mut __guard__ = __v__.write().unwrap();
            for _ in 0 .. $len {
                __guard__.push(::std::clone::Clone::clone(&$elem));
            }
        }
        __v__
    }};

    // this is last because everything can match this
    ($($elem:expr),+$(,)?) => {{
        $crate::GrowLock::from(::std::vec![$($elem),*])
    }};
}
