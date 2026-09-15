use std::io::{Read, Write};
use std::net::TcpStream;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

fn run_bench(target_name: &str, port: u16, total_requests: usize, concurrency: usize) {
    let req_per_thread = total_requests / concurrency;
    let latencies = Arc::new(Mutex::new(Vec::with_capacity(total_requests)));
    let successes = Arc::new(std::sync::atomic::AtomicUsize::new(0));

    let start_total = Instant::now();
    let mut handles = Vec::new();

    for _ in 0..concurrency {
        let latencies = Arc::clone(&latencies);
        let successes = Arc::clone(&successes);
        handles.push(thread::spawn(move || {
            let mut local_lats = Vec::with_capacity(req_per_thread);
            for _ in 0..req_per_thread {
                let t0 = Instant::now();
                if let Ok(mut stream) = TcpStream::connect(("127.0.0.1", port)) {
                    stream.set_nodelay(true).ok();
                    stream.set_read_timeout(Some(Duration::from_millis(2000))).ok();
                    let req = b"GET / HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n";
                    if stream.write_all(req).is_ok() {
                        let mut buf = [0u8; 256];
                        if let Ok(n) = stream.read(&mut buf) {
                            if n > 0 && &buf[..12] == b"HTTP/1.1 200" {
                                local_lats.push(t0.elapsed().as_micros() as f64 / 1000.0);
                                successes.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                            }
                        }
                    }
                }
            }
            latencies.lock().unwrap().extend(local_lats);
        }));
    }

    for h in handles {
        h.join().unwrap();
    }

    let elapsed = start_total.elapsed();
    let mut all_lats = latencies.lock().unwrap().clone();
    all_lats.sort_by(|a, b| a.partial_cmp(b).unwrap());

    let succ = successes.load(std::sync::atomic::Ordering::Relaxed);
    let rps = succ as f64 / elapsed.as_secs_f64();
    let p50 = all_lats.get(all_lats.len() * 50 / 100).copied().unwrap_or(0.0);
    let p90 = all_lats.get(all_lats.len() * 90 / 100).copied().unwrap_or(0.0);
    let p99 = all_lats.get(all_lats.len() * 99 / 100).copied().unwrap_or(0.0);

    println!("Target: {:<10} | Concurrency: {:<3} | Total: {:<5} | Succ: {:<5} | RPS: {:>8.1} | P50: {:>5.2}ms | P90: {:>5.2}ms | P99: {:>5.2}ms",
        target_name, concurrency, total_requests, succ, rps, p50, p90, p99);
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let target = if args.len() > 1 { &args[1] } else { "all" };

    if target == "tungsten" || target == "all" {
        println!("=== Benchmarking Tungsten (Port 18080) ===");
        run_bench("Tungsten", 18080, 2000, 50);
        run_bench("Tungsten", 18080, 5000, 100);
    }
    if target == "tokio" || target == "all" {
        println!("=== Benchmarking Rust Tokio (Port 18081) ===");
        run_bench("Rust Tokio", 18081, 2000, 50);
        run_bench("Rust Tokio", 18081, 5000, 100);
    }
}
