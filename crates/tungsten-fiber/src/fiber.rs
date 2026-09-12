use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FiberId(pub u64);

impl FiberId {
    pub fn next() -> Self {
        static COUNTER: AtomicU64 = AtomicU64::new(1);
        FiberId(COUNTER.fetch_add(1, Ordering::Relaxed))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FiberState {
    Ready,
    Running,
    Suspended,
    Completed,
    Panicked(String),
}

pub trait FiberTask: Send + 'static {
    type Output: Send + 'static;
    fn run(self: Box<Self>) -> Result<Self::Output, String>;
}

impl<F, T> FiberTask for F
where
    F: FnOnce() -> Result<T, String> + Send + 'static,
    T: Send + 'static,
{
    type Output = T;
    fn run(self: Box<Self>) -> Result<T, String> {
        (*self)()
    }
}

pub struct FiberHandle<T> {
    pub id: FiberId,
    pub(crate) result: Arc<Mutex<Option<Result<T, String>>>>,
    pub(crate) completed_notify: Arc<Condvar>,
}


impl<T> Clone for FiberHandle<T> {
    fn clone(&self) -> Self {
        Self {
            id: self.id,
            result: Arc::clone(&self.result),
            completed_notify: Arc::clone(&self.completed_notify),
        }
    }
}

impl<T> FiberHandle<T> {
    pub fn new(id: FiberId) -> (Self, Arc<Mutex<Option<Result<T, String>>>>, Arc<Condvar>) {
        let result = Arc::new(Mutex::new(None));
        let condvar = Arc::new(Condvar::new());
        let handle = Self {
            id,
            result: Arc::clone(&result),
            completed_notify: Arc::clone(&condvar),
        };
        (handle, result, condvar)
    }

    pub fn is_finished(&self) -> bool {
        self.result.lock().unwrap().is_some()
    }

    pub fn join(&self) -> Result<T, String>
    where
        T: Clone,
    {
        let mut lock = self.result.lock().unwrap();
        while lock.is_none() {
            lock = self.completed_notify.wait(lock).unwrap();
        }
        lock.as_ref().unwrap().clone()
    }
}

