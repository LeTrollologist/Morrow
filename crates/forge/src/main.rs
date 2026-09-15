use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process;

mod bindgen;
mod builder;
mod lockfile;
mod manifest;
mod package;
mod resolver;

use builder::{
    build_target, clean_target, get_latest_source_mtime, get_target_dir_root, load_program_auto,
    resolve_source_info, BuildOptions,
};
use manifest::{DependencySpec, DetailedDependency, Manifest};
use package::{find_manifest, ProjectPackage};
use resolver::DependencyResolver;

fn main() {
    let args: Vec<String> = env::args().collect();

    if args.len() < 2 {
        print_usage();
        return;
    }

    match args[1].as_str() {
        "check" => {
            run_check(&args[2..]);
        }
        "run" => {
            run_file(&args[2..]);
        }
        "build" => {
            run_build(&args[2..]);
        }
        "clean" => {
            run_clean(&args[2..]);
        }
        "fmt" => {
            if args.len() < 3 {
                eprintln!("Error: 'forge fmt' requires a path to a .tg file");
                eprintln!("Usage: forge fmt [--check] <file.tg>");
                process::exit(1);
            }
            run_fmt(&args[2..]);
        }
        "tir" | "ir" => {
            if args.len() < 3 {
                eprintln!("Error: 'forge tir' requires a path to a .tg file");
                eprintln!("Usage: forge tir [--opt] <file.tg>");
                process::exit(1);
            }
            run_tir(&args[2..]);
        }
        "lsp" => {
            run_lsp();
        }
        "new" => {
            if args.len() < 3 {
                eprintln!("Error: 'forge new' requires a project name");
                eprintln!("Usage: forge new <project_name> [--lib]");
                process::exit(1);
            }
            create_new_project(&args[2..]);
        }
        "add" => {
            if args.len() < 3 {
                eprintln!("Error: 'forge add' requires a dependency name");
                eprintln!("Usage: forge add <dependency> [--path <path>] [--version <ver>]");
                process::exit(1);
            }
            run_add(&args[2..]);
        }
        "lock" => {
            run_lock();
        }
        "bindgen" => {
            if args.len() < 3 {
                eprintln!("Error: 'forge bindgen' requires a path to a C header file (.h)");
                eprintln!("Usage: forge bindgen <header.h> [--out <output.tg>]");
                process::exit(1);
            }
            bindgen::run_bindgen(&args[2..]);
        }
        "version" | "--version" | "-V" => {
            println!("forge 0.2.0 (tungsten-lang 2026)");
        }
        "help" | "--help" | "-h" => {
            print_usage();
        }
        unknown => {
            eprintln!("Unknown command: '{}'", unknown);
            print_usage();
            process::exit(1);
        }
    }
}

fn print_usage() {
    println!(
        r#"forge - The Tungsten Build Tool & Package Manager

USAGE:
    forge <SUBCOMMAND> [OPTIONS]

SUBCOMMANDS:
    check [file.tg]                       Typecheck project or file, verify refinement bounds & effects
    run [--release] [file.tg]            Incrementally compile and execute project or file (LLVM AOT)
    build [--release] [--emit-llvm] [--emit-asm] [-o <path>] [file.tg]
                                          Compile native binary (target/debug/ -O0 or target/release/ -O3)
    clean [path]                          Remove target/ directory and build artifacts
    new <project_name> [--lib]            Create a new Tungsten binary or library package
    add <dep> [--path P] [--ver V]        Add a dependency to Forge.toml and update Forge.lock
    lock                                  Resolve dependencies and update Forge.lock
    fmt [--check] <file.tg>               Format Tungsten source code according to canonical style
    tir [--opt] <file.tg>                 Compile and print Tungsten Intermediate Representation (TIR)
    bindgen <header.h> [--out file.tg]    Generate safe Tungsten C-ABI bindings from C header
    lsp                                   Start the Tungsten Language Server (JSON-RPC 2.0 over stdio)
    version                               Display version information
    help                                  Display this help message
"#
    );
}

fn run_check(args: &[String]) {
    let target = args.first().map(|s| s.as_str());
    println!("Checking package/file...");
    let (ast, path_desc) = match load_program_auto(target) {
        Ok(res) => res,
        Err(err) => {
            eprintln!("{}", err);
            process::exit(1);
        }
    };

    match tungsten_typeck::check(&ast) {
        Ok(_) => {
            println!("Finished checking '{}': 0 errors. Refinements and algebraic effects verified.", path_desc);
        }
        Err(errs) => {
            eprintln!("\n[Type & Effect Error] {} error(s) found in {}:", errs.len(), path_desc);
            for (idx, err) in errs.iter().enumerate() {
                eprintln!("  {}. [Line {}, Col {}]: {}", idx + 1, err.span.line, err.span.column, err.message);
            }
            process::exit(1);
        }
    }
}

