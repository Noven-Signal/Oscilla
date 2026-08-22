use std::fmt::Debug;

pub fn array_init<T: Sized + Debug, const N: usize>(f: impl Fn() -> T) -> [T; N] {
    [(); N].map(|_| f())
}

pub trait VecExt {
    fn last_index(&self) -> Option<usize>;
}
impl<T> VecExt for Vec<T> {
    fn last_index(&self) -> Option<usize> {
        match self.len() {
            0 => None,
            len => Some(len - 1),
        }
    }
}
