use std::path::PathBuf;
use std::process::Command;
use std::time::Duration;

use forge::bindgen::generate_bindings;
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
fn test_sqlite_benchmark_execution() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let root = manifest_dir.parent().unwrap().parent().unwrap();
    let example_src = root.join("examples").join("sqlite_benchmark.tg");

    assert!(example_src.exists(), "examples/sqlite_benchmark.tg must exist");

    let options = BuildOptions {
        release: false,
        emit_llvm: false,
        emit_asm: false,
        custom_out: None,
        target: None,
    };

    let target_str = example_src.to_str().unwrap();
    let exe_path = build_target(Some(target_str), &options).expect("SQLite benchmark build should succeed");
    let out = run_binary(&exe_path);
    assert!(out.status.success(), "Process exited with failure: {:?}", out.status);

    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("Row count is exactly 10,000"), "Stdout: {}", stdout);
    assert!(stdout.contains("Column count is exactly 3"), "Stdout: {}", stdout);
    assert!(stdout.contains("Column 0 is 'id'"), "Stdout: {}", stdout);
    assert!(stdout.contains("Column 1 is 'name'"), "Stdout: {}", stdout);
    assert!(stdout.contains("Column 2 is 'email'"), "Stdout: {}", stdout);
    assert!(stdout.contains("First row id is 1"), "Stdout: {}", stdout);
    assert!(stdout.contains("First row name is User_1"), "Stdout: {}", stdout);
    assert!(stdout.contains("Mid row id is 5001"), "Stdout: {}", stdout);
    assert!(stdout.contains("Last row id is 10000"), "Stdout: {}", stdout);
    assert!(stdout.contains("Row 42 view name is User_43"), "Stdout: {}", stdout);
    assert!(stdout.contains("bulk-freed in O(1)"), "Stdout: {}", stdout);
    assert!(stdout.contains("Database effect insert affected 1 row"), "Stdout: {}", stdout);
    assert!(stdout.contains("Database effect query returned FiberConcurrency"), "Stdout: {}", stdout);
    assert!(stdout.contains("All Phase C SQLite"), "Stdout: {}", stdout);
}

#[test]
fn test_sqlite_bindgen_generation() {
    let header = r#"
    #define SQLITE_OK 0
    #define SQLITE_ROW 100
    #define SQLITE_DONE 101

    int sqlite3_open(const char *filename, void **ppDb);
    int sqlite3_close(void *db);
    int sqlite3_prepare_v2(void *db, const char *zSql, int nByte, void **ppStmt, const char **pzTail);
    int sqlite3_step(void *pStmt);
    const unsigned char *sqlite3_column_text(void *pStmt, int iCol);
    int sqlite3_column_bytes(void *pStmt, int iCol);
    int sqlite3_finalize(void *pStmt);
    const char *sqlite3_errmsg(void *db);
    "#;

    let tg = generate_bindings(header).expect("Bindgen must succeed for SQLite header subset");
    assert!(tg.contains("sqlite3_open"));
    assert!(tg.contains("sqlite3_close"));
    assert!(tg.contains("sqlite3_prepare_v2"));
    assert!(tg.contains("sqlite3_step"));
    assert!(tg.contains("sqlite3_column_text"));
    assert!(tg.contains("sqlite3_finalize"));
    assert!(tg.contains("sqlite3_errmsg"));
    assert!(tg.contains("extern \"C\""));
}
