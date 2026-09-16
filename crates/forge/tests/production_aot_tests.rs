use std::fs;
use std::process::Command;
use std::time::Duration;
use tempfile::TempDir;

use forge::builder::{build_target, clean_target, get_latest_source_mtime, BuildOptions};

#[test]
fn test_forge_build_debug_and_pdb() {
    let temp = TempDir::new().unwrap();
    let src_path = temp.path().join("main.tg");

    let code = r#"
    fn add(a: i64, b: i64) -> i64 {
        let sum = a + b;
        sum
    }

    fn main() {
        let res = add(20, 22);
        println!("Debug Build Result: {}", res);
    }
    "#;

    fs::write(&src_path, code).unwrap();

    let options = BuildOptions {
        release: false,
        emit_llvm: false,
        emit_asm: false,
        custom_out: None,
        target: None,
    };

    let target_str = src_path.to_str().unwrap();
    let exe_path = build_target(Some(target_str), &options).expect("Debug build should succeed");

    assert!(exe_path.exists(), "Target executable must exist at {}", exe_path.display());
    let pdb_path = exe_path.with_extension("pdb");
    assert!(pdb_path.exists(), "Target PDB must exist at {}", pdb_path.display());

    // Execute the built native binary directly
    let mut out_res = None;
    for attempt in 0..15 {
        match Command::new(&exe_path).output() {
            Ok(out) => {
                out_res = Some(out);
                break;
            }
            Err(e) if e.raw_os_error() == Some(5) => {
                std::thread::sleep(Duration::from_millis(25 * (attempt + 1)));
            }
            Err(e) => panic!("Failed to run binary: {}", e),
        }
    }

    let out = out_res.expect("Execution must succeed after retries");
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("Debug Build Result: 42"), "Unexpected output: {}", stdout);
}

#[test]
fn test_forge_build_release_and_lto() {
    let temp = TempDir::new().unwrap();
    let src_path = temp.path().join("main.tg");

    let code = r#"
    fn factorial(n: i64) -> i64 {
        if n <= 1 {
            return 1;
        }
        return n * factorial(n - 1);
    }

    fn main() {
        let f5 = factorial(5);
        println!("Release Factorial: {}", f5);
    }
    "#;

    fs::write(&src_path, code).unwrap();

    let options = BuildOptions {
        release: true,
        emit_llvm: false,
        emit_asm: false,
        custom_out: None,
        target: None,
    };

    let target_str = src_path.to_str().unwrap();
    let exe_path = build_target(Some(target_str), &options).expect("Release build should succeed");

    assert!(exe_path.exists(), "Release executable must exist at {}", exe_path.display());
    assert!(exe_path.to_string_lossy().contains("release"), "Path must be in target/release");

    let pdb_path = exe_path.with_extension("pdb");
    assert!(pdb_path.exists(), "Release PDB must exist at {}", pdb_path.display());

    let mut out_res = None;
    for attempt in 0..15 {
        match Command::new(&exe_path).output() {
            Ok(out) => {
                out_res = Some(out);
                break;
            }
            Err(e) if e.raw_os_error() == Some(5) => {
                std::thread::sleep(Duration::from_millis(25 * (attempt + 1)));
            }
            Err(e) => panic!("Failed to run binary: {}", e),
        }
    }

    let out = out_res.expect("Execution must succeed after retries");
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("Release Factorial: 120"), "Unexpected output: {}", stdout);
}

#[test]
fn test_forge_build_emit_llvm_and_asm() {
    let temp = TempDir::new().unwrap();
    let src_path = temp.path().join("main.tg");

    let code = r#"
    fn multiply(x: i64, y: i64) -> i64 {
        x * y
    }

    fn main() {
        let m = multiply(6, 7);
        println!("Multiplied: {}", m);
    }
    "#;

    fs::write(&src_path, code).unwrap();

    let options = BuildOptions {
        release: false,
        emit_llvm: true,
        emit_asm: true,
        custom_out: None,
        target: None,
    };

    let target_str = src_path.to_str().unwrap();
    let exe_path = build_target(Some(target_str), &options).expect("Build with --emit-llvm/--emit-asm should succeed");

    let ll_path = exe_path.with_extension("ll");
    let s_path = exe_path.with_extension("s");

    assert!(ll_path.exists(), "Intermediate LLVM IR (.ll) must exist at {}", ll_path.display());
    assert!(s_path.exists(), "Intermediate Assembly (.s) must exist at {}", s_path.display());

    let ll_content = fs::read_to_string(&ll_path).unwrap();
    assert!(ll_content.contains("define i64 @multiply"));
    assert!(ll_content.contains("!DICompileUnit"));

    let s_content = fs::read_to_string(&s_path).unwrap();
    assert!(s_content.contains(".text") || s_content.contains("multiply:"));
}

