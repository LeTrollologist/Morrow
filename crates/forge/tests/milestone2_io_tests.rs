use std::fs;
use std::process::Command;
use std::time::Duration;
use tempfile::TempDir;

use forge::builder::{build_target, BuildOptions};

fn run_binary(exe_path: &std::path::Path) -> std::process::Output {
    let mut out_res = None;
    for attempt in 0..15 {
        match Command::new(exe_path).output() {
            Ok(out) => {
                out_res = Some(out);
                break;
            }
            Err(e) if e.raw_os_error() == Some(5) => {
                std::thread::sleep(Duration::from_millis(25 * (attempt + 1)));
            }
            Err(e) => panic!("Failed to run binary {}: {}", exe_path.display(), e),
        }
    }
    out_res.expect("Binary execution must succeed")
}

#[test]
fn test_milestone2_fs_operations() {
    let temp = TempDir::new().unwrap();
    let src_path = temp.path().join("main.tg");

    let code = r#"
    fn main() yields [FS, IO] {
        let path = "m2_test_file.txt";
        let content = "Hello from Tungsten std::fs!";

        let wrote = fs_write_string(path, content);
        if !wrote {
            println!("WRITE_FAILED");
            return;
        }

        if !fs_exists(path) {
            println!("EXISTS_FAILED");
            return;
        }

        let sz = fs_size(path);
        if sz <= 0 {
            println!("SIZE_FAILED");
            return;
        }

        let read_back = fs_read_to_string(path);
        println!("READ_BACK: {}", read_back);

        let deleted = fs_delete(path);
        if !deleted {
            println!("DELETE_FAILED");
            return;
        }

        if fs_exists(path) {
            println!("STILL_EXISTS_FAILED");
            return;
        }

        println!("FS_SUCCESS");
    }
    "#;

    fs::write(&src_path, code).unwrap();

    let options = BuildOptions {
        release: false,
        emit_llvm: false,
        emit_asm: false,
        custom_out: None,
    };

    let target_str = src_path.to_str().unwrap();
    let exe_path = build_target(Some(target_str), &options).expect("FS build should succeed");
    let out = run_binary(&exe_path);
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("FS_SUCCESS"), "Stdout was: {}", stdout);
    assert!(stdout.contains("Hello from Tungsten std::fs!"), "Stdout was: {}", stdout);
}

#[test]
fn test_milestone2_fs_region_read_zero_copy() {
    let temp = TempDir::new().unwrap();
    let src_path = temp.path().join("main.tg");

    let code = r#"
    fn main() yields [FS, IO] {
        let path = "m2_region_file.txt";
        fs_write_string(path, "Region ingested buffer content");

        region r {
            let content = fs_read_to_string_in(path, r);
            println!("INGESTED: {}", content);
        }

        fs_delete(path);
        println!("REGION_FS_SUCCESS");
    }
    "#;

    fs::write(&src_path, code).unwrap();

    let options = BuildOptions {
        release: true,
        emit_llvm: false,
        emit_asm: false,
        custom_out: None,
    };

    let target_str = src_path.to_str().unwrap();
    let exe_path = build_target(Some(target_str), &options).expect("Region FS build should succeed");
    let out = run_binary(&exe_path);
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("REGION_FS_SUCCESS"), "Stdout was: {}", stdout);
    assert!(stdout.contains("Region ingested buffer content"), "Stdout was: {}", stdout);
}

#[test]
fn test_milestone2_process_execution() {
    let temp = TempDir::new().unwrap();
    let src_path = temp.path().join("main.tg");

    let code = r#"
    fn main() yields [Process, IO] {
        let mut cmd = command_new("cmd.exe");
        command_arg(&mut cmd, "/c echo Milestone2 Process Execution Validated");
        let status = command_status(&cmd);
        println!("PROC_STATUS: {}", status);
        if status == 0 {
            println!("PROCESS_SUCCESS");
        }
    }
    "#;

    fs::write(&src_path, code).unwrap();

    let options = BuildOptions {
        release: false,
        emit_llvm: false,
        emit_asm: false,
        custom_out: None,
    };

    let target_str = src_path.to_str().unwrap();
    let exe_path = build_target(Some(target_str), &options).expect("Process build should succeed");
    let out = run_binary(&exe_path);
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("PROCESS_SUCCESS"), "Stdout was: {}", stdout);
    assert!(stdout.contains("PROC_STATUS: 0"), "Stdout was: {}", stdout);
}

