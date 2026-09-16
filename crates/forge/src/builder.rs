use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use crate::lockfile::Lockfile;
use crate::package::{compile_package_ast, find_manifest, ProjectPackage};
use crate::resolver::DependencyResolver;

#[derive(Debug, Clone)]
pub struct BuildOptions {
    pub release: bool,
    pub emit_llvm: bool,
    pub emit_asm: bool,
    pub custom_out: Option<PathBuf>,
}

impl Default for BuildOptions {
    fn default() -> Self {
        Self {
            release: false,
            emit_llvm: false,
            emit_asm: false,
            custom_out: None,
        }
    }
}

pub fn get_target_dir_root(target: Option<&str>) -> PathBuf {
    if let Some(target_str) = target {
        let p = Path::new(target_str);
        // 1. If there is a Forge.toml somewhere above the target file, that manifest's parent IS the root.
        if let Some(m) = find_manifest(p) {
            return m.parent().unwrap().to_path_buf();
        }

        // 2. Walk UP from the target file path looking for a workspace-root indicator
        //    (both target/crt and std/ directories present). This detects when target is in
        //    a workspace (e.g., examples/sqlite_benchmark.tg inside the Tungsten workspace).
        let start = if p.is_file() {
            p.parent().map(|d| d.to_path_buf())
        } else if p.is_dir() {
            Some(p.to_path_buf())
        } else {
            None
        };
        if let Some(start_dir) = start {
            let mut dir: Option<PathBuf> = Some(start_dir);
            while let Some(d) = dir {
                if d.join("target").join("crt").is_dir() && d.join("std").is_dir() {
                    return d;
                }
                dir = d.parent().map(|p| p.to_path_buf());
            }
        }

        // 3. Target was specified, but is not inside a workspace. Use target's parent directory.
        if p.is_file() {
            if let Some(parent) = p.parent() {
                if !parent.as_os_str().is_empty() {
                    return parent.to_path_buf();
                }
            }
        } else if p.is_dir() {
            return p.to_path_buf();
        }
    }

    // 4. If no target was specified, search from current_dir.
    if let Ok(curr) = env::current_dir() {
        if let Some(m) = find_manifest(&curr) {
            return m.parent().unwrap().to_path_buf();
        }
        let mut dir: Option<PathBuf> = Some(curr);
        while let Some(d) = dir {
            if d.join("target").join("crt").is_dir() && d.join("std").is_dir() {
                return d;
            }
            dir = d.parent().map(|p| p.to_path_buf());
        }
    }

    PathBuf::from(".")
}

pub fn load_program_auto(target: Option<&str>) -> Result<(tungsten_syntax::ast::Program, String), String> {
    let current_dir = env::current_dir().map_err(|e| format!("Could not get current dir: {}", e))?;

    if let Some(path_str) = target {
        let p = Path::new(path_str);
        if p.is_file() {
            if let Some(manifest_path) = find_manifest(p) {
                let resolver = DependencyResolver::new();
                let graph = resolver.resolve(&manifest_path).ok();
                let ast = compile_package_ast(p, graph.as_ref())?;
                return Ok((ast, path_str.to_string()));
            } else {
                let ast = compile_package_ast(p, None)?;
                return Ok((ast, path_str.to_string()));
            }
        } else if p.is_dir() {
            let pkg = ProjectPackage::discover(p)?;
            let resolver = DependencyResolver::new();
            let graph = resolver.resolve(&pkg.manifest_path)?;
            let lock_path = pkg.manifest_path.parent().unwrap().join("Forge.lock");
            if lock_path.is_file() {
                if let Ok(lock) = Lockfile::from_file(&lock_path) {
                    if let Err(e) = graph.verify_lockfile(&lock) {
                        eprintln!("[Forge Warning] {}", e);
                    }
                }
            }
            let ast = compile_package_ast(&pkg.entry_file, Some(&graph))?;
            return Ok((ast, pkg.entry_file.display().to_string()));
        }
    }

    let pkg = ProjectPackage::discover(&current_dir)?;
    let resolver = DependencyResolver::new();
    let graph = resolver.resolve(&pkg.manifest_path)?;
    let lock_path = pkg.manifest_path.parent().unwrap().join("Forge.lock");
    if lock_path.is_file() {
        if let Ok(lock) = Lockfile::from_file(&lock_path) {
            if let Err(e) = graph.verify_lockfile(&lock) {
                eprintln!("[Forge Warning] {}", e);
            }
        }
    }
    let ast = compile_package_ast(&pkg.entry_file, Some(&graph))?;
    Ok((ast, pkg.entry_file.display().to_string()))
}

