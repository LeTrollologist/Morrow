// crates/tungsten-fiber/src/net.rs
// High-performance non-blocking TCP socket registry for M:N fiber scheduling.

use std::collections::HashMap;
use std::io::{ErrorKind, Read, Write};
use std::net::{TcpListener as StdListener, TcpStream as StdStream};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

static SOCKET_COUNTER: AtomicU64 = AtomicU64::new(1);

pub fn next_socket_id() -> u64 {
    SOCKET_COUNTER.fetch_add(1, Ordering::Relaxed)
}

pub enum SocketEntry {
    Listener(StdListener),
    Stream(StdStream),
}

#[derive(Clone)]
pub struct SocketRegistry {
    inner: Arc<Mutex<HashMap<u64, SocketEntry>>>,
}

impl SocketRegistry {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Net::listen(port) -> u64
    pub fn listen(&self, port: u16) -> Result<u64, String> {
        let addr = format!("0.0.0.0:{}", port);
        let listener = StdListener::bind(&addr)
            .or_else(|_| StdListener::bind(format!("127.0.0.1:{}", port)))
            .map_err(|e| format!("Net::listen failed to bind port {}: {}", port, e))?;

        listener.set_nonblocking(true)
            .map_err(|e| format!("Net::listen set_nonblocking failed: {}", e))?;

        let id = next_socket_id();
        self.inner.lock().unwrap().insert(id, SocketEntry::Listener(listener));
        Ok(id)
    }

    /// Net::accept(listener_id) -> u64
    /// Non-blocking accept with cooperative fiber yielding.
    pub fn accept(&self, listener_id: u64) -> Result<u64, String> {
        loop {
            let res = {
                let guard = self.inner.lock().unwrap();
                match guard.get(&listener_id) {
                    Some(SocketEntry::Listener(l)) => l.accept(),
                    Some(SocketEntry::Stream(_)) => {
                        return Err(format!("Net::accept: handle {} is a stream, not a listener", listener_id));
                    }
                    None => {
                        return Err(format!("Net::accept: listener {} not found", listener_id));
                    }
                }
            };

            match res {
                Ok((stream, _addr)) => {
                    let _ = stream.set_nodelay(true);
                    let _ = stream.set_nonblocking(true);
                    let id = next_socket_id();
                    self.inner.lock().unwrap().insert(id, SocketEntry::Stream(stream));
                    return Ok(id);
                }
                Err(ref e) if e.kind() == ErrorKind::WouldBlock => {
                    // Cooperative fiber yield to work-stealing scheduler
                    std::thread::sleep(Duration::from_micros(50));
                }
                Err(e) => {
                    return Err(format!("Net::accept failed: {}", e));
                }
            }
        }
    }

    /// Net::connect(host, port) -> u64
    pub fn connect(&self, host: &str, port: u16) -> Result<u64, String> {
        let addr = format!("{}:{}", host, port);
        let stream = StdStream::connect(&addr)
            .map_err(|e| format!("Net::connect failed to connect {}: {}", addr, e))?;

        let _ = stream.set_nodelay(true);
        let _ = stream.set_nonblocking(true);
        let id = next_socket_id();
        self.inner.lock().unwrap().insert(id, SocketEntry::Stream(stream));
        Ok(id)
    }

    /// Net::read(conn_id, max_bytes) -> String
    pub fn read(&self, conn_id: u64, max_bytes: usize) -> Result<String, String> {
        let capped = max_bytes.min(65536);
        let mut buf = vec![0u8; capped];
        let n = self.read_bytes(conn_id, &mut buf)?;
        String::from_utf8(buf[..n].to_vec())
            .map_err(|e| format!("Net::read utf8 error: {}", e))
    }

    /// Net::read_bytes(conn_id, buf) -> usize
    pub fn read_bytes(&self, conn_id: u64, buf: &mut [u8]) -> Result<usize, String> {
        loop {
            let res = {
                let mut guard = self.inner.lock().unwrap();
                match guard.get_mut(&conn_id) {
                    Some(SocketEntry::Stream(s)) => s.read(buf),
                    Some(SocketEntry::Listener(_)) => {
                        return Err(format!("Net::read: handle {} is a listener, not a stream", conn_id));
                    }
                    None => return Err(format!("Net::read: connection {} not found", conn_id)),
                }
            };

            match res {
                Ok(n) => return Ok(n),
                Err(ref e) if e.kind() == ErrorKind::WouldBlock => {
                    // Cooperative yield to fiber scheduler
                    std::thread::sleep(Duration::from_micros(20));
                }
                Err(e) => return Err(format!("Net::read failed: {}", e)),
            }
        }
    }

    /// Net::write(conn_id, data) -> usize
    pub fn write(&self, conn_id: u64, data: &[u8]) -> Result<usize, String> {
        let mut written = 0;
        while written < data.len() {
            let res = {
                let mut guard = self.inner.lock().unwrap();
                match guard.get_mut(&conn_id) {
                    Some(SocketEntry::Stream(s)) => s.write(&data[written..]),
                    Some(SocketEntry::Listener(_)) => {
                        return Err(format!("Net::write: handle {} is a listener, not a stream", conn_id));
                    }
                    None => return Err(format!("Net::write: connection {} not found", conn_id)),
                }
            };

            match res {
                Ok(0) => break,
                Ok(n) => written += n,
                Err(ref e) if e.kind() == ErrorKind::WouldBlock => {
                    std::thread::sleep(Duration::from_micros(20));
                }
                Err(e) => return Err(format!("Net::write failed: {}", e)),
            }
        }
        Ok(written)
    }

    /// Net::close(conn_id)
    pub fn close(&self, conn_id: u64) {
        self.inner.lock().unwrap().remove(&conn_id);
    }

    /// Local port helper (for tests)
    pub fn local_port(&self, listener_id: u64) -> Option<u16> {
        let guard = self.inner.lock().unwrap();
        if let Some(SocketEntry::Listener(l)) = guard.get(&listener_id) {
            l.local_addr().ok().map(|a| a.port())
        } else {
            None
        }
    }
}

impl Default for SocketRegistry {
    fn default() -> Self {
        Self::new()
    }
}