fn run_clean(args: &[String]) {
    let target = args.first().map(|s| s.as_str());
    match clean_target(target) {
        Ok(dir) => {
            println!("Cleaned target directory: {}", dir.display());
        }
        Err(e) => {
            eprintln!("Error cleaning target: {}", e);
            process::exit(1);
        }
    }
}

fn run_build(args: &[String]) {
    let mut release = false;
    let mut emit_llvm = false;
    let mut emit_asm = false;
    let mut custom_out = None;
    let mut target = None;

    let mut i = 0;
    while i < args.len() {
        let arg = &args[i];
        if arg == "--release" || arg == "-r" {
            release = true;
        } else if arg == "--emit-llvm" {
            emit_llvm = true;
        } else if arg == "--emit-asm" {
            emit_asm = true;
        } else if (arg == "--out" || arg == "-o") && i + 1 < args.len() {
            custom_out = Some(PathBuf::from(&args[i + 1]));
            i += 1;
        } else if !arg.starts_with('-') && target.is_none() {
            target = Some(arg.as_str());
        }
        i += 1;
    }

    let options = BuildOptions {
        release,
        emit_llvm,
        emit_asm,
        custom_out,
    };

    match build_target(target, &options) {
        Ok(out_exe) => {
            let profile = if release {
                "release [optimized]"
            } else {
                "debug [unoptimized + debuginfo]"
            };
            println!("Finished {} target(s) -> {}", profile, out_exe.display());
        }
        Err(e) => {
            eprintln!("{}", e);
            process::exit(1);
        }
    }
}

fn run_file(args: &[String]) {
    let mut release = false;
    let mut vm = false;
    let mut target = None;

    for arg in args {
        if arg == "--release" || arg == "-r" {
            release = true;
        } else if arg == "--vm" {
            vm = true;
        } else if !arg.starts_with('-') && target.is_none() {
            target = Some(arg.as_str());
        }
    }

    if vm {
        let (ast, _path_desc) = match load_program_auto(target) {
            Ok(res) => res,
            Err(err) => {
                eprintln!("{}", err);
                process::exit(1);
            }
        };
        match tungsten_vm::execute(&ast) {
            Ok(_) => {}
            Err(runtime_err) => {
                eprintln!("\n[Runtime Error]: {}", runtime_err);
                process::exit(1);
            }
        }
        return;
    }

    let root_dir = get_target_dir_root(target);
    let bin_name = if let Some(t) = target {
        let p = Path::new(t);
        if p.is_file() {
            p.file_stem().and_then(|s| s.to_str()).unwrap_or("app").to_string()
        } else if let Ok(pkg) = ProjectPackage::discover(p) {
            pkg.root_dir.file_name().and_then(|s| s.to_str()).unwrap_or("app").to_string()
        } else {
            "app".to_string()
        }
    } else if let Ok(pkg) = ProjectPackage::discover(&env::current_dir().unwrap_or_default()) {
        pkg.root_dir.file_name().and_then(|s| s.to_str()).unwrap_or("app").to_string()
    } else {
        "app".to_string()
    };

    let profile_dir = if release { "release" } else { "debug" };
    let exe_path = root_dir.join("target").join(profile_dir).join(format!("{}.exe", bin_name));

    let is_up_to_date = if exe_path.is_file() {
        if let Ok(exe_meta) = fs::metadata(&exe_path) {
            if let Ok(exe_mtime) = exe_meta.modified() {
                if let Some(src_mtime) = get_latest_source_mtime(target) {
                    exe_mtime >= src_mtime
                } else {
                    false
                }
            } else {
                false
            }
        } else {
            false
        }
    } else {
        false
    };

    let target_bin = if is_up_to_date {
        // Incremental skip: already up-to-date
        exe_path
    } else {
        let options = BuildOptions {
            release,
            emit_llvm: false,
            emit_asm: false,
            custom_out: None,
        };
        match build_target(target, &options) {
            Ok(p) => p,
            Err(e) => {
                eprintln!("{}", e);
                process::exit(1);
            }
        }
    };

    // Execute compiled binary with Windows Error 5 retry loop (exponential backoff)
    let mut output_res = None;
    for attempt in 0..15 {
        match std::process::Command::new(&target_bin).status() {
            Ok(status) => {
                output_res = Some(status);
                break;
            }
            Err(e) if e.raw_os_error() == Some(5) => {
                std::thread::sleep(std::time::Duration::from_millis(20 * (1 << attempt.min(5))));
            }
            Err(e) => {
                eprintln!("Failed to execute binary '{}': {}", target_bin.display(), e);
                process::exit(1);
            }
        }
    }

    let status = match output_res {
        Some(s) => s,
        None => {
            eprintln!(
                "Failed to execute binary '{}' after retries (Error 5 Access Denied)",
                target_bin.display()
            );
            process::exit(1);
        }
    };

    if !status.success() {
        process::exit(status.code().unwrap_or(1));
    }
}

