use std::collections::VecDeque;
use std::sync::{Arc, Condvar, Mutex};

struct ChannelInner<T> {
    queue: Mutex<VecDeque<T>>,
    condvar: Condvar,
    cap: Option<usize>,
}

pub struct Channel<T> {
    inner: Arc<ChannelInner<T>>,
}

impl<T> Channel<T> {
    pub fn unbounded() -> Self {
        Self {
            inner: Arc::new(ChannelInner {
                queue: Mutex::new(VecDeque::new()),
                condvar: Condvar::new(),
                cap: None,
            }),
        }
    }

    pub fn bounded(cap: usize) -> Self {
        Self {
            inner: Arc::new(ChannelInner {
                queue: Mutex::new(VecDeque::with_capacity(cap)),
                condvar: Condvar::new(),
                cap: Some(cap),
            }),
        }
    }

    pub fn send(&self, msg: T) -> Result<(), String> {
        let mut q = self.inner.queue.lock().unwrap();
        if let Some(cap) = self.inner.cap {
            while q.len() >= cap {
                q = self.inner.condvar.wait(q).unwrap();
            }
        }
        q.push_back(msg);
        self.inner.condvar.notify_one();
        Ok(())
    }

    pub fn recv(&self) -> Result<T, String> {
        let mut q = self.inner.queue.lock().unwrap();
        while q.is_empty() {
            q = self.inner.condvar.wait(q).unwrap();
        }
        let val = q.pop_front().unwrap();
        self.inner.condvar.notify_one();
        Ok(val)
    }

    pub fn try_recv(&self) -> Result<Option<T>, String> {
        let mut q = self.inner.queue.lock().unwrap();
        if let Some(val) = q.pop_front() {
            self.inner.condvar.notify_one();
            Ok(Some(val))
        } else {
            Ok(None)
        }
    }
}

