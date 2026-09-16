use std::io::{self, Write};
use std::slice;
use std::str;

static SILENT_MODE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

pub fn set_silent_mode(silent: bool) {
    SILENT_MODE.store(silent, std::sync::atomic::Ordering::Relaxed);
}

pub fn is_silent_mode() -> bool {
    SILENT_MODE.load(std::sync::atomic::Ordering::Relaxed)
}

#[no_mangle]
pub extern "C" fn tungsten_print_i64(val: i64) {
    if is_silent_mode() {
        return;
    }
    print!("{}", val);
    let _ = io::stdout().flush();
}

#[no_mangle]
pub extern "C" fn tungsten_println_i64(val: i64) {
    if is_silent_mode() {
        return;
    }
    println!("{}", val);
}

#[no_mangle]
pub extern "C" fn tungsten_print_str(ptr: *const u8, mut len: usize) {
    if is_silent_mode() {
        return;
    }
    if !ptr.is_null() {
        unsafe {
            let mut null_pos = None;
            for i in 0..len {
                if *ptr.add(i) == 0 {
                    null_pos = Some(i);
                    break;
                }
            }
            if let Some(pos) = null_pos {
                len = pos;
            }
            if len > 0 {
                let bytes = slice::from_raw_parts(ptr, len);
                if let Ok(s) = str::from_utf8(bytes) {
                    print!("{}", s);
                    let _ = io::stdout().flush();
                }
            }
        }
    }
}

#[no_mangle]
pub extern "C" fn tungsten_println_str(ptr: *const u8, mut len: usize) {
    if is_silent_mode() {
        return;
    }
    if !ptr.is_null() {
        unsafe {
            let mut null_pos = None;
            for i in 0..len {
                if *ptr.add(i) == 0 {
                    null_pos = Some(i);
                    break;
                }
            }
            if let Some(pos) = null_pos {
                len = pos;
            }
            if len > 0 {
                let bytes = slice::from_raw_parts(ptr, len);
                if let Ok(s) = str::from_utf8(bytes) {
                    println!("{}", s);
                    return;
                }
            }
        }
    }
    println!();
}

#[no_mangle]
pub extern "C" fn tungsten_io_print(ptr: *const u8, mut len: usize) {
    if !ptr.is_null() {
        unsafe {
            let mut null_pos = None;
            for i in 0..len {
                if *ptr.add(i) == 0 {
                    null_pos = Some(i);
                    break;
                }
            }
            if let Some(pos) = null_pos {
                len = pos;
            }
            if len > 0 {
                let bytes = slice::from_raw_parts(ptr, len);
                if let Ok(s) = str::from_utf8(bytes) {
                    println!("[IO] {}", s);
                    return;
                }
            }
        }
    }
    println!("[IO]");
}

#[no_mangle]
pub extern "C" fn tungsten_refinement_panic(val: i64, min: i64, max: i64) {
    eprintln!(
        "\n[Tungsten Native Refinement Panic]: Value {} is outside statically refined range [{}, {}]",
        val, min, max
    );
    std::process::exit(101);
}

#[no_mangle]
pub extern "C" fn tungsten_alloc(size: usize, align: usize) -> *mut u8 {
    let align = align.max(1).next_power_of_two();
    if size == 0 {
        return align as *mut u8;
    }
    let layout = std::alloc::Layout::from_size_align(size, align).unwrap_or_else(|_| {
        std::alloc::Layout::from_size_align(8, 8).unwrap()
    });
    unsafe { std::alloc::alloc(layout) }
}

pub struct PhysicalArena {
    chunks: Vec<(*mut u8, usize, usize)>, // (ptr, capacity, align)
    current_ptr: *mut u8,
    current_offset: usize,
    current_capacity: usize,
}

impl PhysicalArena {
    pub fn new() -> Self {
        const INITIAL_CAPACITY: usize = 4096;
        let layout = std::alloc::Layout::from_size_align(INITIAL_CAPACITY, 128).unwrap();
        let ptr = unsafe { std::alloc::alloc(layout) };
        Self {
            chunks: vec![(ptr, INITIAL_CAPACITY, 128)],
            current_ptr: ptr,
            current_offset: 0,
            current_capacity: INITIAL_CAPACITY,
        }
    }