#[test]
fn test_forge_clean_target_removal() {
    let temp = TempDir::new().unwrap();
    let src_path = temp.path().join("main.tg");

    let code = r#"
    fn main() {
        println!("Hello clean!");
    }
    "#;

    fs::write(&src_path, code).unwrap();

    let options = BuildOptions::default();
    let target_str = src_path.to_str().unwrap();
    let exe_path = build_target(Some(target_str), &options).expect("Build should succeed");

    let target_dir = temp.path().join("target");
    assert!(target_dir.exists(), "target/ directory must exist after build");
    assert!(exe_path.exists());

    let cleaned = clean_target(Some(target_str)).expect("Clean must succeed");
    assert_eq!(cleaned, target_dir);
    assert!(!target_dir.exists(), "target/ directory must be deleted after clean");
}

#[test]
fn test_forge_incremental_timestamp_detection() {
    let temp = TempDir::new().unwrap();
    let src_path = temp.path().join("main.tg");

    let code1 = r#"
    fn main() {
        println!("Version 1");
    }
    "#;

    fs::write(&src_path, code1).unwrap();

    let target_str = src_path.to_str().unwrap();
    let options = BuildOptions::default();
    let exe_path = build_target(Some(target_str), &options).expect("Initial build should succeed");

    let exe_mtime_1 = fs::metadata(&exe_path).unwrap().modified().unwrap();
    let src_mtime_1 = get_latest_source_mtime(Some(target_str)).unwrap();

    // Binary mtime is >= source mtime initially
    assert!(exe_mtime_1 >= src_mtime_1);

    // Sleep briefly so file system mtime advances
    std::thread::sleep(Duration::from_millis(50));

    // Modify source file
    let code2 = r#"
    fn main() {
        println!("Version 2");
    }
    "#;
    fs::write(&src_path, code2).unwrap();

    let src_mtime_2 = get_latest_source_mtime(Some(target_str)).unwrap();
    assert!(src_mtime_2 > exe_mtime_1, "Source file modification must be newer than old binary");

    // Rebuild target
    let exe_path_2 = build_target(Some(target_str), &options).expect("Incremental rebuild should succeed");
    let exe_mtime_2 = fs::metadata(&exe_path_2).unwrap().modified().unwrap();
    assert!(exe_mtime_2 >= src_mtime_2, "New binary must be up to date with new source");
}

#[test]
fn test_forge_embedded_std_fallback_isolated_dir() {
    // Test that a program using std features can build in an isolated directory
    // with NO std/ directory anywhere in its ancestry!
    let temp = TempDir::new().unwrap();
    let standalone_dir = temp.path().join("isolated_workspace");
    fs::create_dir_all(&standalone_dir).unwrap();

    let src_path = standalone_dir.join("main.tg");

    // This code uses Refinement types (from std/refinements.tg or prelude.tg)
    let code = r#"
    type Percentage = u8(0..=100);

    fn check_score(score: Percentage) -> i64 {
        let val: i64 = score as i64;
        val * 2
    }

    fn main() {
        let p: Percentage = 50 as Percentage;
        let doubled = check_score(p);
        println!("Doubled percentage: {}", doubled);
    }
    "#;

    fs::write(&src_path, code).unwrap();

    // Verify there is NO std folder in standalone_dir or temp
    assert!(!standalone_dir.join("std").exists());
    assert!(!temp.path().join("std").exists());

    let target_str = src_path.to_str().unwrap();
    let options = BuildOptions::default();
    let exe_path = build_target(Some(target_str), &options)
        .expect("Build with embedded std fallback should succeed in isolated directory");

    assert!(exe_path.exists());

    let mut out_res = None;
    for attempt in 0..15 {
        match Command::new(&exe_path).output() {
            Ok(out) => {
                out_res = Some(out);
                break;
            }
            Err(e) if e.raw_os_error() == Some(5) => {
                std::thread::sleep(Duration::from_millis(25 * (attempt + 1)));
            }
            Err(e) => panic!("Failed to run binary: {}", e),
        }
    }

    let out = out_res.expect("Execution must succeed after retries");
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("Doubled percentage: 100"), "Unexpected output: {}", stdout);
}
