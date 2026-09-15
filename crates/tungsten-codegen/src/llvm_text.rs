use std::collections::HashMap;
use tungsten_syntax::ast::BinOp;
use tungsten_tir::ir::*;
use tungsten_typeck::types::Type;

pub struct LlvmTextEmitter<'a> {
    module: &'a TirModule,
    out: String,
    strings: Vec<String>,
    string_map: HashMap<String, usize>,
    temp_counter: usize,
    var_types: HashMap<Var, String>,
}

impl<'a> LlvmTextEmitter<'a> {
    pub fn new(module: &'a TirModule) -> Self {
        Self {
            module,
            out: String::new(),
            strings: Vec::new(),
            string_map: HashMap::new(),
            temp_counter: 0,
            var_types: HashMap::new(),
        }
    }

    fn next_temp(&mut self) -> String {
        let id = self.temp_counter;
        self.temp_counter += 1;
        format!("%t{}", id)
    }

    fn intern_string(&mut self, s: &str) -> usize {
        if let Some(&idx) = self.string_map.get(s) {
            idx
        } else {
            let idx = self.strings.len();
            self.strings.push(s.to_string());
            self.string_map.insert(s.to_string(), idx);
            idx
        }
    }

    pub fn emit(mut self) -> String {
        // Collect string literals first
        for func in &self.module.functions {
            for block in &func.blocks {
                for inst in &block.instructions {
                    self.collect_strings_from_inst(inst);
                }
            }
        }

        let mut header = String::new();
        header.push_str("; ==========================================================\n");
        header.push_str("; Tungsten Native LLVM Textual IR\n");
        header.push_str("; Zero-cost compilation target with auto-SIMD and LTO support\n");
        header.push_str("; ==========================================================\n\n");
        header.push_str("target datalayout = \"e-m:w-p270:32:32-p271:32:32-p272:64:64-i64:64-i128:128-f80:128-n8:16:32:64-S128\"\n");
        header.push_str("target triple = \"x86_64-pc-windows-gnu\"\n\n");

        // External C declarations
        header.push_str("; C Library Runtime Declarations\n");
        header.push_str("declare i32 @printf(ptr, ...)\n");
        header.push_str("declare i32 @putchar(i32)\n");
        header.push_str("declare void @exit(i32)\n");
        header.push_str("declare ptr @malloc(i64)\n");
        header.push_str("declare ptr @realloc(ptr, i64)\n");
        header.push_str("declare void @free(ptr)\n");
        header.push_str("declare void @Sleep(i32)\n");
        header.push_str("declare i32 @WSAStartup(i16, ptr)\n");
        header.push_str("declare i64 @socket(i32, i32, i32)\n");
        header.push_str("declare i32 @bind(i64, ptr, i32)\n");
        header.push_str("declare i32 @listen(i64, i32)\n");
        header.push_str("declare i64 @accept(i64, ptr, ptr)\n");
        header.push_str("declare i32 @recv(i64, ptr, i32, i32)\n");
        header.push_str("declare i32 @send(i64, ptr, i32, i32)\n");
        header.push_str("declare i32 @closesocket(i64)\n");
        header.push_str("declare i32 @setsockopt(i64, i32, i32, ptr, i32)\n");
        header.push_str("declare i64 @strlen(ptr)\n");
        header.push_str("declare ptr @CreateThread(ptr, i64, ptr, ptr, i32, ptr)\n");
        header.push_str("declare i32 @CloseHandle(ptr)\n");
        header.push_str("declare ptr @CreateSemaphoreA(ptr, i32, i32, ptr)\n");
        header.push_str("declare ptr @CreateMutexA(ptr, i32, ptr)\n");
        header.push_str("declare i32 @WaitForSingleObject(ptr, i32)\n");
        header.push_str("declare i32 @ReleaseSemaphore(ptr, i32, ptr)\n");
        header.push_str("declare i32 @ReleaseMutex(ptr)\n\n");

        // Dynamic C declarations from extern_blocks
        let mut declared_c_fns: std::collections::HashSet<String> = [
            "printf", "putchar", "exit", "malloc", "realloc", "free", "Sleep",
            "WSAStartup", "socket", "bind", "listen", "accept", "recv", "send",
            "closesocket", "setsockopt", "strlen", "CreateThread", "CloseHandle",
            "CreateSemaphoreA", "CreateMutexA", "WaitForSingleObject",
            "ReleaseSemaphore", "ReleaseMutex"
        ].iter().map(|s| s.to_string()).collect();

        for block in &self.module.extern_blocks {
            for f in &block.fns {
                if declared_c_fns.insert(f.name.clone()) {
                    let ret_str = type_expr_to_llvm_str(&f.ret, true);
                    let param_strs: Vec<String> = f.params.iter().map(|(_, pty)| type_expr_to_llvm_str(pty, false)).collect();
                    header.push_str(&format!("declare {} @{}({})\n", ret_str, f.name, param_strs.join(", ")));
                }
            }
        }
        header.push('\n');

        // Format strings
        header.push_str("; Global Format Strings\n");
        header.push_str("@fmt_i64 = internal constant [6 x i8] c\"%lld\\0A\\00\"\n");
        header.push_str("@fmt_i64_raw = internal constant [5 x i8] c\"%lld\\00\"\n");
        header.push_str("@fmt_str = internal constant [4 x i8] c\"%s\\0A\\00\"\n");
        header.push_str("@fmt_str_raw = internal constant [3 x i8] c\"%s\\00\"\n");
        header.push_str("@fmt_io_str = internal constant [9 x i8] c\"[IO] %s\\0A\\00\"\n");
        header.push_str("@fmt_io_empty = internal constant [6 x i8] c\"[IO]\\0A\\00\"\n");
        header.push_str("@panic_fmt = internal constant [57 x i8] c\"\\0A[Tungsten Refinement Panic]: %lld outside [%lld, %lld]\\0A\\00\"\n\n");

        // Tungsten Runtime Functions (in pure LLVM IR!)
        header.push_str("; Tungsten Native Runtime Functions\n");
        header.push_str("define void @tungsten_print_i64(i64 %v) {\n");
        header.push_str("    call i32 (ptr, ...) @printf(ptr @fmt_i64_raw, i64 %v)\n");
        header.push_str("    ret void\n}\n\n");

        header.push_str("define void @tungsten_println_i64(i64 %v) {\n");
        header.push_str("    call i32 (ptr, ...) @printf(ptr @fmt_i64, i64 %v)\n");
        header.push_str("    ret void\n}\n\n");

        header.push_str("define void @tungsten_print_str(ptr %s, i64 %len) {\n");
        header.push_str("    %null_check = icmp eq ptr %s, null\n");
        header.push_str("    br i1 %null_check, label %ret_blk, label %print_blk\n");
        header.push_str("print_blk:\n");
        header.push_str("    call i32 (ptr, ...) @printf(ptr @fmt_str_raw, ptr %s)\n");
        header.push_str("    br label %ret_blk\n");
        header.push_str("ret_blk:\n    ret void\n}\n\n");

        header.push_str("define void @tungsten_println_str(ptr %s, i64 %len) {\n");
        header.push_str("    %null_check = icmp eq ptr %s, null\n");
        header.push_str("    br i1 %null_check, label %empty_blk, label %print_blk\n");
        header.push_str("print_blk:\n");
        header.push_str("    call i32 (ptr, ...) @printf(ptr @fmt_str, ptr %s)\n");
        header.push_str("    ret void\n");
        header.push_str("empty_blk:\n");
        header.push_str("    call i32 @putchar(i32 10)\n");
        header.push_str("    ret void\n}\n\n");

        header.push_str("define void @tungsten_io_print(ptr %s, i64 %len) {\n");
        header.push_str("    %null_check = icmp eq ptr %s, null\n");
        header.push_str("    br i1 %null_check, label %empty_blk, label %print_blk\n");
        header.push_str("print_blk:\n");
        header.push_str("    call i32 (ptr, ...) @printf(ptr @fmt_io_str, ptr %s)\n");
        header.push_str("    ret void\n");
        header.push_str("empty_blk:\n");
        header.push_str("    call i32 (ptr, ...) @printf(ptr @fmt_io_empty)\n");
        header.push_str("    ret void\n}\n\n");

        header.push_str("define void @tungsten_refinement_panic(i64 %val, i64 %min_v, i64 %max_v) {\n");
        header.push_str("    call i32 (ptr, ...) @printf(ptr @panic_fmt, i64 %val, i64 %min_v, i64 %max_v)\n");
        header.push_str("    call void @exit(i32 101)\n");
        header.push_str("    unreachable\n}\n\n");

        header.push_str("define ptr @tungsten_alloc(i64 %size, i64 %align) {\n");
        header.push_str("    %is_zero = icmp eq i64 %size, 0\n");
        header.push_str("    %alloc_sz = select i1 %is_zero, i64 8, i64 %size\n");
        header.push_str("    %p = call ptr @malloc(i64 %alloc_sz)\n");
        header.push_str("    ret ptr %p\n}\n\n");

        // Region Allocator in pure LLVM IR (Arena: { ptr buf, i64 offset, i64 capacity })
        header.push_str("define ptr @tungsten_region_enter() {\n");
        header.push_str("    %arena = call ptr @malloc(i64 24)\n");
        header.push_str("    %buf = call ptr @malloc(i64 8192)\n");
        header.push_str("    store ptr %buf, ptr %arena\n");
        header.push_str("    %offset_ptr = getelementptr inbounds i8, ptr %arena, i64 8\n");
        header.push_str("    store i64 0, ptr %offset_ptr\n");
        header.push_str("    %cap_ptr = getelementptr inbounds i8, ptr %arena, i64 16\n");
        header.push_str("    store i64 8192, ptr %cap_ptr\n");
        header.push_str("    ret ptr %arena\n}\n\n");

        header.push_str("define ptr @tungsten_region_alloc(ptr %arena, i64 %size, i64 %align) {\n");
        header.push_str("    %offset_ptr = getelementptr inbounds i8, ptr %arena, i64 8\n");
        header.push_str("    %offset = load i64, ptr %offset_ptr\n");
        header.push_str("    %buf_ptr = load ptr, ptr %arena\n");
        header.push_str("    %ptr = getelementptr inbounds i8, ptr %buf_ptr, i64 %offset\n");
        header.push_str("    %new_offset = add i64 %offset, %size\n");
        header.push_str("    store i64 %new_offset, ptr %offset_ptr\n");
        header.push_str("    ret ptr %ptr\n}\n\n");

        header.push_str("define void @tungsten_region_exit(ptr %arena) {\n");
        header.push_str("    %is_null = icmp eq ptr %arena, null\n");
        header.push_str("    br i1 %is_null, label %done, label %free_blk\n");
        header.push_str("free_blk:\n");
        header.push_str("    %buf = load ptr, ptr %arena\n");
        header.push_str("    call void @free(ptr %buf)\n");
        header.push_str("    call void @free(ptr %arena)\n");
        header.push_str("    br label %done\n");
        header.push_str("done:\n    ret void\n}\n\n");

        header.push_str("define void @tungsten_trace_effect(ptr %eff, i64 %elen, ptr %op, i64 %olen) {\n    ret void\n}\n\n");
        header.push_str("define i32 @tungsten_thread_thunk(ptr %param) {\n");
        header.push_str("    %fn_slot = getelementptr inbounds i8, ptr %param, i64 0\n");
        header.push_str("    %fn_ptr = load ptr, ptr %fn_slot\n");
        header.push_str("    %a1_slot = getelementptr inbounds i8, ptr %param, i64 8\n");
        header.push_str("    %a1 = load i64, ptr %a1_slot\n");
        header.push_str("    %a2_slot = getelementptr inbounds i8, ptr %param, i64 16\n");
        header.push_str("    %a2 = load i64, ptr %a2_slot\n");
        header.push_str("    call void %fn_ptr(i64 %a1, i64 %a2)\n");
        header.push_str("    call void @free(ptr %param)\n");
        header.push_str("    ret i32 0\n}\n\n");
        header.push_str("define i64 @tungsten_fiber_spawn(ptr %fn_ptr, i64 %a1, i64 %a2) {\n");
        header.push_str("    %ctx = call ptr @malloc(i64 24)\n");
        header.push_str("    %fn_slot = getelementptr inbounds i8, ptr %ctx, i64 0\n");
        header.push_str("    store ptr %fn_ptr, ptr %fn_slot\n");
        header.push_str("    %a1_slot = getelementptr inbounds i8, ptr %ctx, i64 8\n");
        header.push_str("    store i64 %a1, ptr %a1_slot\n");
        header.push_str("    %a2_slot = getelementptr inbounds i8, ptr %ctx, i64 16\n");
        header.push_str("    store i64 %a2, ptr %a2_slot\n");
        header.push_str("    %th = call ptr @CreateThread(ptr null, i64 0, ptr @tungsten_thread_thunk, ptr %ctx, i32 0, ptr null)\n");
        header.push_str("    %is_null = icmp eq ptr %th, null\n");
        header.push_str("    br i1 %is_null, label %spawn_done, label %close_th\n");
        header.push_str("close_th:\n");
        header.push_str("    call i32 @CloseHandle(ptr %th)\n");
        header.push_str("    br label %spawn_done\n");
        header.push_str("spawn_done:\n");
        header.push_str("    ret i64 1\n}\n\n");
        header.push_str("define void @tungsten_fiber_yield() {\n    call void @Sleep(i32 0)\n    ret void\n}\n\n");
        header.push_str("define void @tungsten_fiber_sleep(i64 %ms) {\n    %trunc = trunc i64 %ms to i32\n    call void @Sleep(i32 %trunc)\n    ret void\n}\n\n");
        header.push_str("define ptr @tungsten_nursery_enter() {\n    %nur = call ptr @malloc(i64 16)\n    store i64 0, ptr %nur\n    ret ptr %nur\n}\n\n");
        header.push_str("define i32 @tungsten_nursery_thread_thunk(ptr %param) {\n");
        header.push_str("    %fn_slot = getelementptr inbounds i8, ptr %param, i64 0\n");
        header.push_str("    %fn_ptr = load ptr, ptr %fn_slot\n");
        header.push_str("    %a1_slot = getelementptr inbounds i8, ptr %param, i64 8\n");
        header.push_str("    %a1 = load i64, ptr %a1_slot\n");
        header.push_str("    %a2_slot = getelementptr inbounds i8, ptr %param, i64 16\n");
        header.push_str("    %a2 = load i64, ptr %a2_slot\n");
        header.push_str("    %nur_slot = getelementptr inbounds i8, ptr %param, i64 24\n");
        header.push_str("    %nursery = load ptr, ptr %nur_slot\n");
        header.push_str("    call void %fn_ptr(i64 %a1, i64 %a2)\n");
        header.push_str("    %old = atomicrmw sub ptr %nursery, i64 1 seq_cst, align 8\n");
        header.push_str("    call void @free(ptr %param)\n");
        header.push_str("    ret i32 0\n}\n\n");
        header.push_str("define i64 @tungsten_nursery_spawn(ptr %nursery, ptr %fn_ptr, i64 %a1, i64 %a2) {\n");
        header.push_str("    %old = atomicrmw add ptr %nursery, i64 1 seq_cst, align 8\n");
        header.push_str("    %ctx = call ptr @malloc(i64 32)\n");
        header.push_str("    %fn_slot = getelementptr inbounds i8, ptr %ctx, i64 0\n");
        header.push_str("    store ptr %fn_ptr, ptr %fn_slot\n");
        header.push_str("    %a1_slot = getelementptr inbounds i8, ptr %ctx, i64 8\n");
        header.push_str("    store i64 %a1, ptr %a1_slot\n");
        header.push_str("    %a2_slot = getelementptr inbounds i8, ptr %ctx, i64 16\n");
        header.push_str("    store i64 %a2, ptr %a2_slot\n");
        header.push_str("    %nur_slot = getelementptr inbounds i8, ptr %ctx, i64 24\n");
        header.push_str("    store ptr %nursery, ptr %nur_slot\n");
        header.push_str("    %th = call ptr @CreateThread(ptr null, i64 0, ptr @tungsten_nursery_thread_thunk, ptr %ctx, i32 0, ptr null)\n");
        header.push_str("    %is_null = icmp eq ptr %th, null\n");
        header.push_str("    br i1 %is_null, label %spawn_done, label %close_th\n");
        header.push_str("close_th:\n");
        header.push_str("    call i32 @CloseHandle(ptr %th)\n");
        header.push_str("    br label %spawn_done\n");
        header.push_str("spawn_done:\n");
        header.push_str("    ret i64 1\n}\n\n");
        header.push_str("define void @tungsten_nursery_wait_all(ptr %nursery) {\nentry:\n");
        header.push_str("    %is_null = icmp eq ptr %nursery, null\n");
        header.push_str("    br i1 %is_null, label %done, label %poll_loop\n");
        header.push_str("poll_loop:\n");
        header.push_str("    %cnt = load atomic i64, ptr %nursery acquire, align 8\n");
        header.push_str("    %is_zero = icmp eq i64 %cnt, 0\n");
        header.push_str("    br i1 %is_zero, label %free_nur, label %yield_sleep\n");
        header.push_str("yield_sleep:\n");
        header.push_str("    call void @Sleep(i32 0)\n");
        header.push_str("    br label %poll_loop\n");
        header.push_str("free_nur:\n");
        header.push_str("    call void @free(ptr %nursery)\n");
        header.push_str("    br label %done\n");
        header.push_str("done:\n    ret void\n}\n\n");

        header.push_str("define i64 @tungsten_channel_bounded(i64 %cap) {\n");
        header.push_str("    %ch = call ptr @malloc(i64 64)\n");
        header.push_str("    %buf_sz = mul i64 %cap, 8\n");
        header.push_str("    %buf = call ptr @malloc(i64 %buf_sz)\n");
        header.push_str("    store ptr %buf, ptr %ch\n");
        header.push_str("    %head_ptr = getelementptr inbounds i8, ptr %ch, i64 8\n");
        header.push_str("    store i64 0, ptr %head_ptr\n");
        header.push_str("    %tail_ptr = getelementptr inbounds i8, ptr %ch, i64 16\n");
        header.push_str("    store i64 0, ptr %tail_ptr\n");
        header.push_str("    %cnt_ptr = getelementptr inbounds i8, ptr %ch, i64 24\n");
        header.push_str("    store i64 0, ptr %cnt_ptr\n");
        header.push_str("    %cap_ptr = getelementptr inbounds i8, ptr %ch, i64 32\n");
        header.push_str("    store i64 %cap, ptr %cap_ptr\n");
        header.push_str("    %cap32 = trunc i64 %cap to i32\n");
        header.push_str("    %sem_items = call ptr @CreateSemaphoreA(ptr null, i32 0, i32 %cap32, ptr null)\n");
        header.push_str("    %items_ptr = getelementptr inbounds i8, ptr %ch, i64 40\n");
        header.push_str("    store ptr %sem_items, ptr %items_ptr\n");
        header.push_str("    %sem_slots = call ptr @CreateSemaphoreA(ptr null, i32 %cap32, i32 %cap32, ptr null)\n");
        header.push_str("    %slots_ptr = getelementptr inbounds i8, ptr %ch, i64 48\n");
        header.push_str("    store ptr %sem_slots, ptr %slots_ptr\n");
        header.push_str("    %mtx = call ptr @CreateMutexA(ptr null, i32 0, ptr null)\n");
        header.push_str("    %mtx_ptr = getelementptr inbounds i8, ptr %ch, i64 56\n");
        header.push_str("    store ptr %mtx, ptr %mtx_ptr\n");
        header.push_str("    %res = ptrtoint ptr %ch to i64\n");
        header.push_str("    ret i64 %res\n}\n\n");

        header.push_str("define i64 @tungsten_channel_new() {\n    %res = call i64 @tungsten_channel_bounded(i64 1024)\n    ret i64 %res\n}\n\n");

        header.push_str("define void @tungsten_channel_send(i64 %cid, i64 %val) {\n");
        header.push_str("    %ch = inttoptr i64 %cid to ptr\n");
        header.push_str("    %slots_ptr = getelementptr inbounds i8, ptr %ch, i64 48\n");
        header.push_str("    %sem_slots = load ptr, ptr %slots_ptr\n");
        header.push_str("    call i32 @WaitForSingleObject(ptr %sem_slots, i32 -1)\n");
        header.push_str("    %mtx_ptr = getelementptr inbounds i8, ptr %ch, i64 56\n");
        header.push_str("    %mtx = load ptr, ptr %mtx_ptr\n");
        header.push_str("    call i32 @WaitForSingleObject(ptr %mtx, i32 -1)\n");
        header.push_str("    %buf = load ptr, ptr %ch\n");
        header.push_str("    %tail_ptr = getelementptr inbounds i8, ptr %ch, i64 16\n");
        header.push_str("    %tail = load i64, ptr %tail_ptr\n");
        header.push_str("    %slot_ptr = getelementptr inbounds i64, ptr %buf, i64 %tail\n");
        header.push_str("    store i64 %val, ptr %slot_ptr\n");
        header.push_str("    %cap_ptr = getelementptr inbounds i8, ptr %ch, i64 32\n");
        header.push_str("    %cap = load i64, ptr %cap_ptr\n");
        header.push_str("    %next_tail = add i64 %tail, 1\n");
        header.push_str("    %rem_tail = urem i64 %next_tail, %cap\n");
        header.push_str("    store i64 %rem_tail, ptr %tail_ptr\n");
        header.push_str("    %cnt_ptr = getelementptr inbounds i8, ptr %ch, i64 24\n");
        header.push_str("    %cnt = load i64, ptr %cnt_ptr\n");
        header.push_str("    %next_cnt = add i64 %cnt, 1\n");
        header.push_str("    store i64 %next_cnt, ptr %cnt_ptr\n");
        header.push_str("    call i32 @ReleaseMutex(ptr %mtx)\n");
        header.push_str("    %items_ptr = getelementptr inbounds i8, ptr %ch, i64 40\n");
        header.push_str("    %sem_items = load ptr, ptr %items_ptr\n");
        header.push_str("    call i32 @ReleaseSemaphore(ptr %sem_items, i32 1, ptr null)\n");
        header.push_str("    ret void\n}\n\n");

        header.push_str("define i64 @tungsten_channel_recv(i64 %cid) {\n");
        header.push_str("    %ch = inttoptr i64 %cid to ptr\n");
        header.push_str("    %items_ptr = getelementptr inbounds i8, ptr %ch, i64 40\n");
        header.push_str("    %sem_items = load ptr, ptr %items_ptr\n");
        header.push_str("    call i32 @WaitForSingleObject(ptr %sem_items, i32 -1)\n");
        header.push_str("    %mtx_ptr = getelementptr inbounds i8, ptr %ch, i64 56\n");
        header.push_str("    %mtx = load ptr, ptr %mtx_ptr\n");
        header.push_str("    call i32 @WaitForSingleObject(ptr %mtx, i32 -1)\n");
        header.push_str("    %buf = load ptr, ptr %ch\n");
        header.push_str("    %head_ptr = getelementptr inbounds i8, ptr %ch, i64 8\n");
        header.push_str("    %head = load i64, ptr %head_ptr\n");
        header.push_str("    %slot_ptr = getelementptr inbounds i64, ptr %buf, i64 %head\n");
        header.push_str("    %val = load i64, ptr %slot_ptr\n");
        header.push_str("    %cap_ptr = getelementptr inbounds i8, ptr %ch, i64 32\n");
        header.push_str("    %cap = load i64, ptr %cap_ptr\n");
        header.push_str("    %next_head = add i64 %head, 1\n");
        header.push_str("    %rem_head = urem i64 %next_head, %cap\n");
        header.push_str("    store i64 %rem_head, ptr %head_ptr\n");
        header.push_str("    %cnt_ptr = getelementptr inbounds i8, ptr %ch, i64 24\n");
        header.push_str("    %cnt = load i64, ptr %cnt_ptr\n");
        header.push_str("    %next_cnt = sub i64 %cnt, 1\n");
        header.push_str("    store i64 %next_cnt, ptr %cnt_ptr\n");
        header.push_str("    call i32 @ReleaseMutex(ptr %mtx)\n");
        header.push_str("    %slots_ptr = getelementptr inbounds i8, ptr %ch, i64 48\n");
        header.push_str("    %sem_slots = load ptr, ptr %slots_ptr\n");
        header.push_str("    call i32 @ReleaseSemaphore(ptr %sem_slots, i32 1, ptr null)\n");
        header.push_str("    ret i64 %val\n}\n\n");

        header.push_str("define i64 @tungsten_net_listen(i64 %port) {\n");
        header.push_str("    %wsa_buf = alloca [400 x i8]\n");
        header.push_str("    call i32 @WSAStartup(i16 514, ptr %wsa_buf)\n");
        header.push_str("    %sock = call i64 @socket(i32 2, i32 1, i32 6)\n");
        header.push_str("    %opt_val = alloca i32\n");
        header.push_str("    store i32 1, ptr %opt_val\n");
        header.push_str("    call i32 @setsockopt(i64 %sock, i32 65535, i32 4, ptr %opt_val, i32 4)\n");
        header.push_str("    %addr = alloca [16 x i8]\n");
        header.push_str("    store i16 2, ptr %addr\n");
        header.push_str("    %p_lo = and i64 %port, 255\n");
        header.push_str("    %p_sh = shl i64 %p_lo, 8\n");
        header.push_str("    %p_hi = lshr i64 %port, 8\n");
        header.push_str("    %p_hi_m = and i64 %p_hi, 255\n");
        header.push_str("    %net_port = or i64 %p_sh, %p_hi_m\n");
        header.push_str("    %net_port16 = trunc i64 %net_port to i16\n");
        header.push_str("    %port_ptr = getelementptr inbounds i8, ptr %addr, i64 2\n");
        header.push_str("    store i16 %net_port16, ptr %port_ptr\n");
        header.push_str("    %addr_ptr = getelementptr inbounds i8, ptr %addr, i64 4\n");
        header.push_str("    store i32 0, ptr %addr_ptr\n");
        header.push_str("    %zero_ptr = getelementptr inbounds i8, ptr %addr, i64 8\n");
        header.push_str("    store i64 0, ptr %zero_ptr\n");
        header.push_str("    call i32 @bind(i64 %sock, ptr %addr, i32 16)\n");
        header.push_str("    call i32 @listen(i64 %sock, i32 128)\n");
        header.push_str("    ret i64 %sock\n}\n\n");

        header.push_str("define i64 @tungsten_net_accept(i64 %listener) {\n");
        header.push_str("    %conn = call i64 @accept(i64 %listener, ptr null, ptr null)\n");
        header.push_str("    ret i64 %conn\n}\n\n");

        header.push_str("define i64 @tungsten_net_connect(ptr %host, i64 %port) {\n    ret i64 0\n}\n\n");

        header.push_str("define ptr @tungsten_net_read(i64 %conn, i64 %max_len) {\n");
        header.push_str("    %buf_sz = add i64 %max_len, 1\n");
        header.push_str("    %buf = call ptr @malloc(i64 %buf_sz)\n");
        header.push_str("    %trunc_len = trunc i64 %max_len to i32\n");
        header.push_str("    %n = call i32 @recv(i64 %conn, ptr %buf, i32 %trunc_len, i32 0)\n");
        header.push_str("    %n_is_neg = icmp slt i32 %n, 0\n");
        header.push_str("    %n_bytes = select i1 %n_is_neg, i32 0, i32 %n\n");
        header.push_str("    %n_i64 = sext i32 %n_bytes to i64\n");
        header.push_str("    %term_ptr = getelementptr inbounds i8, ptr %buf, i64 %n_i64\n");
        header.push_str("    store i8 0, ptr %term_ptr\n");
        header.push_str("    ret ptr %buf\n}\n\n");

        header.push_str("define i64 @tungsten_net_write(i64 %conn, ptr %data, i64 %len) {\n");
        header.push_str("    %str_len = call i64 @strlen(ptr %data)\n");
        header.push_str("    %trunc_len = trunc i64 %str_len to i32\n");
        header.push_str("    %res = call i32 @send(i64 %conn, ptr %data, i32 %trunc_len, i32 0)\n");
        header.push_str("    %res_i64 = sext i32 %res to i64\n");
        header.push_str("    ret i64 %res_i64\n}\n\n");

        header.push_str("define void @tungsten_net_close(i64 %conn) {\n");
        header.push_str("    call i32 @closesocket(i64 %conn)\n");
        header.push_str("    ret void\n}\n\n");

        header.push_str("define i32 @tungsten_foreign_call_thunk(ptr %ctx) {\n");
        header.push_str("    %fn_slot = getelementptr inbounds i8, ptr %ctx, i64 0\n");
        header.push_str("    %fn_ptr = load ptr, ptr %fn_slot\n");
        header.push_str("    %a1_slot = getelementptr inbounds i8, ptr %ctx, i64 8\n");
        header.push_str("    %a1 = load i64, ptr %a1_slot\n");
        header.push_str("    %res = call i64 %fn_ptr(i64 %a1)\n");
        header.push_str("    %res_slot = getelementptr inbounds i8, ptr %ctx, i64 16\n");
        header.push_str("    store i64 %res, ptr %res_slot\n");
        header.push_str("    %sem_slot = getelementptr inbounds i8, ptr %ctx, i64 24\n");
        header.push_str("    %sem = load ptr, ptr %sem_slot\n");
        header.push_str("    call i32 @ReleaseSemaphore(ptr %sem, i32 1, ptr null)\n");
        header.push_str("    ret i32 0\n}\n\n");

        header.push_str("define i64 @tungsten_foreign_call_offload(ptr %fn_ptr, i64 %a1) {\n");
        header.push_str("    %sem = call ptr @CreateSemaphoreA(ptr null, i32 0, i32 1, ptr null)\n");
        header.push_str("    %ctx = call ptr @malloc(i64 32)\n");
        header.push_str("    %fn_slot = getelementptr inbounds i8, ptr %ctx, i64 0\n");
        header.push_str("    store ptr %fn_ptr, ptr %fn_slot\n");
        header.push_str("    %a1_slot = getelementptr inbounds i8, ptr %ctx, i64 8\n");
        header.push_str("    store i64 %a1, ptr %a1_slot\n");
        header.push_str("    %sem_slot = getelementptr inbounds i8, ptr %ctx, i64 24\n");
        header.push_str("    store ptr %sem, ptr %sem_slot\n");
        header.push_str("    %th = call ptr @CreateThread(ptr null, i64 0, ptr @tungsten_foreign_call_thunk, ptr %ctx, i32 0, ptr null)\n");
        header.push_str("    br label %poll\n");
        header.push_str("poll:\n");
        header.push_str("    %w = call i32 @WaitForSingleObject(ptr %sem, i32 0)\n");
        header.push_str("    %done = icmp eq i32 %w, 0\n");
        header.push_str("    br i1 %done, label %finish, label %yield_poll\n");
        header.push_str("yield_poll:\n");
        header.push_str("    call void @Sleep(i32 0)\n");
        header.push_str("    br label %poll\n");
        header.push_str("finish:\n");
        header.push_str("    call i32 @CloseHandle(ptr %th)\n");
        header.push_str("    call i32 @CloseHandle(ptr %sem)\n");
        header.push_str("    %res_slot = getelementptr inbounds i8, ptr %ctx, i64 16\n");
        header.push_str("    %res = load i64, ptr %res_slot\n");
        header.push_str("    call void @free(ptr %ctx)\n");
        header.push_str("    ret i64 %res\n}\n\n");

        // Emit interned string constants
        header.push_str("; User String Literals\n");
        for (i, s) in self.strings.iter().enumerate() {
            let escaped = escape_llvm_string(s);
            let len = s.len() + 1;
            header.push_str(&format!("@str_{} = internal constant [{} x i8] c\"{}\\00\"\n", i, len, escaped));
        }
        header.push('\n');

        // Functions
        for func in &self.module.functions {
            self.emit_function(func);
        }

        let mut full = header;
        full.push_str(&self.out);
        full
    }

