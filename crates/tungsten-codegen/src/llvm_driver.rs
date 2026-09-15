use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use tungsten_tir::ir::TirModule;
use crate::llvm_text::emit_llvm_ir;

pub struct LlvmToolchain {
    pub clang_path: PathBuf,
    pub lld_path: PathBuf,
    pub crt_dir: PathBuf,
}

impl LlvmToolchain {
    pub fn discover() -> Result<Self, String> {
        // 1. Locate clang.exe
        let mut clang_path = None;
        let default_llvm_path = PathBuf::from("C:\\Program Files\\LLVM\\bin\\clang.exe");
        if default_llvm_path.is_file() {
            clang_path = Some(default_llvm_path);
        } else if let Ok(output) = Command::new("where").arg("clang").output() {
            if output.status.success() {
                let s = String::from_utf8_lossy(&output.stdout);
                if let Some(first_line) = s.lines().next() {
                    let p = PathBuf::from(first_line.trim());
                    if p.is_file() {
                        clang_path = Some(p);
                    }
                }
            }
        }

        let clang_path = clang_path.ok_or_else(|| {
            "Could not locate clang.exe. Ensure LLVM is installed in 'C:\\Program Files\\LLVM' or on PATH.".to_string()
        })?;

        // 2. Locate rust sysroot to find rust-lld and self-contained MinGW CRT
        let sysroot_output = Command::new("rustc")
            .arg("--print")
            .arg("sysroot")
            .output()
            .map_err(|e| format!("Failed to run rustc: {}", e))?;

        if !sysroot_output.status.success() {
            return Err("Failed to query rustc sysroot".to_string());
        }

        let sysroot_str = String::from_utf8_lossy(&sysroot_output.stdout).trim().to_string();
        let sysroot = PathBuf::from(sysroot_str);

        let target_dir = sysroot.join("lib").join("rustlib").join("x86_64-pc-windows-gnu");
        let lld_path = target_dir.join("bin").join("rust-lld.exe");
        let crt_dir = target_dir.join("lib").join("self-contained");

        if !lld_path.is_file() {
            return Err(format!("Could not locate rust-lld at {}", lld_path.display()));
        }
        if !crt_dir.is_dir() {
            return Err(format!("Could not locate self-contained CRT directory at {}", crt_dir.display()));
        }

        Ok(Self {
            clang_path,
            lld_path,
            crt_dir,
        })
    }
}

pub fn compile_llvm_aot(
    module: &TirModule,
    out_exe: &Path,
    opt_level: &str,
) -> Result<(), String> {
    let toolchain = LlvmToolchain::discover()?;
    let ir_text = emit_llvm_ir(module);

    let temp_dir = env::temp_dir();
    let unique_id = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();

    let ll_path = temp_dir.join(format!("tungsten_{}.ll", unique_id));
    let o_path = temp_dir.join(format!("tungsten_{}.o", unique_id));

    fs::write(&ll_path, ir_text)
        .map_err(|e| format!("Failed to write LLVM IR to {}: {}", ll_path.display(), e))?;

    // Compile .ll to .o using clang with optimization
    let clang_opt = format!("-{}", opt_level);
    let mut clang_cmd = Command::new(&toolchain.clang_path);
    clang_cmd
        .arg("--target=x86_64-pc-windows-gnu")
        .arg("-c")
        .arg(&ll_path)
        .arg(&clang_opt)
        .arg("-o")
        .arg(&o_path);

    let clang_res = clang_cmd.output().map_err(|e| format!("Failed to run clang: {}", e))?;
    if !clang_res.status.success() {
        let err = String::from_utf8_lossy(&clang_res.stderr);
        let _ = fs::remove_file(&ll_path);
        return Err(format!("LLVM clang compilation error:\n{}", err));
    }

    // Link .o to .exe using rust-lld
    let crt2_path = toolchain.crt_dir.join("crt2.o");
    let mut lld_cmd = Command::new(&toolchain.lld_path);
    lld_cmd
        .arg("-flavor")
        .arg("gnu")
        .arg("-m")
        .arg("i386pep")
        .arg("-Bdynamic")
        .arg("-o")
        .arg(out_exe)
        .arg(&crt2_path)
        .arg(&o_path)
        .arg(format!("-L{}", toolchain.crt_dir.display()))
        .arg("-lmingw32")
        .arg("-lmingwex")
        .arg("-lmsvcrt")
        .arg("-lkernel32")
        .arg("-luser32")
        .arg("-lws2_32")
        .arg("-lgcc");

    let lld_res = lld_cmd.output().map_err(|e| format!("Failed to run rust-lld: {}", e))?;

    // Cleanup temp files
    let _ = fs::remove_file(&ll_path);
    let _ = fs::remove_file(&o_path);

    if !lld_res.status.success() {
        let err = String::from_utf8_lossy(&lld_res.stderr);
        return Err(format!("LLVM rust-lld linking error:\n{}", err));
    }

    Ok(())
}

pub fn run_llvm_aot(module: &TirModule) -> Result<(i32, String), String> {
    let temp_dir = env::temp_dir();
    let unique_id = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();

    let exe_path = temp_dir.join(format!("tungsten_bin_{}.exe", unique_id));

    compile_llvm_aot(module, &exe_path, "O3")?;

    let mut output_res = None;
    for attempt in 0..15 {
        match Command::new(&exe_path).output() {
            Ok(out) => {
                output_res = Some(out);
                break;
            }
            Err(e) if e.raw_os_error() == Some(5) => {
                std::thread::sleep(std::time::Duration::from_millis(50 * (attempt + 1)));
            }
            Err(e) => {
                let _ = fs::remove_file(&exe_path);
                return Err(format!("Failed to execute compiled binary: {}", e));
            }
        }
    }

    let output = match output_res {
        Some(out) => out,
        None => {
            let _ = fs::remove_file(&exe_path);
            return Err("Failed to execute compiled binary after retries (os error 5)".to_string());
        }
    };

    let _ = fs::remove_file(&exe_path);

    let mut stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    if !stderr.is_empty() {
        stdout.push_str(&stderr);
    }
    let code = output.status.code().unwrap_or(0);

    Ok((code, stdout))
}
