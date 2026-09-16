// crates/tungsten-fiber/src/net.rs
// High-performance Win32 IOCP & cross-platform TCP socket engine for M:N fiber scheduling.

use std::collections::HashMap;
#[allow(unused_imports)]
use std::io::{ErrorKind, Read, Write};
use std::net::{TcpListener as StdListener, TcpStream as StdStream};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::Duration;

#[cfg(windows)]
use std::os::windows::io::AsRawSocket;

static SOCKET_COUNTER: AtomicU64 = AtomicU64::new(1);

pub fn next_socket_id() -> u64 {
    SOCKET_COUNTER.fetch_add(1, Ordering::Relaxed)
}

#[cfg(windows)]
pub mod win32 {
    pub const INVALID_HANDLE_VALUE: usize = !0;
    pub const WSA_IO_PENDING: i32 = 997;
    pub const SHUTDOWN_KEY: usize = 0xDEADBEEF;

    #[repr(C)]
    pub struct WSABUF {
        pub len: u32,
        pub buf: *mut u8,
    }

    #[repr(C)]
    #[derive(Debug)]
    pub struct OVERLAPPED {
        pub internal: usize,
        pub internal_high: usize,
        pub offset: u32,
        pub offset_high: u32,
        pub h_event: usize,
    }

    #[repr(C)]
    pub struct PinnedIoContext {
        pub overlapped: OVERLAPPED,
        pub socket_id: u64,
        pub is_write: bool,
        pub bytes_transferred: u32,
        pub error: u32,
        pub completed: bool,
    }

    impl PinnedIoContext {
        pub fn new(socket_id: u64, is_write: bool) -> Self {
            Self {
                overlapped: OVERLAPPED {
                    internal: 0,
                    internal_high: 0,
                    offset: 0,
                    offset_high: 0,
                    h_event: 0,
                },
                socket_id,
                is_write,
                bytes_transferred: 0,
                error: 0,
                completed: false,
            }
        }

        pub fn reset(&mut self) {
            self.overlapped.internal = 0;
            self.overlapped.internal_high = 0;
            self.overlapped.offset = 0;
            self.overlapped.offset_high = 0;
            self.overlapped.h_event = 0;
            self.bytes_transferred = 0;
            self.error = 0;
            self.completed = false;
        }
    }

    #[link(name = "kernel32")]
    extern "system" {
        pub fn CreateIoCompletionPort(
            file_handle: usize,
            existing_completion_port: usize,
            completion_key: usize,
            number_of_concurrent_threads: u32,
        ) -> usize;

        pub fn GetQueuedCompletionStatus(
            completion_port: usize,
            lp_number_of_bytes_transferred: *mut u32,
            lp_completion_key: *mut usize,
            lp_overlapped: *mut *mut OVERLAPPED,
            dw_milliseconds: u32,
        ) -> i32;

        pub fn PostQueuedCompletionStatus(
            completion_port: usize,
            dw_number_of_bytes_transferred: u32,
            dw_completion_key: usize,
            lp_overlapped: *mut OVERLAPPED,
        ) -> i32;

        pub fn CloseHandle(handle: usize) -> i32;
    }

    #[link(name = "ws2_32")]
    extern "system" {
        pub fn WSARecv(
            s: usize,
            lp_buffers: *const WSABUF,
            dw_buffer_count: u32,
            lp_number_of_bytes_recvd: *mut u32,
            lp_flags: *mut u32,
            lp_overlapped: *mut OVERLAPPED,
            lp_completion_routine: usize,
        ) -> i32;

        pub fn WSASend(
            s: usize,
            lp_buffers: *const WSABUF,
            dw_buffer_count: u32,
            lp_number_of_bytes_sent: *mut u32,
            dw_flags: u32,
            lp_overlapped: *mut OVERLAPPED,
            lp_completion_routine: usize,
        ) -> i32;

        pub fn WSAGetLastError() -> i32;
    }
}