#[test]
fn test_milestone2_robin_hood_hashmap() {
    let temp = TempDir::new().unwrap();
    let src_path = temp.path().join("main.tg");

    let code = r#"
    fn main() yields [IO] {
        region r {
            let mut map = hashmap_new_in(r);

            // Insert initial entries
            hashmap_insert(&mut map, "apple", 100);
            hashmap_insert(&mut map, "banana", 200);
            hashmap_insert(&mut map, "cherry", 300);
            hashmap_insert(&mut map, "durian", 400);

            if hashmap_len(&map) != 4 {
                println!("LEN_FAIL");
                return;
            }

            if hashmap_get(&map, "apple") != 100 {
                println!("GET_APPLE_FAIL");
                return;
            }
            if hashmap_get(&map, "banana") != 200 {
                println!("GET_BANANA_FAIL");
                return;
            }
            if hashmap_get(&map, "cherry") != 300 {
                println!("GET_CHERRY_FAIL");
                return;
            }
            if hashmap_get(&map, "durian") != 400 {
                println!("GET_DURIAN_FAIL");
                return;
            }

            // Test non-existent key
            if hashmap_contains(&map, "elderberry") {
                println!("CONTAINS_NONEXISTENT_FAIL");
                return;
            }

            // Test Robin Hood backward-shift removal
            let removed = hashmap_remove(&mut map, "banana");
            if !removed || hashmap_len(&map) != 3 {
                println!("REMOVE_BANANA_FAIL");
                return;
            }

            if hashmap_contains(&map, "banana") {
                println!("REMOVED_STILL_PRESENT_FAIL");
                return;
            }

            // Verify other keys still intact after backward shift
            if hashmap_get(&map, "apple") != 100 || hashmap_get(&map, "cherry") != 300 || hashmap_get(&map, "durian") != 400 {
                println!("POST_REMOVE_LOOKUP_FAIL");
                return;
            }

            println!("ROBIN_HOOD_SUCCESS");
        }
    }
    "#;

    fs::write(&src_path, code).unwrap();

    let options = BuildOptions {
        release: false,
        emit_llvm: false,
        emit_asm: false,
        custom_out: None,
    };

    let target_str = src_path.to_str().unwrap();
    let exe_path = build_target(Some(target_str), &options).expect("HashMap build should succeed");
    let out = run_binary(&exe_path);
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(out.status.success(), "HashMap binary failed with status {:?}. Stdout: {}, Stderr: {}", out.status, stdout, stderr);
    assert!(stdout.contains("ROBIN_HOOD_SUCCESS"), "Stdout was: {}", stdout);
}

#[test]
fn test_milestone2_self_hosting_bridge_full_demo() {
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf();
    let script_path = root.join("examples").join("self_hosting_bridge.tg");
    assert!(script_path.exists(), "self_hosting_bridge.tg must exist at {}", script_path.display());

    let options = BuildOptions {
        release: true,
        emit_llvm: false,
        emit_asm: false,
        custom_out: None,
    };

    let target_str = script_path.to_str().unwrap();
    let exe_path = build_target(Some(target_str), &options).expect("Self-hosting bridge build should succeed");
    let out = run_binary(&exe_path);
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(out.status.success(), "Self-hosting binary failed with status {:?}. Stdout: {}, Stderr: {}", out.status, stdout, stderr);
    assert!(stdout.contains("Self-Hosting Bridge Validation Complete: 100% SUCCESS"), "Stdout was: {}", stdout);
}
