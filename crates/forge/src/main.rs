use std::env;
use std::fs;
use std::path::Path;
use std::process;

fn main() {
    let args: Vec<String> = env::args().collect();

    if args.len() < 2 {
        print_usage();
        return;
    }

    match args[1].as_str() {
        "check" => {
            if args.len() < 3 {
                eprintln!("Error: 'forge check' requires a path to a .tg file");
                eprintln!("Usage: forge check <file.tg>");
                process::exit(1);
            }
            run_check(&args[2]);
        }
        "run" => {
            if args.len() < 3 {
                eprintln!("Error: 'forge run' requires a path to a .tg file");
                eprintln!("Usage: forge run [--native] <file.tg>");
                process::exit(1);
            }
            run_file(&args[2..]);
        }
        "build" => {
            if args.len() < 3 {
                eprintln!("Error: 'forge build' requires a path to a .tg file");
                eprintln!("Usage: forge build <file.tg>");
                process::exit(1);
            }
            run_build(&args[2..]);
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
                eprintln!("Usage: forge new <project_name>");
                process::exit(1);
            }
            create_new_project(&args[2]);
        }
        "version" | "--version" | "-V" => {
            println!("forge 0.1.0 (tungsten-lang 2026)");
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
        r#"forge - The Tungsten Build Tool & Runtime

USAGE:
    forge <SUBCOMMAND> [OPTIONS]

SUBCOMMANDS:
    check <file.tg>           Typecheck, verify refinement bounds, and check effect rows
    run [--native] <file.tg>  Compile, check, and execute (tree-walking VM or native Cranelift JIT)
    build <file.tg>           Compile a Tungsten program to native machine code
    fmt [--check] <file>      Format Tungsten source code according to canonical style
    tir [--opt] <file.tg>     Compile and print Tungsten Intermediate Representation (TIR)
    lsp                       Start the Tungsten Language Server (JSON-RPC 2.0 over stdio)
    new <project_name>        Create a new Tungsten project
    version                   Display version information
    help                      Display this help message
"#
    );
}

fn load_program(filepath: &str) -> Result<tungsten_syntax::ast::Program, String> {
    let source = match fs::read_to_string(filepath) {
        Ok(s) => s,
        Err(e) => return Err(format!("Error reading file '{}': {}", filepath, e)),
    };

    let mut ast = match tungsten_syntax::parse(&source) {
        Ok(prog) => prog,
        Err(err) => return Err(format!("[Syntax Error] in {}:\n  {}", filepath, err)),
    };

    let path = Path::new(filepath);
    let is_stdlib = path.starts_with("std") || path.starts_with("./std") || path.starts_with(".\\std");
    if !is_stdlib && Path::new("std").is_dir() {
        let user_names: std::collections::HashSet<String> = ast.items.iter().filter_map(|it| match it {
            tungsten_syntax::ast::Item::TypeAlias(a) => Some(a.name.clone()),
            tungsten_syntax::ast::Item::Struct(s) => Some(s.name.clone()),
            tungsten_syntax::ast::Item::Fn(f) => Some(f.name.clone()),
            tungsten_syntax::ast::Item::Effect(e) => Some(e.name.clone()),
        }).collect();

        let std_files = [
            "std/prelude.tg",
            "std/refinements.tg",
            "std/effects.tg",
            "std/collections.tg",
            "std/sync.tg",
            "std/net.tg",
            "std/slice.tg",
        ];
        let mut std_items = Vec::new();
        for sf in &std_files {
            if let Ok(content) = fs::read_to_string(sf) {
                if let Ok(std_ast) = tungsten_syntax::parse(&content) {
                    for item in std_ast.items {
                        let name = match &item {
                            tungsten_syntax::ast::Item::TypeAlias(a) => &a.name,
                            tungsten_syntax::ast::Item::Struct(s) => &s.name,
                            tungsten_syntax::ast::Item::Fn(f) => &f.name,
                            tungsten_syntax::ast::Item::Effect(e) => &e.name,
                        };
                        if !user_names.contains(name) {
                            std_items.push(item);
                        }
                    }
                }
            }
        }
        std_items.append(&mut ast.items);
        ast.items = std_items;
    }

    Ok(ast)
}

fn run_check(filepath: &str) {
    println!("Checking {} ...", filepath);
    let ast = match load_program(filepath) {
        Ok(prog) => prog,
        Err(err) => {
            eprintln!("{}", err);
            process::exit(1);
        }
    };

    // 2. Type Checking & Refinement Solver
    match tungsten_typeck::check(&ast) {
        Ok(_) => {
            println!("Finished checking: 0 errors. Refinements and algebraic effects verified.");
        }
        Err(errs) => {
            eprintln!("\n[Type & Effect Error] {} error(s) found in {}:", errs.len(), filepath);
            for (idx, err) in errs.iter().enumerate() {
                eprintln!("  {}. [Line {}, Col {}]: {}", idx + 1, err.span.line, err.span.column, err.message);
            }
            process::exit(1);
        }
    }
}

