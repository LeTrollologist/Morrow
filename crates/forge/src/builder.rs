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
        if p.is_file() {
            if let Some(m) = find_manifest(p) {
                return m.parent().unwrap().to_path_buf();
            } else if let Some(parent) = p.parent() {
                if !parent.as_os_str().is_empty() {
                    return parent.to_path_buf();
                }
            }
        } else if p.is_dir() {
            if let Some(m) = find_manifest(p) {
                return m.parent().unwrap().to_path_buf();
            }
            return p.to_path_buf();
        }
    }
    if let Ok(curr) = env::current_dir() {
        if let Some(m) = find_manifest(&curr) {
            return m.parent().unwrap().to_path_buf();
        }
        return curr;
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

    if let Err(errs) = tungsten_typeck::check(&ast) {
        let mut msg = format!("\n[Type & Effect Error] {} error(s) found in {}:\n", errs.len(), path_desc);
        for (idx, err) in errs.iter().enumerate() {
            msg.push_str(&format!("  {}. [Line {}, Col {}]: {}\n", idx + 1, err.span.line, err.span.column, err.message));
        }
        return Err(msg);
    }

    let (src_file, src_dir) = resolve_source_info(&path_desc);
    let mut module = tungsten_tir::compile_with_source(&ast, src_file, src_dir)
        .map_err(|e| format!("[TIR Lowering Error]: {}", e))?;
    let stats = tungsten_tir::optimize(&mut module);

    let bin_name = Path::new(&path_desc)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("app");

    let root_dir = get_target_dir_root(target);
    let profile_dir = if options.release { "release" } else { "debug" };
    let out_dir = root_dir.join("target").join(profile_dir);
    let _ = fs::create_dir_all(&out_dir);

    let out_exe = options.custom_out.clone().unwrap_or_else(|| out_dir.join(format!("{}.exe", bin_name)));

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

    let aot_opts = tungsten_codegen::AotOptions {
        opt_level: opt_level.to_string(),
        emit_llvm: llvm_path,
        emit_asm: asm_path,
        extra_libs: Vec::new(),
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