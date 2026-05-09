use std::fmt::Debug;

pub fn array_init<T: Sized + Debug, const N: usize>(f: impl Fn() -> T) -> [T; N] {
    [(); N].map(|_| f())
}