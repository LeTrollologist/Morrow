use std::fs;
use std::path::Path;

/// Information about a parsed C struct
#[derive(Debug, Clone, PartialEq)]
pub struct CStruct {
    pub name: String,
    pub fields: Vec<(String, String)>,
}

/// Information about a parsed C function declaration
#[derive(Debug, Clone, PartialEq)]
pub struct CFn {
    pub name: String,
    pub params: Vec<(String, String)>,
    pub ret_ty: String,
}

/// Information about a parsed #define constant
#[derive(Debug, Clone, PartialEq)]
pub struct CDefine {
    pub name: String,
    pub value: String,
    pub is_str: bool,
}

/// Strip C comments (/* ... */ and // ...)
pub fn strip_comments(input: &str) -> String {
    let mut out = String::new();
    let chars: Vec<char> = input.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        if i + 1 < chars.len() && chars[i] == '/' && chars[i + 1] == '*' {
            i += 2;
            while i + 1 < chars.len() && !(chars[i] == '*' && chars[i + 1] == '/') {
                i += 1;
            }
            i += 2;
        } else if i + 1 < chars.len() && chars[i] == '/' && chars[i + 1] == '/' {
            i += 2;
            while i < chars.len() && chars[i] != '\n' {
                i += 1;
            }
        } else {
            out.push(chars[i]);
            i += 1;
        }
    }
    out
}

/// Maps a C type string to a Tungsten type string
pub fn map_c_type(c_ty: &str) -> String {
    let s = c_ty.trim();
    let is_const = s.starts_with("const ") || s.ends_with(" const");
    let clean = s.replace("const", "")
                 .replace("struct", "")
                 .replace("enum", "")
                 .replace("unsigned", "unsigned_")
                 .trim()
                 .to_string();

    if clean.ends_with('*') {
        let base = clean.trim_end_matches('*').trim();
        let inner = if base.is_empty() || base == "void" {
            "u8".to_string()
        } else {
            map_c_type(base)
        };
        if is_const {
            return format!("*const {}", inner);
        } else {
            return format!("*mut {}", inner);
        }
    }

    match clean.as_str() {
        "void" => "()".to_string(),
        "char" | "int8_t" => "u8".to_string(),
        "unsigned_ char" | "uint8_t" | "unsigned_char" => "u8".to_string(),
        "short" | "int16_t" | "short int" => "i16".to_string(),
        "unsigned_ short" | "uint16_t" | "unsigned_short" => "u16".to_string(),
        "int" | "int32_t" => "i32".to_string(),
        "unsigned_ int" | "uint32_t" | "unsigned_" | "unsigned_int" => "u32".to_string(),
        "long" | "long int" | "long long" | "int64_t" => "i64".to_string(),
        "unsigned_ long" | "unsigned_ long long" | "uint64_t" | "size_t" | "uintptr_t" | "unsigned_long" => "usize".to_string(),
        "bool" | "_Bool" => "bool".to_string(),
        other => {
            if other.is_empty() {
                "u8".to_string()
            } else {
                other.to_string()
            }
        }
    }
}

