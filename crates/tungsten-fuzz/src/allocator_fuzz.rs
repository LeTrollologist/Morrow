use crate::rng::Rng;
use tungsten_codegen::runtime::PhysicalArena;

struct LiveAllocation {
    ptr: *mut u8,
    size: usize,
    align: usize,
    seed_byte: u8,
}

pub fn fuzz_physical_arena(iterations: usize, seed: u64) {
    let mut rng = Rng::seed(seed);
    let mut arena = PhysicalArena::new();
    let mut live_allocations: Vec<LiveAllocation> = Vec::new();

    let alignment_candidates = [1, 2, 4, 8, 16, 32, 64, 128, 256, 512, 1024, 2048];
    let special_sizes = [0, 1, 2, 3, 7, 8, 15, 16, 24, 32, 64, 4095, 4096, 4097, 8191, 8192, 16384];

    for i in 0..iterations {
        let align = *rng.choose(&alignment_candidates);
        let size = if rng.gen_bool(0.3) {
            *rng.choose(&special_sizes)
        } else {
            rng.gen_range(1, 1024)
        };

        let ptr = arena.alloc(size, align);
        assert!(!ptr.is_null(), "PhysicalArena returned null pointer on allocation #{}", i);

        // Invariant 1: Pointer must be mathematically aligned to requested alignment
        assert_eq!(
            (ptr as usize) % align,
            0,
            "Allocation #{} (size {}, align {}) failed alignment: address 0x{:X}",
            i,
            size,
            align,
            ptr as usize
        );

        // Fill memory with canary pattern
        let seed_byte = (i as u8).wrapping_mul(37).wrapping_add(11);
        if size > 0 {
            unsafe {
                for offset in 0..size {
                    let expected = seed_byte.wrapping_add(offset as u8);
                    *ptr.add(offset) = expected;
                }
            }
        }

        live_allocations.push(LiveAllocation {
            ptr,
            size,
            align,
            seed_byte,
        });

        // Periodically verify all previous allocations have NOT been corrupted
        if i % 100 == 0 || i == iterations - 1 {
            for (idx, alloc) in live_allocations.iter().enumerate() {
                // Re-verify alignment
                assert_eq!(
                    (alloc.ptr as usize) % alloc.align,
                    0,
                    "Allocation #{} alignment corrupted",
                    idx
                );

                // Re-verify canary memory
                if alloc.size > 0 {
                    unsafe {
                        for offset in 0..alloc.size {
                            let actual = *alloc.ptr.add(offset);
                            let expected = alloc.seed_byte.wrapping_add(offset as u8);
                            assert_eq!(
                                actual, expected,
                                "Memory corruption detected in allocation #{} at offset {}! Expected 0x{:02X}, found 0x{:02X}",
                                idx, offset, expected, actual
                            );
                        }
                    }
                }
            }
        }
    }

    arena.destroy();
}

pub fn fuzz_interleaved_arenas(num_cycles: usize, seed: u64) {
    let mut rng = Rng::seed(seed);

    for _cycle in 0..num_cycles {
        let mut parent_arena = PhysicalArena::new();
        let parent_ptr = parent_arena.alloc(128, 16);
        assert_eq!((parent_ptr as usize) % 16, 0);

        unsafe {
            std::ptr::write_bytes(parent_ptr, 0x11, 128);
        }

        // Child arena lifetime nested inside parent
        {
            let mut child_arena = PhysicalArena::new();
            for _ in 0..50 {
                let sz = rng.gen_range(8, 256);
                let al = *rng.choose(&[1, 8, 32, 64, 128]);
                let c_ptr = child_arena.alloc(sz, al);
                assert_eq!((c_ptr as usize) % al, 0);
                unsafe {
                    std::ptr::write_bytes(c_ptr, 0x22, sz);
                    assert_eq!(*c_ptr, 0x22);
                }
            }
            child_arena.destroy();
        }

        // Parent arena must remain completely valid after child destruction
        unsafe {
            assert_eq!(*parent_ptr, 0x11);
            assert_eq!(*parent_ptr.add(127), 0x11);
        }

        // Parent can continue allocating
        let parent_ptr2 = parent_arena.alloc(512, 64);
        assert_eq!((parent_ptr2 as usize) % 64, 0);

        parent_arena.destroy();
    }
}

pub fn fuzz_extreme_allocations() {
    let mut arena = PhysicalArena::new();

    // 1. Extreme near-usize::MAX allocations must not cause panic or arithmetic wrap-around
    let extreme_sizes = [
        usize::MAX,
        usize::MAX - 1,
        usize::MAX - 16,
        usize::MAX - 4096,
        (isize::MAX as usize) + 1,
    ];

    for &size in &extreme_sizes {
        for &align in &[1, 8, 16, 64, 128] {
            let ptr = arena.alloc(size, align);
            // It must safely return null without crashing
            assert!(ptr.is_null(), "Extreme allocation of size {} should fail safely and return null", size);
        }
    }

    // 2. Arena must still function normally after failed extreme allocation attempts
    let normal_ptr = arena.alloc(64, 8);
    assert!(!normal_ptr.is_null());
    assert_eq!((normal_ptr as usize) % 8, 0);
    unsafe {
        std::ptr::write_bytes(normal_ptr, 0xEE, 64);
        assert_eq!(*normal_ptr, 0xEE);
        assert_eq!(*normal_ptr.add(63), 0xEE);
    }

    arena.destroy();
}