    fn collect_strings_from_inst(&mut self, inst: &Instruction) {
        match inst {
            Instruction::Assign { rvalue, .. } => {
                self.collect_strings_from_rvalue(rvalue);
            }
            Instruction::PerformEffect { effect, op, args, .. } => {
                self.intern_string(effect);
                self.intern_string(op);
                self.intern_string("PlayerOne");
                for a in args {
                    self.collect_strings_from_op(a);
                }
            }
            Instruction::Call { func, args, .. } => {
                self.collect_strings_from_op(func);
                let func_name = match func {
                    Operand::Var(Var::Named(name), _) => name.clone(),
                    Operand::Constant(TirConstant::Str(name)) => name.clone(),
                    _ => "".to_string(),
                };
                if func_name == "@println" {
                    if let Some(Operand::Constant(TirConstant::Str(ref fmt_str))) = args.first() {
                        self.intern_string(fmt_str);
                        for part in fmt_str.split("{}") {
                            if !part.is_empty() {
                                self.intern_string(part);
                            }
                        }
                    }
                }
                for a in args {
                    self.collect_strings_from_op(a);
                }
            }
            Instruction::SetField { val, .. } => {
                self.collect_strings_from_op(val);
            }
            Instruction::ExternCall { args, .. } => {
                for a in args {
                    self.collect_strings_from_op(a);
                }
            }
            Instruction::Store { ptr, value, .. } => {
                self.collect_strings_from_op(ptr);
                self.collect_strings_from_op(value);
            }
            _ => {}
        }
    }