    pub fn alloc(&mut self, size: usize, align: usize) -> *mut u8 {
        let align = align.max(1).next_power_of_two();
        if size == 0 {
            return align as *mut u8;
        }
        let align_mask = align - 1;
        let current_addr = (self.current_ptr as usize) + self.current_offset;
        let aligned_addr = match current_addr.checked_add(align_mask) {
            Some(addr) => addr & !align_mask,
            None => return std::ptr::null_mut(),
        };
        let offset = aligned_addr - (self.current_ptr as usize);

        if let Some(req_offset) = offset.checked_add(size) {
            if req_offset <= self.current_capacity {
                self.current_offset = req_offset;
                return aligned_addr as *mut u8;
            }
        }

        let chunk_align = align.max(128);
        let min_required = match size.checked_add(chunk_align) {
            Some(sum) => sum,
            None => return std::ptr::null_mut(),
        };
        let new_cap = match self.current_capacity.checked_mul(2) {
            Some(doubled) => doubled.max(min_required).max(4096),
            None => min_required.max(4096),
        };

        let layout = match std::alloc::Layout::from_size_align(new_cap, chunk_align) {
            Ok(l) => l,
            Err(_) => return std::ptr::null_mut(),
        };
        let new_ptr = unsafe { std::alloc::alloc(layout) };
        if new_ptr.is_null() {
            return std::ptr::null_mut();
        }
        self.chunks.push((new_ptr, new_cap, chunk_align));
        self.current_ptr = new_ptr;
        self.current_capacity = new_cap;
        self.current_offset = size;
        new_ptr
    }

    pub fn try_grow(&mut self, ptr: *mut u8, old_size: usize, new_size: usize, align: usize) -> *mut u8 {
        if ptr.is_null() || old_size == 0 {
            return self.alloc(new_size, align);
        }
        if new_size <= old_size {
            return ptr;
        }

        let align = align.max(1).next_power_of_two();
        let align_mask = align - 1;
        let ptr_addr = ptr as usize;
        let current_base = self.current_ptr as usize;
        let current_top = current_base + self.current_offset;

        // Check if `ptr` belongs to current chunk and satisfies required alignment
        let is_in_current_chunk = ptr_addr >= current_base && ptr_addr < current_top;
        let is_aligned = (ptr_addr & align_mask) == 0;

        if is_in_current_chunk && is_aligned {
            let end_of_alloc = match ptr_addr.checked_add(old_size) {
                Some(end) => end,
                None => return std::ptr::null_mut(),
            };

            // Invariant: ptr + old_size == current_top (strictly the current tail of the arena)
            if end_of_alloc == current_top {
                let diff = new_size - old_size;
                if let Some(new_offset) = self.current_offset.checked_add(diff) {
                    if new_offset <= self.current_capacity {
                        self.current_offset = new_offset;
                        return ptr; // O(1) in-place growth with ZERO memcpy!
                    }
                }
            }
        }

        // Fallback: Allocate new buffer in arena, copy old contents
        let new_ptr = self.alloc(new_size, align);
        if !new_ptr.is_null() && old_size > 0 {
            unsafe {
                std::ptr::copy_nonoverlapping(ptr, new_ptr, old_size.min(new_size));
            }
        }
        new_ptr
    }

    pub fn destroy(&mut self) {
        for (ptr, cap, chunk_align) in self.chunks.drain(..) {
            if !ptr.is_null() && cap > 0 {
                let layout = std::alloc::Layout::from_size_align(cap, chunk_align).unwrap();
                unsafe {
                    std::alloc::dealloc(ptr, layout);
                }
            }
        }
    }
}

impl Drop for PhysicalArena {
    fn drop(&mut self) {
        self.destroy();
    }
}

#[no_mangle]
pub extern "C" fn tungsten_region_enter() -> *mut PhysicalArena {
    let arena = Box::new(PhysicalArena::new());
    Box::into_raw(arena)
}

#[no_mangle]
pub extern "C" fn tungsten_region_alloc(arena: *mut PhysicalArena, size: usize, align: usize) -> *mut u8 {
    if arena.is_null() {
        return tungsten_alloc(size, align);
    }
    unsafe {
        (*arena).alloc(size, align)
    }
}

#[no_mangle]
pub extern "C" fn tungsten_region_grow(
    arena: *mut PhysicalArena,
    ptr: *mut u8,
    old_size: usize,
    new_size: usize,
    align: usize,
) -> *mut u8 {
    if arena.is_null() {
        unsafe {
            if ptr.is_null() {
                tungsten_alloc(new_size, align)
            } else {
                let layout = std::alloc::Layout::from_size_align(old_size, align.max(1)).unwrap();
                std::alloc::realloc(ptr, layout, new_size)
            }
        }
    } else {
        unsafe {
            (*arena).try_grow(ptr, old_size, new_size, align)
        }
    }
}

#[no_mangle]
pub extern "C" fn tungsten_region_exit(arena: *mut PhysicalArena) {
    if !arena.is_null() {
        unsafe {
            // Box::from_raw takes ownership and its Drop implementation automatically invokes destroy()
            let _boxed = Box::from_raw(arena);
        }
    }
}

