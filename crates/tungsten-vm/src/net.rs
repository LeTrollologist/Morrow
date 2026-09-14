// tungsten-vm/src/net.rs
// Real TCP socket registry for the Net algebraic effect.
//
// Each socket (listener OR stream) is stored as a `SocketEntry` keyed by a
// monotonically-increasing u64 handle.  The registry is an
// `Arc<Mutex<HashMap<u64, SocketEntry>>>` so it can be cheaply cloned into
// fiber sub-evaluators—they all share the same socket table, exactly the way
// `channels` are shared.
//
// Design notes:
// - `TcpListener::accept` is made non-blocking *at the VM level* by having
//   each call block the current fiber thread (not the whole VM), which is
//   correct because the calling fiber is running on a scheduler worker thread.
// - `Net::read` does a single `read_to_string`-style loop capped at
//   `max_bytes` characters; the connection is left open afterwards.
// - All handles are opaque i64 values from the Tungsten perspective.

use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicU64, Ordering};

// ---------------------------------------------------------------------------
// Socket handle allocator
// ---------------------------------------------------------------------------

static SOCKET_COUNTER: AtomicU64 = AtomicU64::new(1);

pub fn next_socket_id() -> u64 {
    SOCKET_COUNTER.fetch_add(1, Ordering::Relaxed)
}

// ---------------------------------------------------------------------------
// Socket entry variants
// ---------------------------------------------------------------------------

pub enum SocketEntry {
    Listener(TcpListener),
    Stream(TcpStream),
}

// ---------------------------------------------------------------------------
// Registry
// ---------------------------------------------------------------------------

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

    // -----------------------------------------------------------------------
    // Net::listen(port) -> i64
    // -----------------------------------------------------------------------
    pub fn listen(&self, port: u16) -> Result<u64, String> {
        let addr = format!("127.0.0.1:{}", port);
        let listener = TcpListener::bind(&addr)
            .map_err(|e| format!("Net::listen failed to bind {}: {}", addr, e))?;
        // Allow multiple rapid bind-unbind cycles in tests
        listener.set_nonblocking(false)
            .map_err(|e| format!("Net::listen set_nonblocking failed: {}", e))?;
        let id = next_socket_id();
        self.inner.lock().unwrap().insert(id, SocketEntry::Listener(listener));
        Ok(id)
    }

    // -----------------------------------------------------------------------
    // Net::accept(listener_id) -> i64
    // -----------------------------------------------------------------------
    pub fn accept(&self, listener_id: u64) -> Result<u64, String> {
        // We need to call accept() while NOT holding the registry lock
        // (otherwise no other fiber can touch the registry while we block).
        // Pull the listener out, accept, then put it back.
        let listener_clone = {
            let guard = self.inner.lock().unwrap();
            match guard.get(&listener_id) {
                Some(SocketEntry::Listener(l)) => {
                    // TcpListener doesn't impl Clone, so use try_clone
                    l.try_clone()
                        .map_err(|e| format!("Net::accept try_clone failed: {}", e))?
                }
                Some(SocketEntry::Stream(_)) => {
                    return Err(format!("Net::accept: handle {} is a stream, not a listener", listener_id));
                }
                None => {
                    return Err(format!("Net::accept: listener {} not found", listener_id));
                }
            }
        };

        let (stream, _addr) = listener_clone.accept()
            .map_err(|e| format!("Net::accept failed: {}", e))?;

        let id = next_socket_id();
        self.inner.lock().unwrap().insert(id, SocketEntry::Stream(stream));
        Ok(id)
    }

    // -----------------------------------------------------------------------
    // Net::connect(host, port) -> i64
    // -----------------------------------------------------------------------
    pub fn connect(&self, host: &str, port: u16) -> Result<u64, String> {
        let addr = format!("{}:{}", host, port);
        let stream = TcpStream::connect(&addr)
            .map_err(|e| format!("Net::connect failed to connect {}: {}", addr, e))?;
        let id = next_socket_id();
        self.inner.lock().unwrap().insert(id, SocketEntry::Stream(stream));
        Ok(id)
    }

    // -----------------------------------------------------------------------
    // Net::read(conn_id, max_bytes) -> String
    // -----------------------------------------------------------------------
    pub fn read(&self, conn_id: u64, max_bytes: usize) -> Result<String, String> {
        let stream_clone = {
            let guard = self.inner.lock().unwrap();
            match guard.get(&conn_id) {
                Some(SocketEntry::Stream(s)) => s.try_clone()
                    .map_err(|e| format!("Net::read try_clone failed: {}", e))?,
                Some(SocketEntry::Listener(_)) => {
                    return Err(format!("Net::read: handle {} is a listener, not a stream", conn_id));
                }
                None => return Err(format!("Net::read: connection {} not found", conn_id)),
            }
        };

        let capped = max_bytes.min(65536);
        let mut buf = vec![0u8; capped];
        let mut stream = stream_clone;
        let n = stream.read(&mut buf)
            .map_err(|e| format!("Net::read failed: {}", e))?;

        String::from_utf8(buf[..n].to_vec())
            .map_err(|e| format!("Net::read utf8 error: {}", e))
    }

    // -----------------------------------------------------------------------
    // Net::write(conn_id, data) -> i64  (bytes written)
    // -----------------------------------------------------------------------
    pub fn write(&self, conn_id: u64, data: &str) -> Result<usize, String> {
        let stream_clone = {
            let guard = self.inner.lock().unwrap();
            match guard.get(&conn_id) {
                Some(SocketEntry::Stream(s)) => s.try_clone()
                    .map_err(|e| format!("Net::write try_clone failed: {}", e))?,
                Some(SocketEntry::Listener(_)) => {
                    return Err(format!("Net::write: handle {} is a listener, not a stream", conn_id));
                }
                None => return Err(format!("Net::write: connection {} not found", conn_id)),
            }
        };

        let bytes = data.as_bytes();
        let mut stream = stream_clone;
        stream.write_all(bytes)
            .map_err(|e| format!("Net::write failed: {}", e))?;
        Ok(bytes.len())
    }

    // -----------------------------------------------------------------------
    // Net::close(conn_id)
    // -----------------------------------------------------------------------
    pub fn close(&self, conn_id: u64) {
        // Remove the entry—Drop on TcpStream/TcpListener closes the OS fd.
        self.inner.lock().unwrap().remove(&conn_id);
    }

    // -----------------------------------------------------------------------
    // Local port helper (for tests)
    // -----------------------------------------------------------------------
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
    fn default() -> Self { Self::new() }
}