fn create_new_project(args: &[String]) {
    let mut name = None;
    let mut is_lib = false;

    for arg in args {
        if arg == "--lib" {
            is_lib = true;
        } else if name.is_none() {
            name = Some(arg.clone());
        }
    }

    let project_name = match name {
        Some(n) => n,
        None => {
            eprintln!("Error: project name is required");
            process::exit(1);
        }
    };

    let project_path = Path::new(&project_name);
    if project_path.exists() {
        eprintln!("Error: Directory '{}' already exists", project_name);
        process::exit(1);
    }

    if let Err(e) = fs::create_dir_all(project_path.join("src")) {
        eprintln!("Failed to create project directory: {}", e);
        process::exit(1);
    }

    let forge_toml = format!(
        r#"[package]
name = "{}"
version = "0.1.0"
edition = "2026"

[dependencies]
"#,
        project_name
    );

    let gitignore = r#"target/
Forge.lock
"#;

    let _ = fs::write(project_path.join("Forge.toml"), forge_toml);
    let _ = fs::write(project_path.join(".gitignore"), gitignore);

    if is_lib {
        let lib_tg = format!(
            r#"// Tungsten Library: {}

pub fn add(a: i64, b: i64) -> i64 {{
    a + b
}}
"#,
            project_name
        );
        let _ = fs::write(project_path.join("src").join("lib.tg"), lib_tg);
        println!("Created library package `{}` (src/lib.tg)", project_name);
    } else {
        let main_tg = r#"// Tungsten Binary Entry Point
type Percentage = u8(0..=100);

fn main() {
    let health: Percentage = 100 as Percentage;
    println!("Welcome to Tungsten! Initial health: {}%", health);
}
"#;
        let _ = fs::write(project_path.join("src").join("main.tg"), main_tg);
        println!("Created binary package `{}` (src/main.tg)", project_name);
    }
}

fn run_add(args: &[String]) {
    let dep_name = &args[0];
    let mut path_opt = None;
    let mut version_opt = None;

    let mut i = 1;
    while i < args.len() {
        if args[i] == "--path" && i + 1 < args.len() {
            path_opt = Some(args[i + 1].clone());
            i += 2;
        } else if args[i] == "--version" && i + 1 < args.len() {
            version_opt = Some(args[i + 1].clone());
            i += 2;
        } else {
            i += 1;
        }
    }

    let current_dir = env::current_dir().unwrap();
    let manifest_path = match find_manifest(&current_dir) {
        Some(p) => p,
        None => {
            eprintln!("Error: Could not find Forge.toml in current directory or any parent directory.");
            process::exit(1);
        }
    };

    let mut manifest = match Manifest::from_file(&manifest_path) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("Error reading Forge.toml: {}", e);
            process::exit(1);
        }
    };

    let spec = if path_opt.is_some() || version_opt.is_some() {
        DependencySpec::Detailed(DetailedDependency {
            version: version_opt,
            path: path_opt,
            git: None,
            branch: None,
        })
    } else {
        DependencySpec::Version("0.1.0".to_string())
    };

    manifest.add_dependency(dep_name.clone(), spec);
    if let Err(e) = manifest.write_file(&manifest_path) {
        eprintln!("Error writing Forge.toml: {}", e);
        process::exit(1);
    }
    println!("Added dependency `{}` to Forge.toml", dep_name);

    // Automatically resolve and update Forge.lock
    let resolver = DependencyResolver::new();
    match resolver.resolve(&manifest_path) {
        Ok(graph) => {
            let lockfile = graph.generate_lockfile();
            let lockfile_path = manifest_path.parent().unwrap().join("Forge.lock");
            if let Err(e) = lockfile.write_file(&lockfile_path) {
                eprintln!("Warning: Failed to write Forge.lock: {}", e);
            } else {
                println!("Updated Forge.lock");
            }
        }
        Err(e) => {
            eprintln!("Resolution warning: {}", e);
        }
    }
}

