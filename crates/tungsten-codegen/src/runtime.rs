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
    let layout = std::alloc::Layout::from_size_align(size, align).unwrap_or_else(|_| {
        std::alloc::Layout::from_size_align(8, 8).unwrap()
    });
    unsafe { std::alloc::alloc(layout) }
}