// Global runtime scheduler and channels for native JIT execution
static SCHEDULER: std::sync::OnceLock<std::sync::Arc<tungsten_fiber::Scheduler>> = std::sync::OnceLock::new();
static CHANNELS: std::sync::OnceLock<std::sync::Mutex<std::collections::HashMap<u64, std::sync::Arc<tungsten_fiber::Channel<i64>>>>> = std::sync::OnceLock::new();

fn get_scheduler() -> &'static std::sync::Arc<tungsten_fiber::Scheduler> {
    SCHEDULER.get_or_init(|| tungsten_fiber::Scheduler::new(0))
}

fn get_channels() -> &'static std::sync::Mutex<std::collections::HashMap<u64, std::sync::Arc<tungsten_fiber::Channel<i64>>>> {
    CHANNELS.get_or_init(|| std::sync::Mutex::new(std::collections::HashMap::new()))
}

#[no_mangle]
pub extern "C" fn tungsten_fiber_spawn(func_ptr: *const u8, arg1: i64, arg2: i64) -> u64 {
    let sched = get_scheduler();
    let fn_addr = func_ptr as usize;

    let handle = sched.spawn(move || {
        let func: extern "C" fn(i64, i64) -> i64 = unsafe { std::mem::transmute(fn_addr) };
        Ok(func(arg1, arg2))
    });

    handle.id.0
}

#[no_mangle]
pub extern "C" fn tungsten_fiber_yield() {
    std::thread::yield_now();
}

#[no_mangle]
pub extern "C" fn tungsten_fiber_sleep(ms: u64) {
    std::thread::sleep(std::time::Duration::from_millis(ms));
}

#[no_mangle]
pub extern "C" fn tungsten_channel_new() -> u64 {
    static CH_ID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
    let cid = CH_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let ch = std::sync::Arc::new(tungsten_fiber::Channel::unbounded());
    get_channels().lock().unwrap().insert(cid, ch);
    cid
}

#[no_mangle]
pub extern "C" fn tungsten_channel_send(cid: u64, val: i64) {
    let ch_opt = get_channels().lock().unwrap().get(&cid).cloned();
    if let Some(ch) = ch_opt {
        let _ = ch.send(val);
    }
}

#[no_mangle]
pub extern "C" fn tungsten_channel_recv(cid: u64) -> i64 {
    let ch_opt = get_channels().lock().unwrap().get(&cid).cloned();
    if let Some(ch) = ch_opt {
        ch.recv().unwrap_or(0)
    } else {
        0
    }
}

static SOCKETS: std::sync::OnceLock<tungsten_fiber::SocketRegistry> = std::sync::OnceLock::new();

pub fn get_sockets() -> &'static tungsten_fiber::SocketRegistry {
    SOCKETS.get_or_init(|| tungsten_fiber::SocketRegistry::new())
}

#[no_mangle]
pub extern "C" fn tungsten_net_listen(port: u64) -> u64 {
    get_sockets().listen(port as u16).unwrap_or(0)
}

#[no_mangle]
pub extern "C" fn tungsten_net_accept(listener: u64) -> u64 {
    get_sockets().accept(listener).unwrap_or(0)
}

#[no_mangle]
pub extern "C" fn tungsten_net_connect(host_ptr: *const u8, host_len: usize, port: u64) -> u64 {
    let host = if !host_ptr.is_null() && host_len > 0 {
        let bytes = unsafe { std::slice::from_raw_parts(host_ptr, host_len) };
        std::str::from_utf8(bytes).unwrap_or("127.0.0.1")
    } else {
        "127.0.0.1"
    };
    get_sockets().connect(host, port as u16).unwrap_or(0)
}

#[no_mangle]
pub extern "C" fn tungsten_net_read(conn: u64, max_len: usize) -> *mut u8 {
    let data = get_sockets().read(conn, max_len).unwrap_or_default();
    let bytes = data.as_bytes();
    let len = bytes.len();
    unsafe {
        let ptr = tungsten_alloc(len + 1, 8);
        std::ptr::copy_nonoverlapping(bytes.as_ptr(), ptr, len);
        *ptr.add(len) = 0;
        ptr
    }
}

#[no_mangle]
pub extern "C" fn tungsten_net_write(conn: u64, data_ptr: *const u8, mut len: usize) -> i64 {
    if data_ptr.is_null() {
        return 0;
    }
    unsafe {
        for i in 0..len {
            if *data_ptr.add(i) == 0 {
                len = i;
                break;
            }
        }
        let bytes = std::slice::from_raw_parts(data_ptr, len);
        get_sockets().write(conn, bytes).unwrap_or(0) as i64
    }
}

#[no_mangle]
pub extern "C" fn tungsten_net_close(conn: u64) {
    get_sockets().close(conn);
}