pub struct StreamContext {
    pub stream: StdStream,
    #[cfg(windows)]
    pub raw_socket: usize,
    #[cfg(windows)]
    pub read_sync: Arc<(Mutex<bool>, Condvar)>,
    #[cfg(windows)]
    pub read_ctx: Box<win32::PinnedIoContext>,
    #[cfg(windows)]
    pub write_sync: Arc<(Mutex<bool>, Condvar)>,
    #[cfg(windows)]
    pub write_ctx: Box<win32::PinnedIoContext>,
}

pub enum SocketEntry {
    Listener(StdListener),
    Stream(StreamContext),
}

struct IocpWorkerState {
    shutdown: AtomicBool,
    #[cfg(windows)]
    iocp_handle: usize,
    sync_map: Mutex<HashMap<u64, (Arc<(Mutex<bool>, Condvar)>, Arc<(Mutex<bool>, Condvar)>)>>,
}

#[derive(Clone)]
pub struct SocketRegistry {
    inner: Arc<Mutex<HashMap<u64, SocketEntry>>>,
    iocp_state: Arc<IocpWorkerState>,
}

impl SocketRegistry {
    pub fn new() -> Self {
        #[cfg(windows)]
        let iocp_handle = unsafe {
            win32::CreateIoCompletionPort(
                win32::INVALID_HANDLE_VALUE,
                0,
                0,
                0,
            )
        };

        let iocp_state = Arc::new(IocpWorkerState {
            shutdown: AtomicBool::new(false),
            #[cfg(windows)]
            iocp_handle,
            sync_map: Mutex::new(HashMap::new()),
        });

        // Start IOCP completion worker thread on Windows
        #[cfg(windows)]
        {
            let worker_state = Arc::clone(&iocp_state);
            std::thread::Builder::new()
                .name("tungsten-iocp-worker".into())
                .spawn(move || {
                    Self::iocp_worker_loop(worker_state);
                })
                .expect("Failed to spawn IOCP worker thread");
        }

        Self {
            inner: Arc::new(Mutex::new(HashMap::new())),
            iocp_state,
        }
    }

