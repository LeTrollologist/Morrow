use std::io::{self, Write};
use std::slice;
use std::str;

#[no_mangle]
pub extern "C" fn tungsten_print_i64(val: i64) {
    print!("{}", val);
    let _ = io::stdout().flush();
}

#[no_mangle]
pub extern "C" fn tungsten_println_i64(val: i64) {
    println!("{}", val);
}

#[no_mangle]
pub extern "C" fn tungsten_print_str(ptr: *const u8, mut len: usize) {
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

    pub fn destroy(mut self) {
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
pub extern "C" fn tungsten_region_exit(arena: *mut PhysicalArena) {
    if !arena.is_null() {
        unsafe {
            let boxed = Box::from_raw(arena);
            boxed.destroy();
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

static EFFECT_TRACES: std::sync::OnceLock<std::sync::Mutex<Vec<String>>> = std::sync::OnceLock::new();

fn get_effect_traces() -> &'static std::sync::Mutex<Vec<String>> {
    EFFECT_TRACES.get_or_init(|| std::sync::Mutex::new(Vec::new()))
}

pub fn clear_effect_traces() {
    get_effect_traces().lock().unwrap().clear();
}

pub fn get_recorded_effect_traces() -> Vec<String> {
    get_effect_traces().lock().unwrap().clone()
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
                get_effect_traces().lock().unwrap().push(entry);
            }
        }
    }
}



