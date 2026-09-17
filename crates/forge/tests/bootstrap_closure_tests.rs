use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::time::Duration;
use sha2::{Digest, Sha256};

use forge::builder::{build_target, BuildOptions};

fn run_command_with_retry(cmd: &mut Command, desc: &str) -> std::process::Output {
    let mut out_res = None;
    for attempt in 0..15 {
        match cmd.output() {
            Ok(out) => {
                out_res = Some(out);
                break;
            }
            Err(e) if e.raw_os_error() == Some(5) => {
                std::thread::sleep(Duration::from_millis(50 * (attempt + 1)));
            }
            Err(e) => panic!("Failed to run {}: {}", desc, e),
        }
    }
    let output = out_res.expect("Process execution must produce output");
    if !output.status.success() {
        eprintln!(
            "Process failed: {}\nStatus: {:?}\nStdout: {}\nStderr: {}",
            desc,
            output.status,
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
    assert!(output.status.success(), "{} must succeed with code 0", desc);
    output
}

#[test]
fn test_milestone1_full_self_hosting_bootstrap_closure() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf();

    let bootstrap_dir = root.join("target").join("bootstrap");
    fs::create_dir_all(&bootstrap_dir).expect("target/bootstrap directory must be created");

    let compiler_main = root.join("compiler").join("main.tg");
    assert!(
        compiler_main.is_file(),
        "compiler/main.tg must exist at {}",
        compiler_main.display()
    );

    let stage1_exe = bootstrap_dir.join("tgc_stage1.exe");
    let stage2_exe = bootstrap_dir.join("tgc_stage2.exe");
    let stage2_ll = bootstrap_dir.join("tgc_stage2.exe.ll");
    let stage3_exe = bootstrap_dir.join("tgc_stage3.exe");
    let stage3_ll = bootstrap_dir.join("tgc_stage3.exe.ll");

    // =========================================================================
    // Stage 1: Rust `forge` compiles `compiler/main.tg` -> `tgc_stage1.exe`
    // =========================================================================
    println!("[Bootstrap] Stage 1: Compiling compiler/main.tg with Forge...");
    let opts = BuildOptions {
        release: true,
        emit_llvm: false,
        emit_asm: false,
        custom_out: Some(stage1_exe.clone()),
        target: None,
    };

    let built_stage1 = build_target(Some(compiler_main.to_str().unwrap()), &opts)
        .expect("Forge must successfully compile compiler/main.tg into stage 1");
    assert!(built_stage1.is_file(), "tgc_stage1.exe must be produced");

    // =========================================================================
    // Stage 2: `tgc_stage1.exe` compiles `compiler/main.tg` -> `tgc_stage2.exe`
    // =========================================================================
    println!("[Bootstrap] Stage 2: Compiling compiler/main.tg with tgc_stage1...");
    let mut cmd_stage2 = Command::new(&stage1_exe);
    cmd_stage2
        .current_dir(&root)
        .args(&["compiler/main.tg", "-o", stage2_exe.to_str().unwrap()]);
    let out_stage2 = run_command_with_retry(&mut cmd_stage2, "tgc_stage1 -> tgc_stage2");
    let stage2_stdout = String::from_utf8_lossy(&out_stage2.stdout);
    assert!(
        stage2_stdout.contains("TGC: Bootstrap Stage 1 compilation completed successfully!"),
        "tgc_stage1 stdout must indicate success. Stdout: {}",
        stage2_stdout
    );
    assert!(stage2_exe.is_file(), "tgc_stage2.exe must exist");
    assert!(stage2_ll.is_file(), "tgc_stage2.exe.ll must exist");

    // =========================================================================
    // Stage 3: `tgc_stage2.exe` compiles `compiler/main.tg` -> `tgc_stage3.exe`
    // =========================================================================
    println!("[Bootstrap] Stage 3: Compiling compiler/main.tg with tgc_stage2...");
    let mut cmd_stage3 = Command::new(&stage2_exe);
    cmd_stage3
        .current_dir(&root)
        .args(&["compiler/main.tg", "-o", stage3_exe.to_str().unwrap()]);
    let out_stage3 = run_command_with_retry(&mut cmd_stage3, "tgc_stage2 -> tgc_stage3");
    let stage3_stdout = String::from_utf8_lossy(&out_stage3.stdout);
    assert!(
        stage3_stdout.contains("TGC: Bootstrap Stage 1 compilation completed successfully!"),
        "tgc_stage2 stdout must indicate success. Stdout: {}",
        stage3_stdout
    );
    assert!(stage3_exe.is_file(), "tgc_stage3.exe must exist");
    assert!(stage3_ll.is_file(), "tgc_stage3.exe.ll must exist");

    // =========================================================================
    // Convergence: Bitwise Fixed-Point LLVM IR Identity (Stage 2 == Stage 3)
    // =========================================================================
    println!("[Bootstrap] Verifying fixed-point LLVM IR bitwise convergence...");
    let stage2_bytes = fs::read(&stage2_ll).expect("Must read tgc_stage2.exe.ll");
    let stage3_bytes = fs::read(&stage3_ll).expect("Must read tgc_stage3.exe.ll");

    let mut hasher2 = Sha256::new();
    hasher2.update(&stage2_bytes);
    let hash2 = format!("{:x}", hasher2.finalize());

    let mut hasher3 = Sha256::new();
    hasher3.update(&stage3_bytes);
    let hash3 = format!("{:x}", hasher3.finalize());

    println!("[Bootstrap] Stage 2 LLVM IR SHA256: {}", hash2);
    println!("[Bootstrap] Stage 3 LLVM IR SHA256: {}", hash3);
    assert_eq!(
        hash2, hash3,
        "Bitwise LLVM IR convergence failed: Stage 2 and Stage 3 must be identical fixed-point!"
    );

    // =========================================================================
    // Verification: `tgc_stage3.exe` compiles `examples/bootstrap_sample.tg`
    // =========================================================================
    println!("[Bootstrap] Verifying tgc_stage3 compiles and runs bootstrap_sample.tg...");
    let sample_stage3_exe = root.join("examples").join("bootstrap_sample_stage3.exe");
    let mut cmd_sample = Command::new(&stage3_exe);
    cmd_sample.current_dir(&root).args(&[
        "examples/bootstrap_sample.tg",
        "-o",
        sample_stage3_exe.to_str().unwrap(),
    ]);
    run_command_with_retry(&mut cmd_sample, "tgc_stage3 -> bootstrap_sample_stage3.exe");
    assert!(
        sample_stage3_exe.is_file(),
        "bootstrap_sample_stage3.exe must exist"
    );

    let mut cmd_run = Command::new(&sample_stage3_exe);
    cmd_run.current_dir(&root);
    let out_run = run_command_with_retry(&mut cmd_run, "Run bootstrap_sample_stage3.exe");
    let sample_stdout = String::from_utf8_lossy(&out_run.stdout);

    assert!(
        sample_stdout.contains("55"),
        "Must output sum of 1..10 (55). Stdout: {}",
        sample_stdout
    );
    assert!(
        sample_stdout.contains("120"),
        "Must output 5! (120). Stdout: {}",
        sample_stdout
    );
    assert!(
        sample_stdout.contains("42"),
        "Must output add(40, 2) (42). Stdout: {}",
        sample_stdout
    );

    println!("[Bootstrap] Milestone v1.0 Self-Hosting Bootstrap Closure COMPLETE!");
}

#[test]
fn test_pure_tungsten_typeck_refinements() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf();

    let bootstrap_dir = root.join("target").join("bootstrap");
    let stage1_exe = bootstrap_dir.join("tgc_stage1.exe");
    assert!(stage1_exe.is_file(), "tgc_stage1.exe must exist (run bootstrap closure test first)");

    let fixtures_dir = root.join("target").join("fixtures");
    fs::create_dir_all(&fixtures_dir).unwrap();

    // 1. Positive Refinement Test: in-bounds values compile cleanly
    let valid_src = fixtures_dir.join("valid_refinement.tg");
    fs::write(
        &valid_src,
        "fn main() {\n    let p: Percentage = 85;\n    let port: Port = 8080;\n}\n",
    ).unwrap();
    let valid_exe = fixtures_dir.join("valid_refinement.exe");

    let mut cmd_valid = Command::new(&stage1_exe);
    cmd_valid.current_dir(&root).args(&[
        valid_src.to_str().unwrap(),
        "-o",
        valid_exe.to_str().unwrap(),
    ]);
    let out_valid = run_command_with_retry(&mut cmd_valid, "tgc_stage1 -> valid_refinement.exe");
    assert!(out_valid.status.success(), "Valid refinement must compile successfully");

    // 2. Negative Refinement Test: out-of-bounds assignment triggers typecheck error
    let invalid_src = fixtures_dir.join("invalid_refinement.tg");
    fs::write(
        &invalid_src,
        "fn main() {\n    let mut p: Percentage = 50;\n    p = 150;\n}\n",
    ).unwrap();
    let invalid_exe = fixtures_dir.join("invalid_refinement.exe");

    let mut cmd_invalid = Command::new(&stage1_exe);
    cmd_invalid.current_dir(&root).args(&[
        invalid_src.to_str().unwrap(),
        "-o",
        invalid_exe.to_str().unwrap(),
    ]);
    let out_invalid = cmd_invalid.output().expect("Execution must produce output");
    assert!(!out_invalid.status.success(), "Out-of-bounds refinement must fail compilation");
    let output_invalid = format!(
        "{}\n{}",
        String::from_utf8_lossy(&out_invalid.stdout),
        String::from_utf8_lossy(&out_invalid.stderr)
    );
    assert!(
        output_invalid.contains("Refinement Constraint Violation") || output_invalid.contains("Type checking failed"),
        "Must report refinement constraint violation! Output: {}",
        output_invalid
    );
}

