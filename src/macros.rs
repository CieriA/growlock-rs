#[macro_export]
macro_rules! atomic_vec {
    ($($tokens:tt)*) => {
        $crate::AtomicVec::from(::std::vec![$($tokens)*])
    };
}