pub fn resolve_source_info(path_desc: &str) -> (Option<String>, Option<String>) {
    let p = Path::new(path_desc);
    let abs = if p.is_absolute() {
        p.to_path_buf()
    } else {
        env::current_dir().unwrap_or_default().join(p)
    };
    (
        abs.file_name().map(|f| f.to_string_lossy().to_string()),
        abs.parent().map(|d| d.to_string_lossy().to_string()),
    )
}

pub fn build_target(
    target: Option<&str>,
    options: &BuildOptions,
) -> Result<PathBuf, String> {
    let (ast, path_desc) = load_program_auto(target)?;

    eprintln!("[Forge] Type checking AST ({} items)...", ast.items.len());
    if let Err(errs) = tungsten_typeck::check(&ast) {
        let mut msg = format!("\n[Type & Effect Error] {} error(s) found in {}:\n", errs.len(), path_desc);
        for (idx, err) in errs.iter().enumerate() {
            msg.push_str(&format!("  {}. [Line {}, Col {}]: {}\n", idx + 1, err.span.line, err.span.column, err.message));
        }
        return Err(msg);
    }
    eprintln!("[Forge] Type check passed. Lowering to TIR...");

    let (src_file, src_dir) = resolve_source_info(&path_desc);
    let mut module = tungsten_tir::compile_with_source(&ast, src_file, src_dir)
        .map_err(|e| format!("[TIR Lowering Error]: {}", e))?;
    eprintln!("[Forge] TIR lowered ({} functions). Running optimization passes...", module.functions.len());
    let stats = tungsten_tir::optimize(&mut module);
    eprintln!("[Forge] TIR optimization complete ({} const folds, {} bounds elim).", stats.const_folds, stats.bounds_checks_eliminated);

    let bin_name = Path::new(&path_desc)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("app");

    let root_dir = get_target_dir_root(target);
    let crt_dst = root_dir.join("target").join("crt");
    if !crt_dst.join("crt2.o").is_file() {
        if let Ok(toolchain) = tungsten_codegen::LlvmToolchain::discover() {
            let _ = fs::create_dir_all(&crt_dst);
            if let Ok(entries) = fs::read_dir(&toolchain.crt_dir) {
                for entry in entries.flatten() {
                    let dest = crt_dst.join(entry.file_name());
                    let _ = fs::copy(entry.path(), dest);
                }
            }
        }
    }

    let workspace_root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .map(|p| p.to_path_buf());

    let sqlite_a = crt_dst.join("libsqlite3.a");
    if !sqlite_a.is_file() {
        // 1. Try to copy prebuilt libsqlite3.a from workspace
        let ws_sqlite_a = workspace_root.as_ref().map(|ws| ws.join("target").join("crt").join("libsqlite3.a"));
        if let Some(src) = ws_sqlite_a.filter(|p| p.is_file()) {
            let _ = fs::copy(&src, &sqlite_a);
        } else if Path::new("target/crt/libsqlite3.a").is_file() {
            let _ = fs::copy("target/crt/libsqlite3.a", &sqlite_a);
        } else {
            // 2. Try to generate via llvm-dlltool from sqlite3.def
            let sqlite_def = root_dir.join("target").join("sqlite").join("sqlite3.def");
            let ws_def = workspace_root.as_ref().map(|ws| ws.join("target").join("sqlite").join("sqlite3.def"));
            let def_path = if sqlite_def.is_file() {
                Some(sqlite_def)
            } else if let Some(p) = ws_def.filter(|p| p.is_file()) {
                Some(p)
            } else if Path::new("target/sqlite/sqlite3.def").is_file() {
                Some(PathBuf::from("target/sqlite/sqlite3.def"))
            } else {
                None
            };

            if let Some(def_p) = def_path {
                if let Ok(toolchain) = tungsten_codegen::LlvmToolchain::discover() {
                    if let Some(llvm_bin) = toolchain.clang_path.parent() {
                        let dlltool = llvm_bin.join("llvm-dlltool.exe");
                        if dlltool.is_file() {
                            let _ = std::process::Command::new(dlltool)
                                .arg("-m").arg("i386:x86-64")
                                .arg("-d").arg(&def_p)
                                .arg("-l").arg(&sqlite_a)
                                .arg("-D").arg("sqlite3.dll")
                                .output();
                        }
                    }
                }
            }
        }
    }

    let profile_dir = if options.release { "release" } else { "debug" };
    let out_dir = root_dir.join("target").join(profile_dir);
    let _ = fs::create_dir_all(&out_dir);

    let out_exe = options.custom_out.clone().unwrap_or_else(|| out_dir.join(format!("{}.exe", bin_name)));

    let sqlite_dll_src = if root_dir.join("target").join("sqlite").join("sqlite3.dll").is_file() {
        Some(root_dir.join("target").join("sqlite").join("sqlite3.dll"))
    } else if let Some(ws_dll) = workspace_root.as_ref().map(|ws| ws.join("target").join("sqlite").join("sqlite3.dll")).filter(|p| p.is_file()) {
        Some(ws_dll)
    } else if Path::new("target/sqlite/sqlite3.dll").is_file() {
        Some(PathBuf::from("target/sqlite/sqlite3.dll"))
    } else {
        None
    };

    if let Some(dll_src) = sqlite_dll_src {
        let _ = fs::copy(&dll_src, out_dir.join("sqlite3.dll"));
        if let Some(parent) = out_exe.parent() {
            let _ = fs::copy(&dll_src, parent.join("sqlite3.dll"));
        }
    }

    let llvm_path = if options.emit_llvm {
        Some(out_exe.with_extension("ll"))
    } else {
        None
    };

    let asm_path = if options.emit_asm {
        Some(out_exe.with_extension("s"))
    } else {
        None
    };

    let opt_level = if options.release { "O3" } else { "O0" };

    let mut extra_libs = Vec::new();
    let mut extra_lib_paths = Vec::new();

    let root_target_crt = root_dir.join("target").join("crt");
    let root_target_sqlite = root_dir.join("target").join("sqlite");

    if sqlite_a.is_file() || root_target_crt.join("libsqlite3.a").is_file() || Path::new("target/crt/libsqlite3.a").is_file() {
        extra_libs.push("sqlite3".to_string());
    }

    // Always supply absolute paths derived from the resolved workspace root,
    // so that `cargo test` (which runs from a different CWD) can still find libsqlite3.a.
    if crt_dst.is_dir() {
        extra_lib_paths.push(crt_dst);
    }
    if root_target_crt.is_dir() {
        extra_lib_paths.push(root_target_crt);
    }
    if root_target_sqlite.is_dir() {
        extra_lib_paths.push(root_target_sqlite);
    }
    if let Some(ref ws) = workspace_root {
        let ws_crt = ws.join("target").join("crt");
        if ws_crt.is_dir() {
            extra_lib_paths.push(ws_crt);
        }
        let ws_sqlite = ws.join("target").join("sqlite");
        if ws_sqlite.is_dir() {
            extra_lib_paths.push(ws_sqlite);
        }
    }
    extra_lib_paths.push(root_dir.clone());

    let aot_opts = tungsten_codegen::AotOptions {
        opt_level: opt_level.to_string(),
        emit_llvm: llvm_path,
        emit_asm: asm_path,
        extra_libs,
        extra_lib_paths,
    };

    if options.release {
        println!("Compiling {} with LLVM backend (--release, -O3, LTO)...", path_desc);
        println!("  - TIR Optimized: {} constant folds, {} bounds checks eliminated", stats.const_folds, stats.bounds_checks_eliminated);
    } else {
        println!("Compiling {} with LLVM backend (debug, -O0, debug symbols)...", path_desc);
        println!("  - TIR Optimized: {} constant folds, {} bounds checks eliminated", stats.const_folds, stats.bounds_checks_eliminated);
    }

    tungsten_codegen::compile_to_native_binary_with_options(&module, &out_exe, &aot_opts)?;
    Ok(out_exe)
}