fn run_lock() {
    let current_dir = env::current_dir().unwrap();
    let manifest_path = match find_manifest(&current_dir) {
        Some(p) => p,
        None => {
            eprintln!("Error: Could not find Forge.toml in current directory or any parent directory.");
            process::exit(1);
        }
    };

    let resolver = DependencyResolver::new();
    match resolver.resolve(&manifest_path) {
        Ok(graph) => {
            let lockfile = graph.generate_lockfile();
            let lockfile_path = manifest_path.parent().unwrap().join("Forge.lock");
            if let Err(e) = lockfile.write_file(&lockfile_path) {
                eprintln!("Error writing Forge.lock: {}", e);
                process::exit(1);
            }
            println!("Successfully generated Forge.lock ({} package(s) locked with SHA-256 checksums).", lockfile.packages.len());
        }
        Err(e) => {
            eprintln!("Resolution error: {}", e);
            process::exit(1);
        }
    }
}

fn run_tir(args: &[String]) {
    let mut optimize = false;
    let mut target = None;

    for arg in args {
        if arg == "--opt" || arg == "-O" {
            optimize = true;
        } else if target.is_none() {
            target = Some(arg.as_str());
        }
    }

    let (ast, path_desc) = match load_program_auto(target) {
        Ok(res) => res,
        Err(err) => {
            eprintln!("{}", err);
            process::exit(1);
        }
    };

    if let Err(errs) = tungsten_typeck::check(&ast) {
        eprintln!("\n[Type & Effect Error] {} error(s) found in {}:", errs.len(), path_desc);
        for (idx, err) in errs.iter().enumerate() {
            eprintln!("  {}. [Line {}, Col {}]: {}", idx + 1, err.span.line, err.span.column, err.message);
        }
        process::exit(1);
    }

    let (src_file, src_dir) = resolve_source_info(&path_desc);
    let mut module = match tungsten_tir::compile_with_source(&ast, src_file, src_dir) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("\n[TIR Lowering Error]: {}", e);
            process::exit(1);
        }
    };

    if optimize {
        let stats = tungsten_tir::optimize(&mut module);
        println!(";; Optimization Passes Applied for {}:", path_desc);
        println!(";;   - Constant folds: {}", stats.const_folds);
        println!(";;   - Redundant bounds checks eliminated: {}", stats.bounds_checks_eliminated);
        println!(";;   - Dead instructions/blocks pruned: {}", stats.dead_code_pruned);
        println!(";;   - Iterations to fixpoint: {}", stats.iterations);
        println!();
    }

    print!("{}", tungsten_tir::print(&module));
}

fn run_fmt(args: &[String]) {
    let mut check_only = false;
    let mut files = Vec::new();

    for arg in args {
        if arg == "--check" {
            check_only = true;
        } else {
            files.push(arg.clone());
        }
    }

    if files.is_empty() {
        eprintln!("Error: 'forge fmt' requires at least one .tg file");
        process::exit(1);
    }

    let mut unformatted_found = false;
    for file in &files {
        let content = match fs::read_to_string(file) {
            Ok(c) => c,
            Err(e) => {
                eprintln!("Error reading '{}': {}", file, e);
                process::exit(1);
            }
        };

        match tungsten_syntax::format_source(&content) {
            Ok(formatted) => {
                if formatted != content {
                    if check_only {
                        eprintln!("File '{}' is not formatted.", file);
                        unformatted_found = true;
                    } else {
                        if let Err(e) = fs::write(file, &formatted) {
                            eprintln!("Failed to write formatted file '{}': {}", file, e);
                            process::exit(1);
                        }
                        println!("Formatted {}", file);
                    }
                } else if !check_only {
                    println!("Already formatted {}", file);
                }
            }
            Err(e) => {
                eprintln!("Syntax error in '{}': {}", file, e);
                process::exit(1);
            }
        }
    }

    if check_only && unformatted_found {
        eprintln!("\n1 or more files require formatting. Run 'forge fmt <file>' to fix.");
        process::exit(1);
    } else if check_only {
        println!("All checked files are properly formatted.");
    }
}

fn run_lsp() {
    let mut server = tungsten_lsp::LspServer::new();
    if let Err(e) = server.run_stdio() {
        eprintln!("LSP server error: {}", e);
        process::exit(1);
    }
}