#[test]
fn test_pure_tungsten_tir_and_opt() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf();

    let bootstrap_dir = root.join("target").join("bootstrap");
    let stage1_exe = bootstrap_dir.join("tgc_stage1.exe");
    assert!(stage1_exe.is_file(), "tgc_stage1.exe must exist (run bootstrap closure test first)");

    let fixtures_dir = root.join("target").join("fixtures");
    fs::create_dir_all(&fixtures_dir).unwrap();

    let tir_test_src = fixtures_dir.join("tir_test.tg");
    fs::write(
        &tir_test_src,
        "fn main() {\n    let a = 10 + 20;\n    let b = 30 * 2;\n    println(a + b);\n}\n",
    ).unwrap();
    let tir_test_exe = fixtures_dir.join("tir_test.exe");

    let mut cmd_tir = Command::new(&stage1_exe);
    cmd_tir.current_dir(&root).args(&[
        tir_test_src.to_str().unwrap(),
        "-o",
        tir_test_exe.to_str().unwrap(),
    ]);
    let out_tir = run_command_with_retry(&mut cmd_tir, "tgc_stage1 -> tir_test.exe");
    let tir_stdout = String::from_utf8_lossy(&out_tir.stdout);
    assert!(
        tir_stdout.contains("TGC: Lowering to TIR & optimizing..."),
        "stdout must show TIR lowering and optimization. Stdout: {}",
        tir_stdout
    );
    assert!(tir_test_exe.is_file(), "tir_test.exe must be compiled");

    let mut cmd_run = Command::new(&tir_test_exe);
    cmd_run.current_dir(&root);
    let out_run = run_command_with_retry(&mut cmd_run, "Run tir_test.exe");
    let run_stdout = String::from_utf8_lossy(&out_run.stdout);
    assert!(
        run_stdout.contains("90"),
        "tir_test.exe must compute 30 + 60 = 90. Stdout: {}",
        run_stdout
    );
}