pub fn clean_target(target: Option<&str>) -> Result<PathBuf, String> {
    let root = get_target_dir_root(target);
    let target_dir = root.join("target");
    if target_dir.exists() {
        for attempt in 0..10 {
            match fs::remove_dir_all(&target_dir) {
                Ok(_) => return Ok(target_dir),
                Err(e) if e.raw_os_error() == Some(5) => {
                    std::thread::sleep(std::time::Duration::from_millis(20 * (1 << attempt.min(5))));
                }
                Err(e) => return Err(format!("Failed to remove target directory: {}", e)),
            }
        }
        Err(format!("Failed to remove target directory '{}' (locked by another process)", target_dir.display()))
    } else {
        Ok(target_dir)
    }
}

pub fn get_latest_source_mtime(target: Option<&str>) -> Option<SystemTime> {
    let mut latest: Option<SystemTime> = None;

    let mut update = |p: &Path| {
        if let Ok(meta) = fs::metadata(p) {
            if let Ok(mtime) = meta.modified() {
                if let Some(curr) = latest {
                    if mtime > curr {
                        latest = Some(mtime);
                    }
                } else {
                    latest = Some(mtime);
                }
            }
        }
    };

    if let Some(target_str) = target {
        let p = Path::new(target_str);
        if p.is_file() {
            update(p);
            if let Some(manifest) = find_manifest(p) {
                update(&manifest);
            }
            return latest;
        } else if p.is_dir() {
            if let Ok(pkg) = ProjectPackage::discover(p) {
                update(&pkg.manifest_path);
                update(&pkg.entry_file);
                return latest;
            }
        }
    }

    if let Ok(curr) = env::current_dir() {
        if let Ok(pkg) = ProjectPackage::discover(&curr) {
            update(&pkg.manifest_path);
            update(&pkg.entry_file);
        }
    }

    latest
}