thread_local! {
    static THREAD_EFFECT_TRACES: std::cell::RefCell<Vec<String>> = std::cell::RefCell::new(Vec::new());
}

pub fn clear_effect_traces() {
    THREAD_EFFECT_TRACES.with(|t| t.borrow_mut().clear());
}

pub fn get_recorded_effect_traces() -> Vec<String> {
    THREAD_EFFECT_TRACES.with(|t| t.borrow().clone())
}

#[no_mangle]
pub extern "C" fn tungsten_trace_effect(
    eff_ptr: *const u8,
    eff_len: usize,
    op_ptr: *const u8,
    op_len: usize,
) {
    if !eff_ptr.is_null() && !op_ptr.is_null() {
        unsafe {
            let eff_bytes = slice::from_raw_parts(eff_ptr, eff_len);
            let op_bytes = slice::from_raw_parts(op_ptr, op_len);
            if let (Ok(eff_str), Ok(op_str)) = (str::from_utf8(eff_bytes), str::from_utf8(op_bytes)) {
                let entry = format!("{}:{}", eff_str, op_str);
                THREAD_EFFECT_TRACES.with(|t| t.borrow_mut().push(entry));
            }
        }
    }
}

#[cfg(test)]
pub mod tests {
    use super::*;

    #[test]
    fn test_miri_arena_alignments() {
        let mut arena = PhysicalArena::new();
        let alignments = [1, 2, 4, 8, 16, 32, 64, 128];
        for &align in &alignments {
            let ptr = arena.alloc(17, align);
            assert!(!ptr.is_null());
            assert_eq!(ptr as usize % align, 0, "Address {:p} not aligned to {}", ptr, align);
            unsafe {
                // Write into allocated memory to verify spatial validity
                std::ptr::write_bytes(ptr, 0xAA, 17);
            }
        }
    }

    #[test]
    fn test_miri_arena_chunk_growth() {
        let mut arena = PhysicalArena::new();
        // Allocate across multiple chunk boundaries
        for i in 0..100 {
            let ptr = arena.alloc(512, 8);
            assert!(!ptr.is_null());
            unsafe {
                let s = slice::from_raw_parts_mut(ptr as *mut u64, 512 / 8);
                s[0] = i as u64;
                s[s.len() - 1] = (i * 10) as u64;
                assert_eq!(s[0], i as u64);
                assert_eq!(s[s.len() - 1], (i * 10) as u64);
            }
        }
        assert!(arena.chunks.len() > 1, "Arena should have grown across multiple chunks");
    }

    #[test]
    fn test_miri_interleaved_arenas_drop() {
        let mut a1 = PhysicalArena::new();
        let mut a2 = PhysicalArena::new();

        let p1 = a1.alloc(64, 8);
        let p2 = a2.alloc(64, 8);

        unsafe {
            std::ptr::write_bytes(p1, 0x11, 64);
            std::ptr::write_bytes(p2, 0x22, 64);
        }

        drop(a1); // a1 freed while a2 still active

        let p3 = a2.alloc(128, 16);
        unsafe {
            std::ptr::write_bytes(p3, 0x33, 128);
        }

        drop(a2); // a2 freed cleanly
    }

    #[test]
    fn test_miri_region_enter_exit_drop() {
        let arena_ptr = tungsten_region_enter();
        assert!(!arena_ptr.is_null());

        let alloc_ptr = tungsten_region_alloc(arena_ptr, 256, 16);
        assert!(!alloc_ptr.is_null());
        assert_eq!(alloc_ptr as usize % 16, 0);

        unsafe {
            std::ptr::write_bytes(alloc_ptr, 0xFF, 256);
        }

        tungsten_region_exit(arena_ptr);
    }

    #[test]
    fn test_raw_region_stress_100k() {
        // 100,000 allocations across deep nested regions with child & parent teardowns
        for parent_cycle in 0..100 {
            let parent_arena = tungsten_region_enter();
            assert!(!parent_arena.is_null());

            for child_cycle in 0..10 {
                let child_arena = tungsten_region_enter();
                assert!(!child_arena.is_null());

                for i in 0..100 {
                    let ptr = tungsten_region_alloc(child_arena, 64, 8);
                    assert!(!ptr.is_null());
                    unsafe {
                        *(ptr as *mut u64) = (parent_cycle * 1000 + child_cycle * 100 + i) as u64;
                    }
                }

                tungsten_region_exit(child_arena);
            }

            // Allocate into parent after child has exited
            let p_ptr = tungsten_region_alloc(parent_arena, 128, 16);
            assert!(!p_ptr.is_null());
            unsafe {
                *(p_ptr as *mut u64) = 0xDEADBEEFCAFEBABE;
            }

            tungsten_region_exit(parent_arena);
        }
    }
}



