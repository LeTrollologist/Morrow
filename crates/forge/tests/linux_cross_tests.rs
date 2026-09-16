use std::fs;
use std::process::Command;
use tempfile::TempDir;

use forge::builder::{build_target, BuildOptions};

#[test]
fn test_linux_cross_compilation_elf_headers() {
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf();

    let temp = TempDir::new().unwrap();
    let elf_binary = temp.path().join("bootstrap_sample_linux");

    let sample_tg = root.join("examples").join("linux_sample.tg");
    assert!(sample_tg.is_file(), "examples/linux_sample.tg must exist");

    let opts = BuildOptions {
        release: false,
        emit_llvm: false,
        emit_asm: false,
        custom_out: Some(elf_binary.clone()),
        target: Some("x86_64-unknown-linux-gnu".to_string()),
    };

    let built_file = build_target(Some(sample_tg.to_str().unwrap()), &opts)
        .expect("Cross-compilation to Linux ELF must succeed");
    assert!(built_file.is_file(), "Output ELF binary must exist");

    let bytes = fs::read(&built_file).expect("Failed to read ELF binary");
    assert!(bytes.len() >= 64, "ELF header must be at least 64 bytes");

    // Check ELF Magic: 0x7F 'E' 'L' 'F'
    assert_eq!(&bytes[0..4], &[0x7F, b'E', b'L', b'F'], "Magic must match ELF header");

    // EI_CLASS: 2 = 64-bit
    assert_eq!(bytes[4], 2, "Must be 64-bit ELF (EI_CLASS == 2)");

    // EI_DATA: 1 = little-endian
    assert_eq!(bytes[5], 1, "Must be little-endian (EI_DATA == 1)");

    // EI_VERSION: 1 = original ELF
    assert_eq!(bytes[6], 1, "Must be ELF version 1");

    // e_machine: x86-64 is 0x003E (little-endian: 0x3E, 0x00)
    assert_eq!(&bytes[18..20], &[0x3E, 0x00], "e_machine must be EM_X86_64 (62 / 0x3E)");
}

#[test]
fn test_linux_cross_web_service_elf() {
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf();

    let temp = TempDir::new().unwrap();
    let elf_binary = temp.path().join("web_service_linux");

    let web_tg = root.join("examples").join("web_service_v2.tg");
    assert!(web_tg.is_file(), "examples/web_service_v2.tg must exist");

    let opts = BuildOptions {
        release: false,
        emit_llvm: false,
        emit_asm: false,
        custom_out: Some(elf_binary.clone()),
        target: Some("x86_64-unknown-linux-gnu".to_string()),
    };

    let built_file = build_target(Some(web_tg.to_str().unwrap()), &opts)
        .expect("Cross-compilation of web_service_v2 to Linux ELF must succeed");
    assert!(built_file.is_file(), "Output ELF binary must exist");

    let bytes = fs::read(&built_file).expect("Failed to read ELF binary");
    assert!(bytes.len() >= 64);
    assert_eq!(&bytes[0..4], &[0x7F, b'E', b'L', b'F']);
    assert_eq!(bytes[4], 2);
    assert_eq!(bytes[5], 1);
    assert_eq!(&bytes[18..20], &[0x3E, 0x00]);
}

#[test]
fn test_linux_cross_execution_in_wsl() {
    if !cfg!(target_os = "windows") {
        return;
    }

    // Check if WSL is available
    let wsl_check = Command::new("wsl")
        .arg("-e")
        .arg("/bin/bash")
        .arg("-c")
        .arg("echo wsl_ok")
        .output();

    if let Ok(out) = wsl_check {
        if !out.status.success() || !String::from_utf8_lossy(&out.stdout).contains("wsl_ok") {
            eprintln!("WSL not accessible, skipping execution test");
            return;
        }
    } else {
        eprintln!("WSL not installed, skipping execution test");
        return;
    }

    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf();

    let temp = TempDir::new().unwrap();
    let elf_binary = temp.path().join("bootstrap_wsl_test");

    let sample_tg = root.join("examples").join("linux_sample.tg");

    let opts = BuildOptions {
        release: false,
        emit_llvm: false,
        emit_asm: false,
        custom_out: Some(elf_binary.clone()),
        target: Some("x86_64-unknown-linux-gnu".to_string()),
    };

    let built_file = build_target(Some(sample_tg.to_str().unwrap()), &opts)
        .expect("Cross-compilation must succeed");
    assert!(built_file.is_file());

    // Map Windows path to WSL /mnt/... path
    let abs_str = built_file.to_string_lossy().replace('\\', "/");
    let clean = abs_str.trim_start_matches("//?/").trim_start_matches("\\\\?\\");
    let wsl_path = if clean.len() >= 2 && clean.as_bytes()[1] == b':' {
        let drive = (clean.as_bytes()[0] as char).to_ascii_lowercase();
        format!("/mnt/{}{}", drive, &clean[2..])
    } else {
        clean.to_string()
    };

    let wsl_run = Command::new("wsl")
        .arg("-e")
        .arg("/bin/bash")
        .arg("-c")
        .arg(&wsl_path)
        .output()
        .expect("WSL execution must succeed");

    assert!(wsl_run.status.success(), "WSL process must exit with 0");
    let stdout = String::from_utf8_lossy(&wsl_run.stdout);
    assert!(stdout.contains("55"), "Output must contain factorial/fib result 55: {}", stdout);
    assert!(stdout.contains("120"), "Output must contain factorial result 120: {}", stdout);
    assert!(stdout.contains("42"), "Output must contain 42: {}", stdout);
}