    fn collect_strings_from_rvalue(&mut self, rv: &RValue) {
        match rv {
            RValue::Use(op) | RValue::Cast { operand: op, .. } | RValue::Ref { operand: op, .. } | RValue::Deref(op) | RValue::AddrOf(op) => {
                self.collect_strings_from_op(op);
            }
            RValue::BinaryOp(_, l, r) => {
                self.collect_strings_from_op(l);
                self.collect_strings_from_op(r);
            }
            RValue::FieldAccess { target, .. } => {
                self.collect_strings_from_op(target);
            }
            RValue::MethodCall { target, args, .. } => {
                self.collect_strings_from_op(target);
                for a in args {
                    self.collect_strings_from_op(a);
                }
            }
            RValue::StructInit { fields, arena, .. } => {
                for (_, f) in fields {
                    self.collect_strings_from_op(f);
                }
                if let Some(a) = arena {
                    self.collect_strings_from_op(a);
                }
            }
            RValue::EnumInit { payload, arena, .. } => {
                for p in payload {
                    self.collect_strings_from_op(p);
                }
                if let Some(a) = arena {
                    self.collect_strings_from_op(a);
                }
            }
            RValue::EnumTag(target) => {
                self.collect_strings_from_op(target);
            }
            RValue::EnumPayload { target, .. } => {
                self.collect_strings_from_op(target);
            }
            RValue::ArrayInit { elements, arena, .. } => {
                for e in elements {
                    self.collect_strings_from_op(e);
                }
                if let Some(a) = arena {
                    self.collect_strings_from_op(a);
                }
            }
            RValue::ArrayIndex { target, index, .. } => {
                self.collect_strings_from_op(target);
                self.collect_strings_from_op(index);
            }
        }
    }