    #[cfg(windows)]
    fn iocp_worker_loop(state: Arc<IocpWorkerState>) {
        while !state.shutdown.load(Ordering::Relaxed) {
            let mut bytes_transferred: u32 = 0;
            let mut completion_key: usize = 0;
            let mut overlapped_ptr: *mut win32::OVERLAPPED = std::ptr::null_mut();

            let ok = unsafe {
                win32::GetQueuedCompletionStatus(
                    state.iocp_handle,
                    &mut bytes_transferred,
                    &mut completion_key,
                    &mut overlapped_ptr,
                    200, // 200ms tick for clean shutdown check
                )
            };

            if completion_key == win32::SHUTDOWN_KEY {
                break;
            }

            if !overlapped_ptr.is_null() {
                let ctx = unsafe { &mut *(overlapped_ptr as *mut win32::PinnedIoContext) };
                ctx.bytes_transferred = bytes_transferred;
                ctx.error = if ok == 0 {
                    unsafe { win32::WSAGetLastError() as u32 }
                } else {
                    0
                };
                ctx.completed = true;

                let socket_id = ctx.socket_id;
                let is_write = ctx.is_write;

                let sync_pair = {
                    let map = state.sync_map.lock().unwrap();
                    map.get(&socket_id).cloned()
                };

                if let Some((read_sync, write_sync)) = sync_pair {
                    let target = if is_write { write_sync } else { read_sync };
                    let (lock, cvar) = &*target;
                    let mut guard = lock.lock().unwrap();
                    *guard = true;
                    cvar.notify_one();
                }
            }
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
    /// Non-blocking accept with kernel-level IOCP registration on accepted socket.
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

                    #[cfg(windows)]
                    {
                        let raw_socket = stream.as_raw_socket() as usize;
                        unsafe {
                            win32::CreateIoCompletionPort(
                                raw_socket,
                                self.iocp_state.iocp_handle,
                                id as usize,
                                0,
                            );
                        }

                        let read_sync = Arc::new((Mutex::new(false), Condvar::new()));
                        let write_sync = Arc::new((Mutex::new(false), Condvar::new()));

                        self.iocp_state.sync_map.lock().unwrap().insert(
                            id,
                            (Arc::clone(&read_sync), Arc::clone(&write_sync)),
                        );

                        let stream_ctx = StreamContext {
                            stream,
                            raw_socket,
                            read_sync,
                            read_ctx: Box::new(win32::PinnedIoContext::new(id, false)),
                            write_sync,
                            write_ctx: Box::new(win32::PinnedIoContext::new(id, true)),
                        };

                        self.inner.lock().unwrap().insert(id, SocketEntry::Stream(stream_ctx));
                    }

                    #[cfg(not(windows))]
                    {
                        let stream_ctx = StreamContext { stream };
                        self.inner.lock().unwrap().insert(id, SocketEntry::Stream(stream_ctx));
                    }

                    return Ok(id);
                }
                Err(ref e) if e.kind() == ErrorKind::WouldBlock => {
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

        #[cfg(windows)]
        {
            let raw_socket = stream.as_raw_socket() as usize;
            unsafe {
                win32::CreateIoCompletionPort(
                    raw_socket,
                    self.iocp_state.iocp_handle,
                    id as usize,
                    0,
                );
            }

            let read_sync = Arc::new((Mutex::new(false), Condvar::new()));
            let write_sync = Arc::new((Mutex::new(false), Condvar::new()));

            self.iocp_state.sync_map.lock().unwrap().insert(
                id,
                (Arc::clone(&read_sync), Arc::clone(&write_sync)),
            );

            let stream_ctx = StreamContext {
                stream,
                raw_socket,
                read_sync,
                read_ctx: Box::new(win32::PinnedIoContext::new(id, false)),
                write_sync,
                write_ctx: Box::new(win32::PinnedIoContext::new(id, true)),
            };

            self.inner.lock().unwrap().insert(id, SocketEntry::Stream(stream_ctx));
        }

        #[cfg(not(windows))]
        {
            let stream_ctx = StreamContext { stream };
            self.inner.lock().unwrap().insert(id, SocketEntry::Stream(stream_ctx));
        }

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
    /// Asynchronous kernel-driven read via Windows IOCP WSARecv.
    #[cfg(windows)]
    pub fn read_bytes(&self, conn_id: u64, buf: &mut [u8]) -> Result<usize, String> {
        let (raw_socket, read_sync, read_ctx_ptr) = {
            let mut guard = self.inner.lock().unwrap();
            match guard.get_mut(&conn_id) {
                Some(SocketEntry::Stream(s)) => {
                    (s.raw_socket, Arc::clone(&s.read_sync), &mut *s.read_ctx as *mut win32::PinnedIoContext)
                }
                Some(SocketEntry::Listener(_)) => {
                    return Err(format!("Net::read: handle {} is a listener, not a stream", conn_id));
                }
                None => return Err(format!("Net::read: connection {} not found", conn_id)),
            }
        };

        let ctx = unsafe { &mut *read_ctx_ptr };
        ctx.reset();

        let wsa_buf = win32::WSABUF {
            len: buf.len() as u32,
            buf: buf.as_mut_ptr(),
        };

        let mut bytes_recvd: u32 = 0;
        let mut flags: u32 = 0;

        let ret = unsafe {
            win32::WSARecv(
                raw_socket,
                &wsa_buf,
                1,
                &mut bytes_recvd,
                &mut flags,
                &mut ctx.overlapped,
                0,
            )
        };

        if ret == 0 {
            // Immediate synchronous completion from socket buffer
            return Ok(bytes_recvd as usize);
        }

        let err = unsafe { win32::WSAGetLastError() };
        if err != win32::WSA_IO_PENDING {
            return Err(format!("WSARecv failed immediately with error: {}", err));
        }

        // Pended to IOCP kernel event port: wait on condvar notification
        let (lock, cvar) = &*read_sync;
        let mut completed_guard = lock.lock().unwrap();
        while !*completed_guard {
            completed_guard = cvar.wait(completed_guard).unwrap();
        }
        *completed_guard = false; // Reset for next operation

        if ctx.error != 0 && ctx.error != 10054 && ctx.error != 10053 {
            return Err(format!("Asynchronous read completed with OS error: {}", ctx.error));
        }

        Ok(ctx.bytes_transferred as usize)
    }

    #[cfg(not(windows))]
    pub fn read_bytes(&self, conn_id: u64, buf: &mut [u8]) -> Result<usize, String> {
        loop {
            let res = {
                let mut guard = self.inner.lock().unwrap();
                match guard.get_mut(&conn_id) {
                    Some(SocketEntry::Stream(s)) => s.stream.read(buf),
                    Some(SocketEntry::Listener(_)) => {
                        return Err(format!("Net::read: handle {} is a listener, not a stream", conn_id));
                    }
                    None => return Err(format!("Net::read: connection {} not found", conn_id)),
                }
            };

            match res {
                Ok(n) => return Ok(n),
                Err(ref e) if e.kind() == ErrorKind::WouldBlock => {
                    std::thread::sleep(Duration::from_micros(20));
                }
                Err(e) => return Err(format!("Net::read failed: {}", e)),
            }
        }
    }

    /// Net::write(conn_id, data) -> usize
    /// Asynchronous kernel-driven write via Windows IOCP WSASend.
    #[cfg(windows)]
    pub fn write(&self, conn_id: u64, data: &[u8]) -> Result<usize, String> {
        let (raw_socket, write_sync, write_ctx_ptr) = {
            let mut guard = self.inner.lock().unwrap();
            match guard.get_mut(&conn_id) {
                Some(SocketEntry::Stream(s)) => {
                    (s.raw_socket, Arc::clone(&s.write_sync), &mut *s.write_ctx as *mut win32::PinnedIoContext)
                }
                Some(SocketEntry::Listener(_)) => {
                    return Err(format!("Net::write: handle {} is a listener, not a stream", conn_id));
                }
                None => return Err(format!("Net::write: connection {} not found", conn_id)),
            }
        };

        let ctx = unsafe { &mut *write_ctx_ptr };
        ctx.reset();

        let wsa_buf = win32::WSABUF {
            len: data.len() as u32,
            buf: data.as_ptr() as *mut u8,
        };

        let mut bytes_sent: u32 = 0;
        let ret = unsafe {
            win32::WSASend(
                raw_socket,
                &wsa_buf,
                1,
                &mut bytes_sent,
                0,
                &mut ctx.overlapped,
                0,
            )
        };

        if ret == 0 {
            return Ok(bytes_sent as usize);
        }

        let err = unsafe { win32::WSAGetLastError() };
        if err != win32::WSA_IO_PENDING {
            return Err(format!("WSASend failed immediately with error: {}", err));
        }

        let (lock, cvar) = &*write_sync;
        let mut completed_guard = lock.lock().unwrap();
        while !*completed_guard {
            completed_guard = cvar.wait(completed_guard).unwrap();
        }
        *completed_guard = false;

        if ctx.error != 0 {
            return Err(format!("Asynchronous write completed with OS error: {}", ctx.error));
        }

        Ok(ctx.bytes_transferred as usize)
    }

    #[cfg(not(windows))]
    pub fn write(&self, conn_id: u64, data: &[u8]) -> Result<usize, String> {
        let mut written = 0;
        while written < data.len() {
            let res = {
                let mut guard = self.inner.lock().unwrap();
                match guard.get_mut(&conn_id) {
                    Some(SocketEntry::Stream(s)) => s.stream.write(&data[written..]),
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
        self.iocp_state.sync_map.lock().unwrap().remove(&conn_id);
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

impl Drop for SocketRegistry {
    fn drop(&mut self) {
        // Post shutdown notification if strong count is 1
        if Arc::strong_count(&self.iocp_state) <= 2 {
            self.iocp_state.shutdown.store(true, Ordering::Relaxed);
            #[cfg(windows)]
            unsafe {
                win32::PostQueuedCompletionStatus(
                    self.iocp_state.iocp_handle,
                    0,
                    win32::SHUTDOWN_KEY,
                    std::ptr::null_mut(),
                );
            }
        }
    }
}

impl Default for SocketRegistry {
    fn default() -> Self {
        Self::new()
    }
}