#[test]
fn test_pure_tungsten_forge_and_fmt() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf();

    let bootstrap_dir = root.join("target").join("bootstrap");
    let stage1_exe = bootstrap_dir.join("tgc_stage1.exe");
    assert!(stage1_exe.is_file(), "tgc_stage1.exe must exist (run bootstrap closure test first)");

    let fixtures_dir = root.join("target").join("fixtures");
    fs::create_dir_all(&fixtures_dir).unwrap();

    // 1. Test: forge version
    let mut cmd_ver = Command::new(&stage1_exe);
    cmd_ver.current_dir(&root).args(&["version"]);
    let out_ver = run_command_with_retry(&mut cmd_ver, "forge version");
    let ver_stdout = String::from_utf8_lossy(&out_ver.stdout);
    assert!(
        ver_stdout.contains("forge 0.2.0 (pure tungsten genesis 2026)"),
        "Version output mismatch: {}",
        ver_stdout
    );

    // 2. Test: forge help
    let mut cmd_help = Command::new(&stage1_exe);
    cmd_help.current_dir(&root).args(&["help"]);
    let out_help = run_command_with_retry(&mut cmd_help, "forge help");
    let help_stdout = String::from_utf8_lossy(&out_help.stdout);
    assert!(
        help_stdout.contains("Usage: forge <command> [options]"),
        "Help output mismatch: {}",
        help_stdout
    );

    // 3. Test: forge check on valid refinement file
    let valid_src = fixtures_dir.join("forge_check_valid.tg");
    fs::write(
        &valid_src,
        "fn main() {\n    let p: Percentage = 99;\n}\n",
    ).unwrap();

    let mut cmd_chk_ok = Command::new(&stage1_exe);
    cmd_chk_ok.current_dir(&root).args(&["check", valid_src.to_str().unwrap()]);
    let out_chk_ok = run_command_with_retry(&mut cmd_chk_ok, "forge check valid");
    let chk_ok_stdout = String::from_utf8_lossy(&out_chk_ok.stdout);
    assert!(
        chk_ok_stdout.contains("forge: check passed: 0 errors"),
        "forge check should report 0 errors. Stdout: {}",
        chk_ok_stdout
    );

    // 4. Test: forge check on invalid refinement file
    let invalid_src = fixtures_dir.join("forge_check_invalid.tg");
    fs::write(
        &invalid_src,
        "fn main() {\n    let mut p: Percentage = 50;\n    p = 200;\n}\n",
    ).unwrap();

    let mut cmd_chk_err = Command::new(&stage1_exe);
    cmd_chk_err.current_dir(&root).args(&["check", invalid_src.to_str().unwrap()]);
    let out_chk_err = cmd_chk_err.output().expect("Command must produce output");
    assert!(!out_chk_err.status.success(), "forge check must fail on refinement violation");

    // 5. Test: forge fmt
    let unformatted_src = fixtures_dir.join("unformatted.tg");
    fs::write(
        &unformatted_src,
        "fn add(a: i64, b: i64) -> i64 { return a + b; }\n",
    ).unwrap();

    let mut cmd_fmt = Command::new(&stage1_exe);
    cmd_fmt.current_dir(&root).args(&["fmt", unformatted_src.to_str().unwrap()]);
    let out_fmt = run_command_with_retry(&mut cmd_fmt, "forge fmt");
    let fmt_stdout = String::from_utf8_lossy(&out_fmt.stdout);
    assert!(
        fmt_stdout.contains("Formatted file"),
        "forge fmt must format the file. Stdout: {}",
        fmt_stdout
    );

    // Verify idempotency: second format reports 'Already formatted'
    let mut cmd_fmt2 = Command::new(&stage1_exe);
    cmd_fmt2.current_dir(&root).args(&["fmt", "--check", unformatted_src.to_str().unwrap()]);
    let out_fmt2 = run_command_with_retry(&mut cmd_fmt2, "forge fmt --check");
    assert!(out_fmt2.status.success(), "forge fmt --check should succeed on formatted file");
}



