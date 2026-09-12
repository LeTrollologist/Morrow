pub mod fiber;
pub mod scheduler;
pub mod nursery;
pub mod channel;

pub use fiber::{FiberHandle, FiberId, FiberState, FiberTask};
pub use scheduler::Scheduler;
pub use nursery::Nursery;
pub use channel::Channel;

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;
    use std::time::Duration;

    #[test]
    fn test_fiber_spawn_and_join() {
        let sched = Scheduler::new(2);
        let handle = sched.spawn(|| {
            Ok(42)
        });
        assert_eq!(handle.join().unwrap(), 42);
    }

    #[test]
    fn test_fiber_work_stealing_multiplexing() {
        let sched = Scheduler::new(4);
        let counter = Arc::new(AtomicUsize::new(0));

        let mut handles = Vec::new();
        for _ in 0..50 {
            let c = Arc::clone(&counter);
            handles.push(sched.spawn(move || {
                std::thread::sleep(Duration::from_millis(2));
                c.fetch_add(1, Ordering::SeqCst);
                Ok(())
            }));
        }

        for h in handles {
            h.join().unwrap();
        }

        assert_eq!(counter.load(Ordering::SeqCst), 50);
    }

    #[test]
    fn test_nursery_structured_concurrency() {
        let sched = Scheduler::new(3);
        let nursery: Nursery<i32> = Nursery::new(Arc::clone(&sched));

        nursery.spawn(|| Ok(10));
        nursery.spawn(|| Ok(20));
        nursery.spawn(|| Ok(30));

        let results = nursery.wait_all().unwrap();
        assert_eq!(results.len(), 3);
        assert!(results.contains(&10));
        assert!(results.contains(&20));
        assert!(results.contains(&30));
    }

    #[test]
    fn test_fiber_channel() {
        let channel = Arc::new(Channel::unbounded());
        let sched = Scheduler::new(2);

        let ch_send = Arc::clone(&channel);
        let h_producer = sched.spawn(move || {
            ch_send.send(100)?;
            ch_send.send(200)?;
            Ok(())
        });

        let ch_recv = Arc::clone(&channel);
        let h_consumer = sched.spawn(move || {
            let v1 = ch_recv.recv()?;
            let v2 = ch_recv.recv()?;
            Ok(v1 + v2)
        });

        h_producer.join().unwrap();
        assert_eq!(h_consumer.join().unwrap(), 300);
    }
}
