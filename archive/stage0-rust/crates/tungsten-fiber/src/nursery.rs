use std::sync::{Arc, Mutex};
use crate::fiber::FiberHandle;
use crate::scheduler::Scheduler;

pub struct Nursery<T> {
    scheduler: Arc<Scheduler>,
    handles: Arc<Mutex<Vec<FiberHandle<T>>>>,
}

impl<T: Clone + Send + 'static> Nursery<T> {
    pub fn new(scheduler: Arc<Scheduler>) -> Self {
        Self {
            scheduler,
            handles: Arc::new(Mutex::new(Vec::new())),
        }
    }

    pub fn spawn<F>(&self, task: F) -> FiberHandle<T>
    where
        F: FnOnce() -> Result<T, String> + Send + 'static,
    {
        let handle = self.scheduler.spawn(task);
        self.handles.lock().unwrap().push(handle.clone());
        handle
    }

    pub fn wait_all(&self) -> Result<Vec<T>, String> {
        let handles = self.handles.lock().unwrap().clone();
        let mut results = Vec::new();
        for h in handles {
            let res = h.join()?;
            results.push(res);
        }
        Ok(results)
    }
}