    fn collect_strings_from_op(&mut self, op: &Operand) {
        if let Operand::Constant(TirConstant::Str(s)) = op {
            self.intern_string(s);
        }
    }

    fn collect_vars_from_inst(&self, inst: &Instruction, vars: &mut HashMap<Var, String>) {
        match inst {
            Instruction::Assign { dest, rvalue, ty, .. } => {
                vars.insert(dest.clone(), type_to_llvm(ty));
                self.collect_vars_from_rvalue(rvalue, vars);
            }
            Instruction::AssertRefinement { operand, .. } => {
                self.collect_vars_from_op(operand, vars);
            }
            Instruction::Call { dest, func, args, ty, .. } => {
                if let Some(d) = dest {
                    vars.insert(d.clone(), type_to_llvm(ty));
                }
                self.collect_vars_from_op(func, vars);
                for a in args {
                    self.collect_vars_from_op(a, vars);
                }
            }
            Instruction::PerformEffect { dest, args, ty, .. } => {
                if let Some(d) = dest {
                    vars.insert(d.clone(), type_to_llvm(ty));
                }
                for a in args {
                    self.collect_vars_from_op(a, vars);
                }
            }
            Instruction::SetField { base, val, .. } => {
                vars.entry(base.clone()).or_insert_with(|| "ptr".to_string());
                self.collect_vars_from_op(val, vars);
            }
            Instruction::RegionEnter { dest, .. } => {
                vars.insert(dest.clone(), "ptr".to_string());
            }
            Instruction::RegionExit { arena, .. } => {
                self.collect_vars_from_op(arena, vars);
            }
            Instruction::NurseryEnter { dest, .. } => {
                vars.insert(dest.clone(), "ptr".to_string());
            }
            Instruction::NurseryExit { nursery, .. } => {
                self.collect_vars_from_op(nursery, vars);
            }
            Instruction::ExternCall { dest, args, ty, .. } => {
                if let Some(d) = dest {
                    vars.insert(d.clone(), type_to_llvm(ty));
                }
                for a in args {
                    self.collect_vars_from_op(a, vars);
                }
            }
            Instruction::Store { ptr, value, .. } => {
                self.collect_vars_from_op(ptr, vars);
                self.collect_vars_from_op(value, vars);
            }
        }
    }

    fn collect_vars_from_rvalue(&self, rv: &RValue, vars: &mut HashMap<Var, String>) {
        match rv {
            RValue::Use(op) | RValue::Cast { operand: op, .. } | RValue::Ref { operand: op, .. } | RValue::Deref(op) | RValue::AddrOf(op) => {
                self.collect_vars_from_op(op, vars);
            }
            RValue::BinaryOp(_, l, r) => {
                self.collect_vars_from_op(l, vars);
                self.collect_vars_from_op(r, vars);
            }
            RValue::FieldAccess { target, .. } => {
                self.collect_vars_from_op(target, vars);
            }
            RValue::MethodCall { target, args, .. } => {
                self.collect_vars_from_op(target, vars);
                for a in args {
                    self.collect_vars_from_op(a, vars);
                }
            }
            RValue::StructInit { fields, arena, .. } => {
                for (_, f) in fields {
                    self.collect_vars_from_op(f, vars);
                }
                if let Some(a) = arena {
                    self.collect_vars_from_op(a, vars);
                }
            }
            RValue::EnumInit { payload, arena, .. } => {
                for p in payload {
                    self.collect_vars_from_op(p, vars);
                }
                if let Some(a) = arena {
                    self.collect_vars_from_op(a, vars);
                }
            }
            RValue::EnumTag(target) => {
                self.collect_vars_from_op(target, vars);
            }
            RValue::EnumPayload { target, .. } => {
                self.collect_vars_from_op(target, vars);
            }
            RValue::ArrayInit { elements, arena, .. } => {
                for e in elements {
                    self.collect_vars_from_op(e, vars);
                }
                if let Some(a) = arena {
                    self.collect_vars_from_op(a, vars);
                }
            }
            RValue::ArrayIndex { target, index, .. } => {
                self.collect_vars_from_op(target, vars);
                self.collect_vars_from_op(index, vars);
            }
        }
    }

    fn collect_vars_from_op(&self, op: &Operand, vars: &mut HashMap<Var, String>) {
        if let Operand::Var(v, ty) = op {
            if let Var::Named(name) = v {
                if name.starts_with('@') {
                    return;
                }
                let clean = name.trim_start_matches('_');
                if self.module.functions.iter().any(|f| f.name == clean) {
                    return;
                }
            }
            vars.entry(v.clone()).or_insert_with(|| type_to_llvm(ty));
        }
    }

    fn emit_function(&mut self, func: &TirFunction) {
        let is_main = func.name == "main";
        let ret_llvm_ty = if is_main {
            "i32".to_string()
        } else {
            type_to_llvm_ret(&func.return_type)
        };

        let mut params_sig = Vec::new();
        for p in &func.params {
            let ty_str = type_to_llvm(&p.ty);
            params_sig.push(format!("{} %arg_{}", ty_str, p.name));
        }

        self.out.push_str(&format!("define {} @{}({}) {{\n", ret_llvm_ty, func.name, params_sig.join(", ")));

        // Entry block: allocate stack slots for parameters and local variables
        self.out.push_str("entry:\n");

        // Collect all variables used in the function
        let mut vars = HashMap::new();
        for p in &func.params {
            vars.insert(Var::Named(p.name.clone()), type_to_llvm(&p.ty));
        }
        for block in &func.blocks {
            for inst in &block.instructions {
                self.collect_vars_from_inst(inst, &mut vars);
            }
            if let Some(ref term) = block.terminator {
                match term {
                    Terminator::Return(Some(op)) => self.collect_vars_from_op(op, &mut vars),
                    Terminator::BranchCond { cond, .. } => self.collect_vars_from_op(cond, &mut vars),
                    _ => {}
                }
            }
        }

        self.var_types = vars.clone();

        for (v, ty_str) in &vars {
            let v_name = var_to_slot_name(v);
            self.out.push_str(&format!("    {} = alloca {}\n", v_name, ty_str));
            if ty_str == "ptr" {
                self.out.push_str(&format!("    store ptr null, ptr {}\n", v_name));
            } else {
                self.out.push_str(&format!("    store {} 0, ptr {}\n", ty_str, v_name));
            }
        }

        // Store parameter values into allocas
        for p in &func.params {
            let slot = var_to_slot_name(&Var::Named(p.name.clone()));
            let ty_str = type_to_llvm(&p.ty);
            self.out.push_str(&format!("    store {} %arg_{}, ptr {}\n", ty_str, p.name, slot));
        }

        // Jump to first basic block
        self.out.push_str(&format!("    br label %bb{}\n\n", func.entry_block.0));

        // Emit basic blocks
        for block in &func.blocks {
            self.out.push_str(&format!("bb{}:\n", block.id.0));

            for inst in &block.instructions {
                self.emit_instruction(inst);
            }

            if let Some(ref term) = block.terminator {
                self.emit_terminator(term, is_main, &func.return_type);
            } else {
                if is_main {
                    self.out.push_str("    ret i32 0\n");
                } else if ret_llvm_ty == "void" {
                    self.out.push_str("    ret void\n");
                } else {
                    self.out.push_str(&format!("    ret {} 0\n", ret_llvm_ty));
                }
            }
            self.out.push('\n');
        }

        self.out.push_str("}\n\n");
    }