/// Parse C header source and generate Tungsten code
pub fn generate_bindings(header_content: &str) -> Result<String, String> {
    let stripped = strip_comments(header_content);
    let mut defines = Vec::new();
    let mut structs = Vec::new();
    let mut functions = Vec::new();

    for line in stripped.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("#define") {
            let parts: Vec<&str> = trimmed.split_whitespace().collect();
            if parts.len() >= 3 {
                let name = parts[1];
                let val = parts[2..].join(" ");
                if val.starts_with('"') && val.ends_with('"') {
                    defines.push(CDefine {
                        name: name.to_string(),
                        value: val,
                        is_str: true,
                    });
                } else if let Ok(_) = val.parse::<i64>() {
                    defines.push(CDefine {
                        name: name.to_string(),
                        value: val,
                        is_str: false,
                    });
                }
            }
        }
    }

    // Parse structs: `struct Name { ... };` or `typedef struct { ... } Name;`
    let mut cursor = 0;
    let chars: Vec<char> = stripped.chars().collect();
    while cursor < chars.len() {
        if let Some(pos) = find_substring(&chars, cursor, "struct ") {
            let start = pos + 7;
            // Check if there is an opening brace before semicolon
            if let Some(brace_pos) = find_char(&chars, start, '{') {
                if let Some(semi_pos) = find_char(&chars, start, ';') {
                    if semi_pos < brace_pos {
                        // Forward declaration: struct Foo; - skip
                        cursor = semi_pos + 1;
                        continue;
                    }
                }
                // Extract struct name (if before brace)
                let name_candidate: String = chars[start..brace_pos].iter().collect();
                let name = name_candidate.trim().to_string();

                // Find matching closing brace
                if let Some(close_brace) = find_char(&chars, brace_pos + 1, '}') {
                    let body: String = chars[brace_pos + 1..close_brace].iter().collect();
                    let end_pos = find_char(&chars, close_brace, ';').unwrap_or(close_brace);
                    let after_brace: String = chars[close_brace + 1..end_pos].iter().collect();
                    let final_name = if !name.is_empty() {
                        name
                    } else {
                        after_brace.trim().to_string()
                    };

                    if !final_name.is_empty() {
                        let mut fields = Vec::new();
                        for field_stmt in body.split(';') {
                            let f_trim = field_stmt.trim();
                            if f_trim.is_empty() {
                                continue;
                            }
                            // Split by whitespace: type field_name
                            let parts: Vec<&str> = f_trim.split_whitespace().collect();
                            if parts.len() >= 2 {
                                let mut f_name = parts.last().unwrap().to_string();
                                let mut type_parts = parts[..parts.len() - 1].to_vec();
                                while f_name.starts_with('*') {
                                    f_name.remove(0);
                                    type_parts.push("*");
                                }
                                let c_ty = type_parts.join(" ");
                                let tg_ty = map_c_type(&c_ty);
                                fields.push((f_name, tg_ty));
                            }
                        }
                        structs.push(CStruct {
                            name: final_name,
                            fields,
                        });
                    }
                    cursor = end_pos + 1;
                    continue;
                }
            }
        }
        break;
    }

    // Parse functions: declarations ending in ';'
    for stmt in stripped.split(';') {
        let trimmed = stmt.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') || trimmed.contains('{') || trimmed.contains('}') {
            continue;
        }

        if let Some(paren_open) = trimmed.find('(') {
            if let Some(paren_close) = trimmed.rfind(')') {
                if paren_close > paren_open {
                    let pre_paren = trimmed[..paren_open].trim();
                    let params_str = trimmed[paren_open + 1..paren_close].trim();

                    // Pre-paren contains: [extern] [modifiers] <ret_type> <fn_name>
                    let pre_parts: Vec<&str> = pre_paren.split_whitespace()
                        .filter(|p| !matches!(*p, "extern" | "__cdecl" | "__stdcall" | "WINAPI" | "APIENTRY"))
                        .collect();

                    if pre_parts.is_empty() {
                        continue;
                    }

                    let mut fn_name = pre_parts.last().unwrap().to_string();
                    let mut ret_parts = pre_parts[..pre_parts.len() - 1].to_vec();
                    while fn_name.starts_with('*') {
                        fn_name.remove(0);
                        ret_parts.push("*");
                    }

                    if fn_name.is_empty() {
                        continue;
                    }

                    let ret_c_ty = ret_parts.join(" ");
                    let ret_ty = map_c_type(&ret_c_ty);

                    // Parse parameters
                    let mut params = Vec::new();
                    if !params_str.is_empty() && params_str != "void" {
                        for (idx, p) in params_str.split(',').enumerate() {
                            let p_trim = p.trim();
                            if p_trim.is_empty() || p_trim == "void" {
                                continue;
                            }
                            if p_trim == "..." {
                                // Varargs not supported directly in fn sig
                                continue;
                            }
                            let parts: Vec<&str> = p_trim.split_whitespace().collect();
                            if parts.len() == 1 {
                                // Anonymous parameter: type only
                                let p_ty = map_c_type(parts[0]);
                                params.push((format!("arg{}", idx), p_ty));
                            } else {
                                let mut p_name = parts.last().unwrap().to_string();
                                let mut type_parts = parts[..parts.len() - 1].to_vec();
                                while p_name.starts_with('*') {
                                    p_name.remove(0);
                                    type_parts.push("*");
                                }
                                let c_ty = type_parts.join(" ");
                                let p_ty = map_c_type(&c_ty);
                                params.push((p_name, p_ty));
                            }
                        }
                    }

                    functions.push(CFn {
                        name: fn_name,
                        params,
                        ret_ty,
                    });
                }
            }
        }
    }

    // Assemble Tungsten output
    let mut out = String::new();
    out.push_str("// ==========================================================\n");
    out.push_str("// Generated by forge bindgen — Tungsten C-ABI Stable Bridge\n");
    out.push_str("// ==========================================================\n\n");

    if !defines.is_empty() {
        out.push_str("// Constants (from C #define)\n");
        for d in &defines {
            if d.is_str {
                out.push_str(&format!("// const {}: String = {};\n", d.name, d.value));
            } else {
                out.push_str(&format!("// const {}: i64 = {};\n", d.name, d.value));
            }
        }
        out.push('\n');
    }

    if !structs.is_empty() {
        out.push_str("// C-Compatible Struct Layouts\n");
        for s in &structs {
            out.push_str("#[repr(C)]\n");
            out.push_str(&format!("struct {} {{\n", s.name));
            for (f_name, f_ty) in &s.fields {
                out.push_str(&format!("    {}: {},\n", f_name, f_ty));
            }
            out.push_str("}\n\n");
        }
    }

    if !functions.is_empty() {
        out.push_str("// Foreign Function Interface Declarations\n");
        out.push_str("extern \"C\" {\n");
        for f in &functions {
            let param_str: Vec<String> = f.params.iter()
                .map(|(pname, pty)| format!("{}: {}", pname, pty))
                .collect();
            if f.ret_ty == "()" {
                out.push_str(&format!("    fn {}({});\n", f.name, param_str.join(", ")));
            } else {
                out.push_str(&format!("    fn {}({}) -> {};\n", f.name, param_str.join(", "), f.ret_ty));
            }
        }
        out.push_str("}\n");
    }

    Ok(out)
}

