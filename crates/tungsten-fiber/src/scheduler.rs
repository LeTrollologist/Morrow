use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use crate::fiber::{FiberHandle, FiberId};

pub struct TaskJob {
    pub id: FiberId,
    pub task: Box<dyn FnOnce() + Send + 'static>,
}

pub struct Scheduler {
    num_threads: usize,
    injector: Arc<Mutex<VecDeque<TaskJob>>>,
    worker_queues: Arc<Vec<Arc<Mutex<VecDeque<TaskJob>>>>>,
    shutdown: Arc<AtomicBool>,
    workers: Mutex<Vec<JoinHandle<()>>>,
}

impl Scheduler {
    pub fn new(num_threads: usize) -> Arc<Self> {
        let n = if num_threads == 0 {
            std::thread::available_parallelism()
                .map(|p| p.get())
                .unwrap_or(2)
        } else {
            num_threads
        };

        let injector = Arc::new(Mutex::new(VecDeque::new()));
        let mut queues = Vec::new();
        for _ in 0..n {
            queues.push(Arc::new(Mutex::new(VecDeque::new())));
        }
        let worker_queues = Arc::new(queues);
        let shutdown = Arc::new(AtomicBool::new(false));

        let sched = Arc::new(Self {
            num_threads: n,
            injector,
            worker_queues,
            shutdown,
            workers: Mutex::new(Vec::new()),
        });

        sched.start_workers();
        sched
    }

    fn start_workers(&self) {
        let mut join_handles = Vec::new();
        for idx in 0..self.num_threads {
            let injector = Arc::clone(&self.injector);
            let worker_queues = Arc::clone(&self.worker_queues);
            let shutdown = Arc::clone(&self.shutdown);

            let handle = thread::Builder::new()
                .name(format!("tungsten-fiber-{}", idx))
                .spawn(move || {
                    Self::worker_loop(idx, injector, worker_queues, shutdown);
                })
                .expect("Failed to spawn fiber worker thread");

            join_handles.push(handle);
        }

        *self.workers.lock().unwrap() = join_handles;
    }

    fn worker_loop(
        worker_id: usize,
        injector: Arc<Mutex<VecDeque<TaskJob>>>,
        worker_queues: Arc<Vec<Arc<Mutex<VecDeque<TaskJob>>>>>,
        shutdown: Arc<AtomicBool>,
    ) {
        while !shutdown.load(Ordering::Relaxed) {
            // 1. Pop from local worker queue
            let job_opt = {
                let mut local = worker_queues[worker_id].lock().unwrap();
                local.pop_front()
            };

            if let Some(job) = job_opt {
                (job.task)();
                continue;
            }

            // 2. Pop from global injector
            let injector_job = {
                let mut inj = injector.lock().unwrap();
                inj.pop_front()
            };

            if let Some(job) = injector_job {
                (job.task)();
                continue;
            }

            // 3. Work steal from other workers (pop from back)
            let mut stolen = None;
            for (idx, queue) in worker_queues.iter().enumerate() {
                if idx == worker_id {
                    continue;
                }
                if let Ok(mut other) = queue.try_lock() {
                    if let Some(job) = other.pop_back() {
                        stolen = Some(job);
                        break;
                    }
                }
            }

            if let Some(job) = stolen {
                (job.task)();
                continue;
            }

            // No work found, yield thread
            thread::sleep(Duration::from_micros(50));
        }
    }

    pub fn spawn<F, T>(&self, task: F) -> FiberHandle<T>
    where
        F: FnOnce() -> Result<T, String> + Send + 'static,
        T: Clone + Send + 'static,
    {
        let id = FiberId::next();
        let (handle, res_slot, notify) = FiberHandle::new(id);

        let job_task = move || {
            let res = task();
            *res_slot.lock().unwrap() = Some(res);
            notify.notify_all();
        };

        self.injector.lock().unwrap().push_back(TaskJob {
            id,
            task: Box::new(job_task),
        });

        handle
    }

    pub fn shutdown(&self) {
        self.shutdown.store(true, Ordering::Relaxed);
        let mut workers = self.workers.lock().unwrap();
        for h in workers.drain(..) {
            let _ = h.join();
        }
    }
}

impl Drop for Scheduler {
    fn drop(&mut self) {
        self.shutdown();
    }
}