    fn emit_instruction(&mut self, inst: &Instruction) {
        match inst {
            Instruction::Assign { dest, rvalue, ty, .. } => {
                let (rval_res, rval_ty) = self.emit_rvalue(rvalue, ty);
                let target_ty = self.var_types.get(dest).cloned().unwrap_or_else(|| type_to_llvm(ty));
                let coerced = self.coerce_val(&rval_res, &rval_ty, &target_ty);
                let slot = var_to_slot_name(dest);
                self.out.push_str(&format!("    store {} {}, ptr {}\n", target_ty, coerced, slot));
            }
            Instruction::AssertRefinement { operand, interval, .. } => {
                let op_val = self.emit_operand(operand);
                let op_ty = self.get_operand_llvm_type(operand);
                let op_i64 = if op_ty == "i64" {
                    op_val
                } else {
                    let t_ext = self.next_temp();
                    self.out.push_str(&format!("    {} = zext {} {} to i64\n", t_ext, op_ty, op_val));
                    t_ext
                };
                let t_low = self.next_temp();
                let t_high = self.next_temp();
                let t_err = self.next_temp();
                let id = self.temp_counter;

                self.out.push_str(&format!("    {} = icmp slt i64 {}, {}\n", t_low, op_i64, interval.min));
                self.out.push_str(&format!("    {} = icmp sgt i64 {}, {}\n", t_high, op_i64, interval.max));
                self.out.push_str(&format!("    {} = or i1 {}, {}\n", t_err, t_low, t_high));
                self.out.push_str(&format!("    br i1 {}, label %panic_{}, label %cont_{}\n", t_err, id, id));

                self.out.push_str(&format!("panic_{}:\n", id));
                self.out.push_str(&format!("    call void @tungsten_refinement_panic(i64 {}, i64 {}, i64 {})\n", op_i64, interval.min, interval.max));
                self.out.push_str("    unreachable\n");

                self.out.push_str(&format!("cont_{}:\n", id));
            }
            Instruction::PerformEffect { effect, op, args, dest, ty, .. } => {
                if effect == "IO" && op == "print" {
                    if let Some(first_arg) = args.first() {
                        let arg_val = self.emit_operand(first_arg);
                        self.out.push_str(&format!("    call void @tungsten_io_print(ptr {}, i64 0)\n", arg_val));
                    }
                } else if (effect == "Db" && op == "query") || (effect == "PostgresPool" && op == "execute") {
                    let ptr_temp = self.next_temp();
                    self.out.push_str(&format!("    {} = call ptr @tungsten_alloc(i64 24, i64 8)\n", ptr_temp));
                    
                    let s_idx = self.intern_string("PlayerOne");
                    let name_gep = self.next_temp();
                    self.out.push_str(&format!("    {} = getelementptr inbounds i8, ptr {}, i64 0\n", name_gep, ptr_temp));
                    self.out.push_str(&format!("    store ptr @str_{}, ptr {}\n", s_idx, name_gep));

                    let hp_gep = self.next_temp();
                    self.out.push_str(&format!("    {} = getelementptr inbounds i8, ptr {}, i64 8\n", hp_gep, ptr_temp));
                    self.out.push_str(&format!("    store i64 80, ptr {}\n", hp_gep));

                    let id_val = if let Some(a) = args.get(1).or_else(|| args.first()) {
                        self.emit_operand(a)
                    } else {
                        "42".to_string()
                    };
                    let id_gep = self.next_temp();
                    self.out.push_str(&format!("    {} = getelementptr inbounds i8, ptr {}, i64 16\n", id_gep, ptr_temp));
                    self.out.push_str(&format!("    store i64 {}, ptr {}\n", id_val, id_gep));

                    if let Some(d) = dest {
                        let slot = var_to_slot_name(d);
                        self.out.push_str(&format!("    store ptr {}, ptr {}\n", ptr_temp, slot));
                    }
                } else if effect == "Async" {
                    if op == "spawn" {
                        let fn_arg = args.first().map(|a| self.emit_operand(a)).unwrap_or_else(|| "null".to_string());
                        let a1_val = args.get(1).map(|a| self.emit_operand(a)).unwrap_or_else(|| "0".to_string());
                        let a1_ty = args.get(1).map(|a| self.get_operand_llvm_type(a)).unwrap_or_else(|| "i64".to_string());
                        let a1 = self.coerce_val(&a1_val, &a1_ty, "i64");
                        let a2_val = args.get(2).map(|a| self.emit_operand(a)).unwrap_or_else(|| "0".to_string());
                        let a2_ty = args.get(2).map(|a| self.get_operand_llvm_type(a)).unwrap_or_else(|| "i64".to_string());
                        let a2 = self.coerce_val(&a2_val, &a2_ty, "i64");
                        let res_temp = self.next_temp();
                        self.out.push_str(&format!("    {} = call i64 @tungsten_fiber_spawn(ptr {}, i64 {}, i64 {})\n", res_temp, fn_arg, a1, a2));
                        if let Some(d) = dest {
                            let slot = var_to_slot_name(d);
                            self.out.push_str(&format!("    store i64 {}, ptr {}\n", res_temp, slot));
                        }
                    } else if op == "yield_now" {
                        self.out.push_str("    call void @tungsten_fiber_yield()\n");
                    } else if op == "sleep" {
                        let ms_val = args.first().map(|a| self.emit_operand(a)).unwrap_or_else(|| "0".to_string());
                        let ms_ty = args.first().map(|a| self.get_operand_llvm_type(a)).unwrap_or_else(|| "i64".to_string());
                        let ms = self.coerce_val(&ms_val, &ms_ty, "i64");
                        self.out.push_str(&format!("    call void @tungsten_fiber_sleep(i64 {})\n", ms));
                    }
                } else if effect == "Channel" {
                    if op == "new" {
                        let res_temp = self.next_temp();
                        self.out.push_str(&format!("    {} = call i64 @tungsten_channel_new()\n", res_temp));
                        if let Some(d) = dest {
                            let slot = var_to_slot_name(d);
                            self.out.push_str(&format!("    store i64 {}, ptr {}\n", res_temp, slot));
                        }
                    } else if op == "bounded" && !args.is_empty() {
                        let cap_val = self.emit_operand(&args[0]);
                        let cap_ty = self.get_operand_llvm_type(&args[0]);
                        let cap = self.coerce_val(&cap_val, &cap_ty, "i64");
                        let res_temp = self.next_temp();
                        self.out.push_str(&format!("    {} = call i64 @tungsten_channel_bounded(i64 {})\n", res_temp, cap));
                        if let Some(d) = dest {
                            let slot = var_to_slot_name(d);
                            self.out.push_str(&format!("    store i64 {}, ptr {}\n", res_temp, slot));
                        }
                    } else if op == "send" && args.len() >= 2 {
                        let cid_val = self.emit_operand(&args[0]);
                        let cid_ty = self.get_operand_llvm_type(&args[0]);
                        let cid = self.coerce_val(&cid_val, &cid_ty, "i64");
                        let val_op = self.emit_operand(&args[1]);
                        let val_ty = self.get_operand_llvm_type(&args[1]);
                        let val = self.coerce_val(&val_op, &val_ty, "i64");
                        self.out.push_str(&format!("    call void @tungsten_channel_send(i64 {}, i64 {})\n", cid, val));
                    } else if op == "recv" && !args.is_empty() {
                        let cid_val = self.emit_operand(&args[0]);
                        let cid_ty = self.get_operand_llvm_type(&args[0]);
                        let cid = self.coerce_val(&cid_val, &cid_ty, "i64");
                        let res_temp = self.next_temp();
                        self.out.push_str(&format!("    {} = call i64 @tungsten_channel_recv(i64 {})\n", res_temp, cid));
                        if let Some(d) = dest {
                            let slot = var_to_slot_name(d);
                            self.out.push_str(&format!("    store i64 {}, ptr {}\n", res_temp, slot));
                        }
                    }
                } else if effect == "Nursery" {
                    if op == "spawn" && !args.is_empty() {
                        let nur_v = self.emit_operand(&args[0]);
                        let nur_ty = self.get_operand_llvm_type(&args[0]);
                        let nur_ptr = if nur_ty == "ptr" { nur_v } else {
                            let t = self.next_temp();
                            self.out.push_str(&format!("    {} = inttoptr {} {} to ptr\n", t, nur_ty, nur_v));
                            t
                        };
                        let fn_arg = args.get(1).map(|a| self.emit_operand(a)).unwrap_or_else(|| "null".to_string());
                        let a1_val = args.get(2).map(|a| self.emit_operand(a)).unwrap_or_else(|| "0".to_string());
                        let a1_ty = args.get(2).map(|a| self.get_operand_llvm_type(a)).unwrap_or_else(|| "i64".to_string());
                        let a1 = self.coerce_val(&a1_val, &a1_ty, "i64");
                        let a2_val = args.get(3).map(|a| self.emit_operand(a)).unwrap_or_else(|| "0".to_string());
                        let a2_ty = args.get(3).map(|a| self.get_operand_llvm_type(a)).unwrap_or_else(|| "i64".to_string());
                        let a2 = self.coerce_val(&a2_val, &a2_ty, "i64");
                        let res_temp = self.next_temp();
                        self.out.push_str(&format!("    {} = call i64 @tungsten_nursery_spawn(ptr {}, ptr {}, i64 {}, i64 {})\n", res_temp, nur_ptr, fn_arg, a1, a2));
                        if let Some(d) = dest {
                            let slot = var_to_slot_name(d);
                            self.out.push_str(&format!("    store i64 {}, ptr {}\n", res_temp, slot));
                        }
                    }
                } else if effect == "Net" {
                    if op == "listen" {
                        let port_val = args.first().map(|a| self.emit_operand(a)).unwrap_or_else(|| "8080".to_string());
                        let port_ty = args.first().map(|a| self.get_operand_llvm_type(a)).unwrap_or_else(|| "i64".to_string());
                        let port_i64 = self.coerce_val(&port_val, &port_ty, "i64");
                        let res_temp = self.next_temp();
                        self.out.push_str(&format!("    {} = call i64 @tungsten_net_listen(i64 {})\n", res_temp, port_i64));
                        if let Some(d) = dest {
                            let slot = var_to_slot_name(d);
                            self.out.push_str(&format!("    store i64 {}, ptr {}\n", res_temp, slot));
                        }
                    } else if op == "accept" {
                        let sock_val = args.first().map(|a| self.emit_operand(a)).unwrap_or_else(|| "0".to_string());
                        let sock_ty = args.first().map(|a| self.get_operand_llvm_type(a)).unwrap_or_else(|| "i64".to_string());
                        let sock_i64 = self.coerce_val(&sock_val, &sock_ty, "i64");
                        let res_temp = self.next_temp();
                        self.out.push_str(&format!("    {} = call i64 @tungsten_net_accept(i64 {})\n", res_temp, sock_i64));
                        if let Some(d) = dest {
                            let slot = var_to_slot_name(d);
                            self.out.push_str(&format!("    store i64 {}, ptr {}\n", res_temp, slot));
                        }
                    } else if op == "connect" {
                        let host_val = args.first().map(|a| self.emit_operand(a)).unwrap_or_else(|| "null".to_string());
                        let host_ty = args.first().map(|a| self.get_operand_llvm_type(a)).unwrap_or_else(|| "ptr".to_string());
                        let host_ptr = self.coerce_val(&host_val, &host_ty, "ptr");
                        let port_val = args.get(1).map(|a| self.emit_operand(a)).unwrap_or_else(|| "8080".to_string());
                        let port_ty = args.get(1).map(|a| self.get_operand_llvm_type(a)).unwrap_or_else(|| "i64".to_string());
                        let port_i64 = self.coerce_val(&port_val, &port_ty, "i64");
                        let res_temp = self.next_temp();
                        self.out.push_str(&format!("    {} = call i64 @tungsten_net_connect(ptr {}, i64 {})\n", res_temp, host_ptr, port_i64));
                        if let Some(d) = dest {
                            let slot = var_to_slot_name(d);
                            self.out.push_str(&format!("    store i64 {}, ptr {}\n", res_temp, slot));
                        }
                    } else if op == "read" {
                        let conn_val = args.first().map(|a| self.emit_operand(a)).unwrap_or_else(|| "0".to_string());
                        let conn_ty = args.first().map(|a| self.get_operand_llvm_type(a)).unwrap_or_else(|| "i64".to_string());
                        let conn_i64 = self.coerce_val(&conn_val, &conn_ty, "i64");
                        let max_len = args.get(1).map(|a| self.emit_operand(a)).unwrap_or_else(|| "4096".to_string());
                        let len_ty = args.get(1).map(|a| self.get_operand_llvm_type(a)).unwrap_or_else(|| "i64".to_string());
                        let len_i64 = self.coerce_val(&max_len, &len_ty, "i64");
                        let res_temp = self.next_temp();
                        self.out.push_str(&format!("    {} = call ptr @tungsten_net_read(i64 {}, i64 {})\n", res_temp, conn_i64, len_i64));
                        if let Some(d) = dest {
                            let slot = var_to_slot_name(d);
                            self.out.push_str(&format!("    store ptr {}, ptr {}\n", res_temp, slot));
                        }
                    } else if op == "write" {
                        let conn_val = args.first().map(|a| self.emit_operand(a)).unwrap_or_else(|| "0".to_string());
                        let conn_ty = args.first().map(|a| self.get_operand_llvm_type(a)).unwrap_or_else(|| "i64".to_string());
                        let conn_i64 = self.coerce_val(&conn_val, &conn_ty, "i64");
                        let data_val = args.get(1).map(|a| self.emit_operand(a)).unwrap_or_else(|| "null".to_string());
                        let data_ty = args.get(1).map(|a| self.get_operand_llvm_type(a)).unwrap_or_else(|| "ptr".to_string());
                        let data_ptr = self.coerce_val(&data_val, &data_ty, "ptr");
                        let res_temp = self.next_temp();
                        self.out.push_str(&format!("    {} = call i64 @tungsten_net_write(i64 {}, ptr {}, i64 4096)\n", res_temp, conn_i64, data_ptr));
                        if let Some(d) = dest {
                            let slot = var_to_slot_name(d);
                            self.out.push_str(&format!("    store i64 {}, ptr {}\n", res_temp, slot));
                        }
                    } else if op == "close" {
                        let conn_val = args.first().map(|a| self.emit_operand(a)).unwrap_or_else(|| "0".to_string());
                        let conn_ty = args.first().map(|a| self.get_operand_llvm_type(a)).unwrap_or_else(|| "i64".to_string());
                        let conn_i64 = self.coerce_val(&conn_val, &conn_ty, "i64");
                        self.out.push_str(&format!("    call void @tungsten_net_close(i64 {})\n", conn_i64));
                    }
                } else if (effect == "Foreign" || effect == "ForeignCall") && (op == "call" || op == "blocking") {
                    let fn_arg = args.first().map(|a| self.emit_operand(a)).unwrap_or_else(|| "null".to_string());
                    let a1_val = args.get(1).map(|a| self.emit_operand(a)).unwrap_or_else(|| "0".to_string());
                    let a1_ty = args.get(1).map(|a| self.get_operand_llvm_type(a)).unwrap_or_else(|| "i64".to_string());
                    let a1 = self.coerce_val(&a1_val, &a1_ty, "i64");
                    let res_temp = self.next_temp();
                    self.out.push_str(&format!("    {} = call i64 @tungsten_foreign_call_offload(ptr {}, i64 {})\n", res_temp, fn_arg, a1));
                    if let Some(d) = dest {
                        let slot = var_to_slot_name(d);
                        let target_ty = self.var_types.get(d).cloned().unwrap_or_else(|| type_to_llvm(ty));
                        let coerced = self.coerce_val(&res_temp, "i64", &target_ty);
                        self.out.push_str(&format!("    store {} {}, ptr {}\n", target_ty, coerced, slot));
                    }
                }
            }
            Instruction::Call { dest, func, args, ty, .. } => {
                let func_name = match func {
                    Operand::Var(Var::Named(name), _) => name.clone(),
                    Operand::Constant(TirConstant::Str(name)) => name.clone(),
                    _ => "".to_string(),
                };

                if func_name == "@println" {
                    self.emit_macro_println(args);
                    return;
                }

                let clean = func_name.trim_start_matches('_');
                let target_fn = self.module.functions.iter().find(|f| f.name == clean);
                let mut arg_strs = Vec::new();
                for (idx, a) in args.iter().enumerate() {
                    let op_str = self.emit_operand(a);
                    let from_ty = self.get_operand_llvm_type(a);
                    let expected_ty = if let Some(tf) = target_fn {
                        tf.params.get(idx).map(|p| type_to_llvm(&p.ty)).unwrap_or_else(|| from_ty.clone())
                    } else {
                        from_ty.clone()
                    };
                    let coerced = self.coerce_val(&op_str, &from_ty, &expected_ty);
                    arg_strs.push(format!("{} {}", expected_ty, coerced));
                }

                let ret_ty = type_to_llvm_ret(ty);
                if ret_ty == "void" {
                    self.out.push_str(&format!("    call void @{}({})\n", clean, arg_strs.join(", ")));
                } else {
                    let call_temp = self.next_temp();
                    self.out.push_str(&format!("    {} = call {} @{}({})\n", call_temp, ret_ty, clean, arg_strs.join(", ")));
                    if let Some(d) = dest {
                        let dest_ty = self.var_types.get(d).cloned().unwrap_or_else(|| type_to_llvm(ty));
                        let coerced = self.coerce_val(&call_temp, &ret_ty, &dest_ty);
                        let slot = var_to_slot_name(d);
                        self.out.push_str(&format!("    store {} {}, ptr {}\n", dest_ty, coerced, slot));
                    }
                }
            }
            Instruction::SetField { base, field, val, .. } => {
                let base_slot = var_to_slot_name(base);
                let base_ty = self.var_types.get(base).cloned().unwrap_or_else(|| "ptr".to_string());
                let base_val = self.next_temp();
                self.out.push_str(&format!("    {} = load {}, ptr {}\n", base_val, base_ty, base_slot));
                let base_ptr = self.coerce_val(&base_val, &base_ty, "ptr");
                let offset = self.calculate_field_offset(field);
                let gep = self.next_temp();
                self.out.push_str(&format!("    {} = getelementptr inbounds i8, ptr {}, i64 {}\n", gep, base_ptr, offset));
                let val_str = self.emit_operand(val);
                let val_ty = self.get_operand_llvm_type(val);
                let val_i64 = self.coerce_val(&val_str, &val_ty, "i64");
                self.out.push_str(&format!("    store i64 {}, ptr {}\n", val_i64, gep));
            }
            Instruction::RegionEnter { dest, .. } => {
                let arena_temp = self.next_temp();
                self.out.push_str(&format!("    {} = call ptr @tungsten_region_enter()\n", arena_temp));
                let slot = var_to_slot_name(dest);
                self.out.push_str(&format!("    store ptr {}, ptr {}\n", arena_temp, slot));
            }
            Instruction::RegionExit { arena, .. } => {
                let arena_val = self.emit_operand(arena);
                let arena_ty = self.get_operand_llvm_type(arena);
                let arena_ptr = if arena_ty == "ptr" {
                    arena_val
                } else {
                    let t = self.next_temp();
                    self.out.push_str(&format!("    {} = inttoptr {} {} to ptr\n", t, arena_ty, arena_val));
                    t
                };
                self.out.push_str(&format!("    call void @tungsten_region_exit(ptr {})\n", arena_ptr));
            }
            Instruction::NurseryEnter { dest, .. } => {
                let nur_temp = self.next_temp();
                self.out.push_str(&format!("    {} = call ptr @tungsten_nursery_enter()\n", nur_temp));
                let slot = var_to_slot_name(dest);
                self.out.push_str(&format!("    store ptr {}, ptr {}\n", nur_temp, slot));
            }
            Instruction::NurseryExit { nursery, .. } => {
                let nur_val = self.emit_operand(nursery);
                let nur_ty = self.get_operand_llvm_type(nursery);
                let nur_ptr = if nur_ty == "ptr" {
                    nur_val
                } else {
                    let t = self.next_temp();
                    self.out.push_str(&format!("    {} = inttoptr {} {} to ptr\n", t, nur_ty, nur_val));
                    t
                };
                self.out.push_str(&format!("    call void @tungsten_nursery_wait_all(ptr {})\n", nur_ptr));
            }
            Instruction::ExternCall { dest, func, args, ty, .. } => {
                let mut arg_strs = Vec::new();
                for a in args {
                    let val = self.emit_operand(a);
                    let arg_ty = self.get_operand_llvm_type(a);
                    arg_strs.push(format!("{} {}", arg_ty, val));
                }
                let ret_llvm_ty = type_to_llvm_ret(ty);
                if ret_llvm_ty == "void" {
                    self.out.push_str(&format!("    call void @{}({})\n", func, arg_strs.join(", ")));
                } else {
                    let res_temp = self.next_temp();
                    self.out.push_str(&format!("    {} = call {} @{}({})\n", res_temp, ret_llvm_ty, func, arg_strs.join(", ")));
                    if let Some(d) = dest {
                        let slot = var_to_slot_name(d);
                        let target_ty = self.var_types.get(d).cloned().unwrap_or_else(|| type_to_llvm(ty));
                        let coerced = self.coerce_val(&res_temp, &ret_llvm_ty, &target_ty);
                        self.out.push_str(&format!("    store {} {}, ptr {}\n", target_ty, coerced, slot));
                    }
                }
            }
            Instruction::Store { ptr, value, .. } => {
                let ptr_val = self.emit_operand(ptr);
                let ptr_ty = self.get_operand_llvm_type(ptr);
                let ptr_coerced = self.coerce_val(&ptr_val, &ptr_ty, "ptr");
                let val = self.emit_operand(value);
                let val_ty = self.get_operand_llvm_type(value);
                self.out.push_str(&format!("    store {} {}, ptr {}\n", val_ty, val, ptr_coerced));
            }
        }
    }