fn find_substring(chars: &[char], start: usize, sub: &str) -> Option<usize> {
    let sub_chars: Vec<char> = sub.chars().collect();
    if sub_chars.is_empty() || start >= chars.len() {
        return None;
    }
    for i in start..=chars.len().saturating_sub(sub_chars.len()) {
        if chars[i..i + sub_chars.len()] == sub_chars[..] {
            return Some(i);
        }
    }
    None
}

fn find_char(chars: &[char], start: usize, target: char) -> Option<usize> {
    for i in start..chars.len() {
        if chars[i] == target {
            return Some(i);
        }
    }
    None
}

/// CLI runner for `forge bindgen <header.h> [--out <output.tg>]`
pub fn run_bindgen(args: &[String]) {
    if args.is_empty() {
        eprintln!("Error: 'forge bindgen' requires a path to a C header file (.h)");
        eprintln!("Usage: forge bindgen <header.h> [--out <output.tg>]");
        std::process::exit(1);
    }

    let header_path = Path::new(&args[0]);
    if !header_path.exists() {
        eprintln!("Error: header file '{}' not found", header_path.display());
        std::process::exit(1);
    }

    let mut out_path = None;
    let mut i = 1;
    while i < args.len() {
        if args[i] == "--out" || args[i] == "-o" {
            if i + 1 < args.len() {
                out_path = Some(args[i + 1].clone());
                i += 2;
                continue;
            }
        }
        i += 1;
    }

    let target_out = out_path.unwrap_or_else(|| {
        let stem = header_path.file_stem().and_then(|s| s.to_str()).unwrap_or("bindings");
        format!("{}.tg", stem)
    });

    let content = match fs::read_to_string(header_path) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Error reading header '{}': {}", header_path.display(), e);
            std::process::exit(1);
        }
    };

    match generate_bindings(&content) {
        Ok(tg_code) => {
            if let Err(e) = fs::write(&target_out, &tg_code) {
                eprintln!("Error writing output '{}': {}", target_out, e);
                std::process::exit(1);
            }
            println!("[forge bindgen] Successfully generated Tungsten bindings at '{}'", target_out);
        }
        Err(e) => {
            eprintln!("Error generating bindings: {}", e);
            std::process::exit(1);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bindgen_simple_header() {
        let header = r#"
        #define BUFFER_SIZE 4096
        #define GREETING "Hello World"

        struct Point {
            int x;
            int y;
        };

        int puts(const char *s);
        size_t strlen(const char *str);
        void *malloc(size_t size);
        void free(void *ptr);
        int add(int a, int b);
        "#;

        let tg = generate_bindings(header).unwrap();
        assert!(tg.contains("// const BUFFER_SIZE: i64 = 4096;"));
        assert!(tg.contains("// const GREETING: String = \"Hello World\";"));
        assert!(tg.contains("#[repr(C)]"));
        assert!(tg.contains("struct Point {"));
        assert!(tg.contains("    x: i32,"));
        assert!(tg.contains("    y: i32,"));
        assert!(tg.contains("extern \"C\" {"));
        assert!(tg.contains("fn puts(s: *const u8) -> i32;"));
        assert!(tg.contains("fn strlen(str: *const u8) -> usize;"));
        assert!(tg.contains("fn malloc(size: usize) -> *mut u8;"));
        assert!(tg.contains("fn free(ptr: *mut u8);"));
        assert!(tg.contains("fn add(a: i32, b: i32) -> i32;"));
    }
}
