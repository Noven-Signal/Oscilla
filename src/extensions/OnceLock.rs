use crate::AppState;
use std::sync::*;

pub trait OnceLock_ext<T> {
    fn get_mutex_guard(&self) -> MutexGuard<'_, T>;
}

impl<T> OnceLock_ext<T> for OnceLock<Mutex<T>> {
    fn get_mutex_guard(&self) -> MutexGuard<'_, T> {
        self.get().unwrap().lock().unwrap()
    }
}