    fn emit_macro_println(&mut self, args: &[Operand]) {
        if args.is_empty() {
            self.out.push_str("    call i32 @putchar(i32 10)\n");
            return;
        }

        if let Operand::Constant(TirConstant::Str(ref fmt_str)) = args[0] {
            if args.len() == 1 {
                let idx = self.intern_string(fmt_str);
                self.out.push_str(&format!("    call void @tungsten_println_str(ptr @str_{}, i64 {})\n", idx, fmt_str.len()));
                return;
            }

            let parts: Vec<&str> = fmt_str.split("{}").collect();
            let mut arg_idx = 1;
            for (i, part) in parts.iter().enumerate() {
                if !part.is_empty() {
                    let idx = self.intern_string(part);
                    self.out.push_str(&format!("    call void @tungsten_print_str(ptr @str_{}, i64 {})\n", idx, part.len()));
                }

                if i < parts.len() - 1 && arg_idx < args.len() {
                    let arg_op = &args[arg_idx];
                    arg_idx += 1;
                    let val_str = self.emit_operand(arg_op);
                    let val_ty = self.get_operand_llvm_type(arg_op);
                    match arg_op.get_type() {
                        Type::String => {
                            let s_ptr = self.coerce_val(&val_str, &val_ty, "ptr");
                            self.out.push_str(&format!("    call void @tungsten_print_str(ptr {}, i64 0)\n", s_ptr));
                        }
                        _ => {
                            let val_i64 = if val_ty == "i64" {
                                val_str
                            } else {
                                let t_ext = self.next_temp();
                                self.out.push_str(&format!("    {} = zext {} {} to i64\n", t_ext, val_ty, val_str));
                                t_ext
                            };
                            self.out.push_str(&format!("    call void @tungsten_print_i64(i64 {})\n", val_i64));
                        }
                    }
                }
            }

            self.out.push_str("    call i32 @putchar(i32 10)\n");
        } else {
            for a in args {
                let val_str = self.emit_operand(a);
                let val_ty = self.get_operand_llvm_type(a);
                match a.get_type() {
                    Type::String => {
                        let s_ptr = self.coerce_val(&val_str, &val_ty, "ptr");
                        self.out.push_str(&format!("    call void @tungsten_println_str(ptr {}, i64 0)\n", s_ptr));
                    }
                    _ => {
                        let val_i64 = if val_ty == "i64" {
                            val_str
                        } else {
                            let t_ext = self.next_temp();
                            self.out.push_str(&format!("    {} = zext {} {} to i64\n", t_ext, val_ty, val_str));
                            t_ext
                        };
                        self.out.push_str(&format!("    call void @tungsten_println_i64(i64 {})\n", val_i64));
                    }
                }
            }
        }
    }

