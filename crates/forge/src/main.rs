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
                eprintln!("Usage: forge run <file.tg>");
                process::exit(1);
            }
            run_file(&args[2]);
        }
        "fmt" => {
            if args.len() < 3 {
                eprintln!("Error: 'forge fmt' requires a path to a .tg file");
                eprintln!("Usage: forge fmt [--check] <file.tg>");
                process::exit(1);
            }
            run_fmt(&args[2..]);
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
    check <file.tg>       Typecheck, verify refinement bounds, and check effect rows
    run <file.tg>         Compile, check, and execute a Tungsten program
    fmt [--check] <file>  Format Tungsten source code according to canonical style
    lsp                   Start the Tungsten Language Server (JSON-RPC 2.0 over stdio)
    new <project_name>    Create a new Tungsten project
    version               Display version information
    help                  Display this help message
"#
    );
}

fn run_check(filepath: &str) {
    println!("Checking {} ...", filepath);
    let source = match fs::read_to_string(filepath) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Error reading file '{}': {}", filepath, e);
            process::exit(1);
        }
    };

    // 1. Parsing
    let ast = match tungsten_syntax::parse(&source) {
        Ok(prog) => prog,
        Err(err) => {
            eprintln!("\n[Syntax Error] in {}:", filepath);
            eprintln!("  {}", err);
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

fn run_file(filepath: &str) {
    let source = match fs::read_to_string(filepath) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Error reading file '{}': {}", filepath, e);
            process::exit(1);
        }
    };

    // 1. Parsing
    let ast = match tungsten_syntax::parse(&source) {
        Ok(prog) => prog,
        Err(err) => {
            eprintln!("\n[Syntax Error] in {}:", filepath);
            eprintln!("  {}", err);
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

    // 3. VM Execution
    match tungsten_vm::execute(&ast) {
        Ok(_) => {}
        Err(runtime_err) => {
            eprintln!("\n[Runtime Error]: {}", runtime_err);
            process::exit(1);
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