fn run_file(args: &[String]) {
    let mut native = false;
    let mut file = None;

    for arg in args {
        if arg == "--native" || arg == "-n" {
            native = true;
        } else if file.is_none() {
            file = Some(arg.clone());
        }
    }

    let filepath = match file {
        Some(f) => f,
        None => {
            eprintln!("Error: 'forge run' requires a path to a .tg file");
            process::exit(1);
        }
    };

    let ast = match load_program(&filepath) {
        Ok(prog) => prog,
        Err(err) => {
            eprintln!("{}", err);
            process::exit(1);
        }
    };

    // 2. Type Checking & Effect Verification
    if let Err(errs) = tungsten_typeck::check(&ast) {
        eprintln!("\n[Type & Effect Error] {} error(s) found in {}:", errs.len(), filepath);
        for (idx, err) in errs.iter().enumerate() {
            eprintln!("  {}. [Line {}, Col {}]: {}", idx + 1, err.span.line, err.span.column, err.message);
        }
        process::exit(1);
    }

    if native {
        // Native Cranelift JIT Execution
        let mut tir_module = match tungsten_tir::compile(&ast) {
            Ok(m) => m,
            Err(e) => {
                eprintln!("\n[TIR Lowering Error]: {}", e);
                process::exit(1);
            }
        };
        tungsten_tir::optimize(&mut tir_module);

        match tungsten_codegen::compile_and_run(&tir_module) {
            Ok(_) => {}
            Err(err) => {
                eprintln!("\n[Native Execution Error]: {}", err);
                process::exit(1);
            }
        }
    } else {
        // VM Execution
        match tungsten_vm::execute(&ast) {
            Ok(_) => {}
            Err(runtime_err) => {
                eprintln!("\n[Runtime Error]: {}", runtime_err);
                process::exit(1);
            }
        }
    }
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

fn create_new_project(name: &str) {
    let project_path = Path::new(name);
    if project_path.exists() {
        eprintln!("Error: Directory '{}' already exists", name);
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
"#,
        name
    );
    let main_tg = r#"// Tungsten Main Entry Point
type Percentage = u8(0..=100);

fn main() {
    let health: Percentage = 100 as Percentage;
    println!("Welcome to Tungsten! Initial health: {}%", health);
}
"#;

    let _ = fs::write(project_path.join("Forge.toml"), forge_toml);
    let _ = fs::write(project_path.join("src").join("main.tg"), main_tg);

    println!("Created new Tungsten project `{}`", name);
}

fn run_tir(args: &[String]) {
    let mut optimize = false;
    let mut file = None;

    for arg in args {
        if arg == "--opt" || arg == "-O" {
            optimize = true;
        } else if file.is_none() {
            file = Some(arg.clone());
        }
    }

    let filepath = match file {
        Some(f) => f,
        None => {
            eprintln!("Error: 'forge tir' requires a path to a .tg file");
            process::exit(1);
        }
    };

    let ast = match load_program(&filepath) {
        Ok(prog) => prog,
        Err(err) => {
            eprintln!("{}", err);
            process::exit(1);
        }
    };

    // 2. Type Checking
    if let Err(errs) = tungsten_typeck::check(&ast) {
        eprintln!("\n[Type & Effect Error] {} error(s) found in {}:", errs.len(), filepath);
        for (idx, err) in errs.iter().enumerate() {
            eprintln!("  {}. [Line {}, Col {}]: {}", idx + 1, err.span.line, err.span.column, err.message);
        }
        process::exit(1);
    }

    // 3. TIR Lowering
    let mut module = match tungsten_tir::compile(&ast) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("\n[TIR Lowering Error]: {}", e);
            process::exit(1);
        }
    };

    if optimize {
        let stats = tungsten_tir::optimize(&mut module);
        println!(";; Optimization Passes Applied:");
        println!(";;   - Constant folds: {}", stats.const_folds);
        println!(";;   - Redundant bounds checks eliminated: {}", stats.bounds_checks_eliminated);
        println!(";;   - Dead instructions/blocks pruned: {}", stats.dead_code_pruned);
        println!(";;   - Iterations to fixpoint: {}", stats.iterations);
        println!();
    }

    // 4. Print TIR
    print!("{}", tungsten_tir::print(&module));
}

fn run_build(args: &[String]) {
    let mut file = None;
    for arg in args {
        if file.is_none() {
            file = Some(arg.clone());
        }
    }

    let filepath = match file {
        Some(f) => f,
        None => {
            eprintln!("Error: 'forge build' requires a path to a .tg file");
            process::exit(1);
        }
    };

    let ast = match load_program(&filepath) {
        Ok(prog) => prog,
        Err(err) => {
            eprintln!("{}", err);
            process::exit(1);
        }
    };

    // 2. Type Checking
    if let Err(errs) = tungsten_typeck::check(&ast) {
        eprintln!("\n[Type & Effect Error] {} error(s) found in {}:", errs.len(), filepath);
        for (idx, err) in errs.iter().enumerate() {
            eprintln!("  {}. [Line {}, Col {}]: {}", idx + 1, err.span.line, err.span.column, err.message);
        }
        process::exit(1);
    }

    // 3. TIR Lowering & Optimization
    let mut module = match tungsten_tir::compile(&ast) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("\n[TIR Lowering Error]: {}", e);
            process::exit(1);
        }
    };
    let stats = tungsten_tir::optimize(&mut module);

    println!("Compiling {} with Cranelift JIT/AOT backend...", filepath);
    println!("  - Optimized: {} constant folds, {} bounds checks eliminated", stats.const_folds, stats.bounds_checks_eliminated);

    match tungsten_codegen::compile_and_run(&module) {
        Ok(_) => {
            println!("Finished build: verification run successful.");
        }
        Err(e) => {
            eprintln!("Build verification failed: {}", e);
            process::exit(1);
        }
    }
}