    fn coerce_val(&mut self, val: &str, from_ty: &str, to_ty: &str) -> String {
        if from_ty == to_ty {
            val.to_string()
        } else if from_ty == "i64" && (to_ty == "i8" || to_ty == "i16" || to_ty == "i32") {
            let t = self.next_temp();
            self.out.push_str(&format!("    {} = trunc i64 {} to {}\n", t, val, to_ty));
            t
        } else if (from_ty == "i8" || from_ty == "i16" || from_ty == "i32") && to_ty == "i64" {
            let t = self.next_temp();
            self.out.push_str(&format!("    {} = zext {} {} to i64\n", t, from_ty, val));
            t
        } else if from_ty == "ptr" && to_ty == "i64" {
            let t = self.next_temp();
            self.out.push_str(&format!("    {} = ptrtoint ptr {} to i64\n", t, val));
            t
        } else if from_ty == "i64" && to_ty == "ptr" {
            let t = self.next_temp();
            self.out.push_str(&format!("    {} = inttoptr i64 {} to ptr\n", t, val));
            t
        } else {
            val.to_string()
        }
    }

    fn emit_rvalue(&mut self, rv: &RValue, _ty: &Type) -> (String, String) {
        match rv {
            RValue::Use(op) => (self.emit_operand(op), self.get_operand_llvm_type(op)),
            RValue::BinaryOp(op, l, r) => {
                let lv = self.emit_operand(l);
                let l_ty = self.get_operand_llvm_type(l);
                let lv_i64 = self.coerce_val(&lv, &l_ty, "i64");

                let rv = self.emit_operand(r);
                let r_ty = self.get_operand_llvm_type(r);
                let rv_i64 = self.coerce_val(&rv, &r_ty, "i64");

                let t = self.next_temp();
                let op_str = match op {
                    BinOp::Add => "add i64",
                    BinOp::Sub => "sub i64",
                    BinOp::Mul => "mul i64",
                    BinOp::Div => "sdiv i64",
                    BinOp::Eq => "icmp eq i64",
                    BinOp::NotEq => "icmp ne i64",
                    BinOp::Lt => "icmp slt i64",
                    BinOp::LtEq => "icmp sle i64",
                    BinOp::Gt => "icmp sgt i64",
                    BinOp::GtEq => "icmp sge i64",
                    BinOp::And => "and i64",
                    BinOp::Or => "or i64",
                };
                if matches!(op, BinOp::Eq | BinOp::NotEq | BinOp::Lt | BinOp::LtEq | BinOp::Gt | BinOp::GtEq) {
                    let cmp_t = self.next_temp();
                    self.out.push_str(&format!("    {} = {} {}, {}\n", cmp_t, op_str, lv_i64, rv_i64));
                    self.out.push_str(&format!("    {} = zext i1 {} to i64\n", t, cmp_t));
                } else {
                    self.out.push_str(&format!("    {} = {} {}, {}\n", t, op_str, lv_i64, rv_i64));
                }
                (t, "i64".to_string())
            }
            RValue::MethodCall { target, method, args } => {
                let target_v = self.emit_operand(target);
                let target_ty = self.get_operand_llvm_type(target);
                let target_i64 = self.coerce_val(&target_v, &target_ty, "i64");
                if method == "spawn" {
                    let nur_ptr = if target_ty == "ptr" {
                        target_v
                    } else {
                        let t = self.next_temp();
                        self.out.push_str(&format!("    {} = inttoptr {} {} to ptr\n", t, target_ty, target_v));
                        t
                    };
                    let fn_arg = args.first().map(|a| self.emit_operand(a)).unwrap_or_else(|| "null".to_string());
                    let a1_val = args.get(1).map(|a| self.emit_operand(a)).unwrap_or_else(|| "0".to_string());
                    let a1_ty = args.get(1).map(|a| self.get_operand_llvm_type(a)).unwrap_or_else(|| "i64".to_string());
                    let a1 = self.coerce_val(&a1_val, &a1_ty, "i64");
                    let a2_val = args.get(2).map(|a| self.emit_operand(a)).unwrap_or_else(|| "0".to_string());
                    let a2_ty = args.get(2).map(|a| self.get_operand_llvm_type(a)).unwrap_or_else(|| "i64".to_string());
                    let a2 = self.coerce_val(&a2_val, &a2_ty, "i64");
                    let res_temp = self.next_temp();
                    self.out.push_str(&format!("    {} = call i64 @tungsten_nursery_spawn(ptr {}, ptr {}, i64 {}, i64 {})\n", res_temp, nur_ptr, fn_arg, a1, a2));
                    (res_temp, "i64".to_string())
                } else if method == "saturating_add" && args.len() == 1 {
                    let delta_v = self.emit_operand(&args[0]);
                    let delta_ty = self.get_operand_llvm_type(&args[0]);
                    let delta_i64 = self.coerce_val(&delta_v, &delta_ty, "i64");
                    let sum_t = self.next_temp();
                    let cmp_t = self.next_temp();
                    let sel_t = self.next_temp();
                    self.out.push_str(&format!("    {} = add i64 {}, {}\n", sum_t, target_i64, delta_i64));
                    self.out.push_str(&format!("    {} = icmp sgt i64 {}, 100\n", cmp_t, sum_t));
                    self.out.push_str(&format!("    {} = select i1 {}, i64 100, i64 {}\n", sel_t, cmp_t, sum_t));
                    (sel_t, "i64".to_string())
                } else {
                    (target_i64, "i64".to_string())
                }
            }
            RValue::StructInit { fields, arena, .. } => {
                let size = (fields.len() * 8).max(8);
                let ptr_temp = self.next_temp();
                if let Some(a_op) = arena {
                    let a_val = self.emit_operand(a_op);
                    let a_ty = self.get_operand_llvm_type(a_op);
                    let a_ptr = if a_ty == "ptr" {
                        a_val
                    } else {
                        let t = self.next_temp();
                        self.out.push_str(&format!("    {} = inttoptr {} {} to ptr\n", t, a_ty, a_val));
                        t
                    };
                    self.out.push_str(&format!("    {} = call ptr @tungsten_region_alloc(ptr {}, i64 {}, i64 8)\n", ptr_temp, a_ptr, size));
                } else {
                    self.out.push_str(&format!("    {} = call ptr @tungsten_alloc(i64 {}, i64 8)\n", ptr_temp, size));
                }

                for (idx, (_, f_op)) in fields.iter().enumerate() {
                    let f_val = self.emit_operand(f_op);
                    let f_ty = self.get_operand_llvm_type(f_op);
                    let f_i64 = self.coerce_val(&f_val, &f_ty, "i64");
                    let gep = self.next_temp();
                    self.out.push_str(&format!("    {} = getelementptr inbounds i8, ptr {}, i64 {}\n", gep, ptr_temp, idx * 8));
                    self.out.push_str(&format!("    store i64 {}, ptr {}\n", f_i64, gep));
                }

                (ptr_temp, "ptr".to_string())
            }
            RValue::FieldAccess { target, field } => {
                let target_v = self.emit_operand(target);
                let target_ty = self.get_operand_llvm_type(target);
                let target_ptr = self.coerce_val(&target_v, &target_ty, "ptr");
                let offset = self.calculate_field_offset(field);
                let gep = self.next_temp();
                let loaded = self.next_temp();
                self.out.push_str(&format!("    {} = getelementptr inbounds i8, ptr {}, i64 {}\n", gep, target_ptr, offset));
                self.out.push_str(&format!("    {} = load i64, ptr {}\n", loaded, gep));
                (loaded, "i64".to_string())
            }
            RValue::Cast { operand, target_ty } => {
                let val = self.emit_operand(operand);
                let from_ty = self.get_operand_llvm_type(operand);
                let to_ty = type_to_llvm(target_ty);
                let coerced = self.coerce_val(&val, &from_ty, &to_ty);
                (coerced, to_ty)
            }
            RValue::Ref { operand, .. } => {
                match operand {
                    Operand::Var(v, _) => {
                        let slot = var_to_slot_name(v);
                        (slot, "ptr".to_string())
                    }
                    Operand::Constant(TirConstant::Str(s)) => {
                        let idx = self.intern_string(s);
                        (format!("@str_{}", idx), "ptr".to_string())
                    }
                    _ => {
                        let val = self.emit_operand(operand);
                        let val_ty = self.get_operand_llvm_type(operand);
                        let t_slot = self.next_temp();
                        self.out.push_str(&format!("    {} = alloca {}\n", t_slot, val_ty));
                        self.out.push_str(&format!("    store {} {}, ptr {}\n", val_ty, val, t_slot));
                        (t_slot, "ptr".to_string())
                    }
                }
            }
            RValue::EnumInit { tag, payload, arena, .. } => {
                let size = 8 + payload.len() * 8;
                let ptr_temp = self.next_temp();
                if let Some(a_op) = arena {
                    let a_val = self.emit_operand(a_op);
                    let a_ty = self.get_operand_llvm_type(a_op);
                    let a_ptr = if a_ty == "ptr" {
                        a_val
                    } else {
                        let t = self.next_temp();
                        self.out.push_str(&format!("    {} = inttoptr {} {} to ptr\n", t, a_ty, a_val));
                        t
                    };
                    self.out.push_str(&format!("    {} = call ptr @tungsten_region_alloc(ptr {}, i64 {}, i64 8)\n", ptr_temp, a_ptr, size));
                } else {
                    self.out.push_str(&format!("    {} = call ptr @tungsten_alloc(i64 {}, i64 8)\n", ptr_temp, size));
                }

                // Store tag at offset 0
                let tag_gep = self.next_temp();
                self.out.push_str(&format!("    {} = getelementptr inbounds i8, ptr {}, i64 0\n", tag_gep, ptr_temp));
                self.out.push_str(&format!("    store i64 {}, ptr {}\n", tag, tag_gep));

                // Store payload elements at offset 8, 16, ...
                for (idx, p_op) in payload.iter().enumerate() {
                    let p_val = self.emit_operand(p_op);
                    let p_ty = self.get_operand_llvm_type(p_op);
                    let p_i64 = self.coerce_val(&p_val, &p_ty, "i64");
                    let p_gep = self.next_temp();
                    self.out.push_str(&format!("    {} = getelementptr inbounds i8, ptr {}, i64 {}\n", p_gep, ptr_temp, 8 + idx * 8));
                    self.out.push_str(&format!("    store i64 {}, ptr {}\n", p_i64, p_gep));
                }

                (ptr_temp, "ptr".to_string())
            }
            RValue::EnumTag(target) => {
                let target_v = self.emit_operand(target);
                let target_ty = self.get_operand_llvm_type(target);
                let target_ptr = self.coerce_val(&target_v, &target_ty, "ptr");
                let tag_val = self.next_temp();
                self.out.push_str(&format!("    {} = load i64, ptr {}\n", tag_val, target_ptr));
                (tag_val, "i64".to_string())
            }
            RValue::EnumPayload { target, index } => {
                let target_v = self.emit_operand(target);
                let target_ty = self.get_operand_llvm_type(target);
                let target_ptr = self.coerce_val(&target_v, &target_ty, "ptr");
                let gep = self.next_temp();
                let loaded = self.next_temp();
                self.out.push_str(&format!("    {} = getelementptr inbounds i8, ptr {}, i64 {}\n", gep, target_ptr, 8 + index * 8));
                self.out.push_str(&format!("    {} = load i64, ptr {}\n", loaded, gep));
                (loaded, "i64".to_string())
            }
            RValue::ArrayInit { elements, elem_stride, arena } => {
                let stride = (*elem_stride).max(1);
                let size = (elements.len() * stride).max(8);
                let ptr_temp = self.next_temp();
                if let Some(a_op) = arena {
                    let a_val = self.emit_operand(a_op);
                    let a_ty = self.get_operand_llvm_type(a_op);
                    let a_ptr = if a_ty == "ptr" {
                        a_val
                    } else {
                        let t = self.next_temp();
                        self.out.push_str(&format!("    {} = inttoptr {} {} to ptr\n", t, a_ty, a_val));
                        t
                    };
                    self.out.push_str(&format!("    {} = call ptr @tungsten_region_alloc(ptr {}, i64 {}, i64 8)\n", ptr_temp, a_ptr, size));
                } else {
                    self.out.push_str(&format!("    {} = call ptr @tungsten_alloc(i64 {}, i64 8)\n", ptr_temp, size));
                }

                let elem_llvm_ty = match stride {
                    1 => "i8",
                    2 => "i16",
                    4 => "i32",
                    _ => "i64",
                };

                for (idx, e_op) in elements.iter().enumerate() {
                    let e_val = self.emit_operand(e_op);
                    let e_ty = self.get_operand_llvm_type(e_op);
                    let coerced = self.coerce_val(&e_val, &e_ty, elem_llvm_ty);
                    let gep = self.next_temp();
                    self.out.push_str(&format!("    {} = getelementptr inbounds i8, ptr {}, i64 {}\n", gep, ptr_temp, idx * stride));
                    self.out.push_str(&format!("    store {} {}, ptr {}\n", elem_llvm_ty, coerced, gep));
                }

                (ptr_temp, "ptr".to_string())
            }
            RValue::ArrayIndex { target, index, stride } => {
                let target_v = self.emit_operand(target);
                let target_ty = self.get_operand_llvm_type(target);
                let target_ptr = self.coerce_val(&target_v, &target_ty, "ptr");

                let idx_v = self.emit_operand(index);
                let idx_ty = self.get_operand_llvm_type(index);
                let idx_i64 = self.coerce_val(&idx_v, &idx_ty, "i64");

                let s = (*stride).max(1);
                let offset_val = if s == 1 {
                    idx_i64
                } else {
                    let mul_t = self.next_temp();
                    self.out.push_str(&format!("    {} = mul i64 {}, {}\n", mul_t, idx_i64, s));
                    mul_t
                };

                let gep = self.next_temp();
                self.out.push_str(&format!("    {} = getelementptr inbounds i8, ptr {}, i64 {}\n", gep, target_ptr, offset_val));

                let elem_llvm_ty = match s {
                    1 => "i8",
                    2 => "i16",
                    4 => "i32",
                    _ => "i64",
                };

                let loaded = self.next_temp();
                self.out.push_str(&format!("    {} = load {}, ptr {}\n", loaded, elem_llvm_ty, gep));

                if s < 8 {
                    let ext = self.next_temp();
                    self.out.push_str(&format!("    {} = zext {} {} to i64\n", ext, elem_llvm_ty, loaded));
                    (ext, "i64".to_string())
                } else {
                    (loaded, "i64".to_string())
                }
            }
            RValue::Deref(op) => {
                let ptr_val = self.emit_operand(op);
                let ptr_ty = self.get_operand_llvm_type(op);
                let ptr_coerced = self.coerce_val(&ptr_val, &ptr_ty, "ptr");
                let target_llvm_ty = type_to_llvm(_ty);
                let loaded = self.next_temp();
                self.out.push_str(&format!("    {} = load {}, ptr {}\n", loaded, target_llvm_ty, ptr_coerced));
                (loaded, target_llvm_ty)
            }
            RValue::AddrOf(op) => {
                match op {
                    Operand::Var(v, _) => {
                        let slot = var_to_slot_name(v);
                        (slot, "ptr".to_string())
                    }
                    Operand::Constant(TirConstant::Str(s)) => {
                        let idx = self.intern_string(s);
                        (format!("@str_{}", idx), "ptr".to_string())
                    }
                    _ => {
                        let val = self.emit_operand(op);
                        let val_ty = self.get_operand_llvm_type(op);
                        let t_slot = self.next_temp();
                        self.out.push_str(&format!("    {} = alloca {}\n", t_slot, val_ty));
                        self.out.push_str(&format!("    store {} {}, ptr {}\n", val_ty, val, t_slot));
                        (t_slot, "ptr".to_string())
                    }
                }
            }
        }
    }

    fn emit_operand(&mut self, op: &Operand) -> String {
        match op {
            Operand::Constant(TirConstant::Int(n)) => n.to_string(),
            Operand::Constant(TirConstant::Bool(b)) => if *b { "1".to_string() } else { "0".to_string() },
            Operand::Constant(TirConstant::Unit) => "0".to_string(),
            Operand::Constant(TirConstant::Str(s)) => {
                let idx = self.intern_string(s);
                format!("@str_{}", idx)
            }
            Operand::Var(v, ty) => {
                if let Var::Named(name) = v {
                    let clean = name.trim_start_matches('_');
                    if self.module.functions.iter().any(|f| f.name == clean) {
                        return format!("@{}", clean);
                    }
                    if self.module.extern_blocks.iter().any(|b| b.fns.iter().any(|f| f.name == clean)) {
                        return format!("@{}", clean);
                    }
                }
                let slot = var_to_slot_name(v);
                let load_temp = self.next_temp();
                let ty_str = self.var_types.get(v).cloned().unwrap_or_else(|| type_to_llvm(ty));
                self.out.push_str(&format!("    {} = load {}, ptr {}\n", load_temp, ty_str, slot));
                load_temp
            }
        }
    }

    fn get_operand_llvm_type(&self, op: &Operand) -> String {
        match op {
            Operand::Var(v, ty) => {
                if let Var::Named(name) = v {
                    let clean = name.trim_start_matches('_');
                    if self.module.functions.iter().any(|f| f.name == clean) {
                        return "ptr".to_string();
                    }
                    if self.module.extern_blocks.iter().any(|b| b.fns.iter().any(|f| f.name == clean)) {
                        return "ptr".to_string();
                    }
                }
                self.var_types.get(v).cloned().unwrap_or_else(|| type_to_llvm(ty))
            }
            Operand::Constant(TirConstant::Str(_)) => "ptr".to_string(),
            _ => "i64".to_string(),
        }
    }

    fn emit_terminator(&mut self, term: &Terminator, is_main: bool, ret_ty: &Type) {
        match term {
            Terminator::Return(Some(op)) => {
                let val = self.emit_operand(op);
                let from_ty = self.get_operand_llvm_type(op);
                if is_main {
                    let coerced = self.coerce_val(&val, &from_ty, "i32");
                    self.out.push_str(&format!("    ret i32 {}\n", coerced));
                } else {
                    let ty_str = type_to_llvm_ret(ret_ty);
                    if ty_str == "void" {
                        self.out.push_str("    ret void\n");
                    } else {
                        let coerced = self.coerce_val(&val, &from_ty, &ty_str);
                        self.out.push_str(&format!("    ret {} {}\n", ty_str, coerced));
                    }
                }
            }
            Terminator::Return(None) => {
                if is_main {
                    self.out.push_str("    ret i32 0\n");
                } else {
                    self.out.push_str("    ret void\n");
                }
            }
            Terminator::Branch(target) => {
                self.out.push_str(&format!("    br label %bb{}\n", target.0));
            }
            Terminator::BranchCond { cond, then_block, else_block } => {
                let cond_v = self.emit_operand(cond);
                let cond_ty = self.get_operand_llvm_type(cond);
                let cond_i1 = if cond_ty == "i1" {
                    cond_v
                } else {
                    let cmp_temp = self.next_temp();
                    self.out.push_str(&format!("    {} = icmp ne {} {}, 0\n", cmp_temp, cond_ty, cond_v));
                    cmp_temp
                };
                self.out.push_str(&format!("    br i1 {}, label %bb{}, label %bb{}\n", cond_i1, then_block.0, else_block.0));
            }
            Terminator::HandleEffect { body_entry, .. } => {
                self.out.push_str(&format!("    br label %bb{}\n", body_entry.0));
            }
            Terminator::Resume { continuation_block, .. } => {
                self.out.push_str(&format!("    br label %bb{}\n", continuation_block.0));
            }
            Terminator::Unreachable => {
                self.out.push_str("    unreachable\n");
            }
        }
    }

    fn calculate_field_offset(&self, field_name: &str) -> usize {
        for s in &self.module.structs {
            for (idx, f) in s.fields.iter().enumerate() {
                if f.name == field_name {
                    return idx * 8;
                }
            }
        }
        if field_name == "value" || field_name == "name" {
            0
        } else {
            8
        }
    }
}

fn var_to_slot_name(v: &Var) -> String {
    match v {
        Var::Named(name) => {
            let clean = name.trim_start_matches('@').trim_start_matches('_');
            format!("%slot_{}", clean)
        }
        Var::Temp(id) => format!("%slot_tmp_{}", id),
    }
}

fn type_to_llvm(ty: &Type) -> String {
    match ty {
        Type::Unit => "i64".to_string(),
        Type::Bool | Type::U8 => "i8".to_string(),
        Type::U16 => "i16".to_string(),
        Type::U32 => "i32".to_string(),
        Type::I64 | Type::U64 | Type::Usize => "i64".to_string(),
        Type::String | Type::Ref { .. } | Type::Ptr { .. } | Type::Struct(_) | Type::Instantiated { .. } | Type::Fn { .. } | Type::Enum(_) | Type::Array { .. } => {
            "ptr".to_string()
        }
        Type::Refined { base, .. } | Type::Relational { base, .. } => type_to_llvm(base),
        _ => "i64".to_string(),
    }
}

fn type_to_llvm_ret(ty: &Type) -> String {
    match ty {
        Type::Unit => "void".to_string(),
        _ => type_to_llvm(ty),
    }
}

fn type_expr_to_llvm_str(te: &tungsten_syntax::ast::TypeExpr, is_ret: bool) -> String {
    use tungsten_syntax::ast::TypeExpr;
    match te {
        TypeExpr::Unit(_) => if is_ret { "void".to_string() } else { "i64".to_string() },
        TypeExpr::Ptr { .. } | TypeExpr::Ref { .. } => "ptr".to_string(),
        TypeExpr::Named(name, _) => {
            match name.as_str() {
                "u8" | "i8" | "bool" => "i8".to_string(),
                "u16" | "i16" => "i16".to_string(),
                "u32" | "i32" => "i32".to_string(),
                "u64" | "i64" | "usize" | "isize" => "i64".to_string(),
                "void" | "()" => if is_ret { "void".to_string() } else { "i64".to_string() },
                "String" => "ptr".to_string(),
                _ => "ptr".to_string(),
            }
        }
        _ => "ptr".to_string(),
    }
}

fn escape_llvm_string(s: &str) -> String {
    let mut res = String::new();
    for b in s.bytes() {
        if b >= 32 && b <= 126 && b != b'\\' && b != b'"' {
            res.push(b as char);
        } else {
            res.push_str(&format!("\\{:02X}", b));
        }
    }
    res
}

pub fn emit_llvm_ir(module: &TirModule) -> String {
    let emitter = LlvmTextEmitter::new(module);
    emitter.emit()
}
