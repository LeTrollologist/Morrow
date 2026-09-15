use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use tungsten_syntax::ast::*;

use crate::resolver::{ResolvedGraph, ResolvedPackage};

#[allow(dead_code)]
pub struct ProjectPackage {
    pub root_dir: PathBuf,
    pub manifest_path: PathBuf,
    pub entry_file: PathBuf,
    pub is_lib: bool,
}

impl ProjectPackage {
    pub fn discover(start_dir: &Path) -> Result<Self, String> {
        let manifest_path = find_manifest(start_dir)
            .ok_or_else(|| format!("Could not find Forge.toml in '{}' or any parent directory", start_dir.display()))?;
        let root_dir = manifest_path.parent().unwrap().to_path_buf();

        let src_main = root_dir.join("src").join("main.tg");
        let src_lib = root_dir.join("src").join("lib.tg");

        if src_main.exists() {
            Ok(Self {
                root_dir,
                manifest_path,
                entry_file: src_main,
                is_lib: false,
            })
        } else if src_lib.exists() {
            Ok(Self {
                root_dir,
                manifest_path,
                entry_file: src_lib,
                is_lib: true,
            })
        } else {
            let dir_name = root_dir.file_name().and_then(|s| s.to_str()).unwrap_or("main");
            let candidate = root_dir.join(format!("{}.tg", dir_name));
            if candidate.exists() {
                Ok(Self {
                    root_dir,
                    manifest_path,
                    entry_file: candidate,
                    is_lib: false,
                })
            } else {
                Err(format!(
                    "No entry point found in '{}'. Expected 'src/main.tg' (binary) or 'src/lib.tg' (library)",
                    root_dir.display()
                ))
            }
        }
    }
}

pub fn find_manifest(start: &Path) -> Option<PathBuf> {
    let mut current = if start.is_file() {
        start.parent()?.to_path_buf()
    } else {
        start.to_path_buf()
    };

    loop {
        let candidate = current.join("Forge.toml");
        if candidate.is_file() {
            return Some(candidate);
        }
        if !current.pop() {
            break;
        }
    }
    None
}

#[derive(Debug, Clone)]
pub struct ResolvedModule {
    pub file_path: PathBuf,
    pub pkg_info: Option<ResolvedPackage>,
    pub module_name: String,
    pub is_dependency: bool,
}

#[derive(Debug, Default)]
pub struct ModuleExports {
    pub public_items: HashSet<String>,
    pub private_items: HashSet<String>,
    pub mangled_names: HashMap<String, String>,
}

const EMBEDDED_PRELUDE: &str = include_str!("../../../std/prelude.tg");
const EMBEDDED_REFINEMENTS: &str = include_str!("../../../std/refinements.tg");
const EMBEDDED_EFFECTS: &str = include_str!("../../../std/effects.tg");
const EMBEDDED_COLLECTIONS: &str = include_str!("../../../std/collections.tg");
const EMBEDDED_SYNC: &str = include_str!("../../../std/sync.tg");
const EMBEDDED_NET: &str = include_str!("../../../std/net.tg");
const EMBEDDED_SLICE: &str = include_str!("../../../std/slice.tg");
const EMBEDDED_FS: &str = include_str!("../../../std/fs.tg");
const EMBEDDED_IO: &str = include_str!("../../../std/io.tg");
const EMBEDDED_PROCESS: &str = include_str!("../../../std/process.tg");

pub const EMBEDDED_STD_FILES: &[(&str, &str)] = &[
    ("prelude.tg", EMBEDDED_PRELUDE),
    ("refinements.tg", EMBEDDED_REFINEMENTS),
    ("effects.tg", EMBEDDED_EFFECTS),
    ("collections.tg", EMBEDDED_COLLECTIONS),
    ("sync.tg", EMBEDDED_SYNC),
    ("net.tg", EMBEDDED_NET),
    ("slice.tg", EMBEDDED_SLICE),
    ("fs.tg", EMBEDDED_FS),
    ("io.tg", EMBEDDED_IO),
    ("process.tg", EMBEDDED_PROCESS),
];

/// Recursively compile a package and its dependencies into a unified Program AST
pub fn compile_package_ast(
    entry_file: &Path,
    graph: Option<&ResolvedGraph>,
) -> Result<Program, String> {
    let canonical_entry = entry_file.canonicalize()
        .map_err(|e| format!("Failed to canonicalize entry file '{}': {}", entry_file.display(), e))?;

    let root_pkg = graph.and_then(|g| g.get_root());
    let current_pkg_root = if let Some(rp) = root_pkg {
        rp.root_dir.clone()
    } else if let Some(m_path) = find_manifest(&canonical_entry) {
        m_path.parent().unwrap().to_path_buf()
    } else {
        canonical_entry.parent().unwrap().to_path_buf()
    };

    let mut loaded_files = HashSet::new();
    let mut module_exports_map: HashMap<PathBuf, ModuleExports> = HashMap::new();
    let mut compiled_items: Vec<Item> = Vec::new();

    // 1. If dependency graph is present, compile dependencies first in topological order
    if let Some(g) = graph {
        for pkg_id in &g.build_order {
            if pkg_id == &g.root_id {
                continue;
            }
            if let Some(dep_pkg) = g.packages.get(pkg_id) {
                let dep_lib = dep_pkg.root_dir.join("src").join("lib.tg");
                if dep_lib.exists() {
                    load_and_process_module(
                        &dep_lib,
                        &dep_pkg.root_dir,
                        Some(dep_pkg),
                        graph,
                        false, // not root entry
                        &mut loaded_files,
                        &mut module_exports_map,
                        &mut compiled_items,
                    )?;
                }
            }
        }
    }

    // 2. Load the root entry file and any submodules it imports
    load_and_process_module(
        &canonical_entry,
        &current_pkg_root,
        root_pkg,
        graph,
        true, // is root entry
        &mut loaded_files,
        &mut module_exports_map,
        &mut compiled_items,
    )?;

    // 3. Inject standard library definitions unless #![no_std] or //! [no_std] is requested
    let entry_source = fs::read_to_string(&canonical_entry).unwrap_or_default();
    let is_no_std = has_no_std_pragma(&entry_source);
    let is_stdlib = canonical_entry.to_string_lossy().contains("std");

    if !is_no_std && !is_stdlib {
        let std_dir_opt = find_std_dir(&current_pkg_root);

        let existing_names: HashSet<String> = compiled_items.iter().filter_map(|it| match it {
            Item::TypeAlias(a) => Some(a.name.clone()),
            Item::Struct(s) => Some(s.name.clone()),
            Item::Fn(f) => Some(f.name.clone()),
            Item::Effect(e) => Some(e.name.clone()),
            Item::Enum(e) => Some(e.name.clone()),
            Item::Import(_) => None,
            Item::ExternBlock(_) => None,
        }).collect();

        let mut std_items = Vec::new();
        for (sf, embedded_content) in EMBEDDED_STD_FILES {
            let content = match &std_dir_opt {
                Some(dir) => {
                    let p = dir.join(sf);
                    fs::read_to_string(&p).unwrap_or_else(|_| embedded_content.to_string())
                }
                None => embedded_content.to_string(),
            };

            match tungsten_syntax::parse(&content) {
                Ok(std_ast) => {
                    for item in std_ast.items {
                        let name_opt = match &item {
                            Item::TypeAlias(a) => Some(&a.name),
                            Item::Struct(s) => Some(&s.name),
                            Item::Fn(f) => Some(&f.name),
                            Item::Effect(e) => Some(&e.name),
                            Item::Enum(e) => Some(&e.name),
                            Item::Import(_) => None,
                            Item::ExternBlock(_) => None,
                        };
                        if let Some(name) = name_opt {
                            if !existing_names.contains(name) {
                                std_items.push(item);
                            }
                        } else if matches!(item, Item::ExternBlock(_)) {
                            std_items.push(item);
                        }
                    }
                }
                Err(e) => {
                    eprintln!("STD PARSE ERROR in {}: {}", sf, e);
                }
            }
        }
        std_items.append(&mut compiled_items);
        compiled_items = std_items;
    }

    Ok(Program {
        items: compiled_items,
    })
}

pub fn has_no_std_pragma(source: &str) -> bool {
    for line in source.lines() {
        let trimmed = line.trim();
        if trimmed == "#![no_std]" || trimmed == "//! [no_std]" {
            return true;
        }
        if !trimmed.is_empty() && !trimmed.starts_with("//") && !trimmed.starts_with("#!") {
            break;
        }
    }
    false
}

fn find_std_dir(start: &Path) -> Option<PathBuf> {
    let mut curr = Some(start);
    while let Some(dir) = curr {
        let candidate = dir.join("std");
        if candidate.is_dir() && candidate.join("prelude.tg").is_file() {
            return Some(candidate);
        }
        curr = dir.parent();
    }
    let local_std = Path::new("std");
    if local_std.is_dir() && local_std.join("prelude.tg").is_file() {
        return Some(local_std.to_path_buf());
    }
    None
}

fn load_and_process_module(
    file_path: &Path,
    pkg_root: &Path,
    pkg: Option<&ResolvedPackage>,
    graph: Option<&ResolvedGraph>,
    is_root_entry: bool,
    loaded_files: &mut HashSet<PathBuf>,
    module_exports_map: &mut HashMap<PathBuf, ModuleExports>,
    collected_items: &mut Vec<Item>,
) -> Result<(), String> {
    let canonical = file_path.canonicalize()
        .map_err(|e| format!("Failed to canonicalize path '{}': {}", file_path.display(), e))?;

    if loaded_files.contains(&canonical) {
        return Ok(());
    }
    loaded_files.insert(canonical.clone());

    let source = fs::read_to_string(&canonical)
        .map_err(|e| format!("Failed to read source file '{}': {}", canonical.display(), e))?;

    let ast = tungsten_syntax::parse(&source)
        .map_err(|e| format!("[Syntax Error] in '{}':\n  {}", canonical.display(), e))?;

    // Determine mangling prefix for diamond dependency segregation
    // If it's a dependency package, mangle types and symbols with __pkg_{name}_v{major}_
    let dep_mangle_prefix = if let Some(p) = pkg {
        if !is_root_entry {
            Some(format!("__pkg_{}_v{}_", p.name, p.version.major))
        } else {
            None
        }
    } else {
        None
    };

    // 1. Scan items in this module, partition into public vs private
    let mut exports = ModuleExports::default();
    for item in &ast.items {
        match item {
            Item::TypeAlias(a) => {
                if a.is_pub {
                    exports.public_items.insert(a.name.clone());
                } else {
                    exports.private_items.insert(a.name.clone());
                }
                if let Some(ref pfx) = dep_mangle_prefix {
                    exports.mangled_names.insert(a.name.clone(), format!("{}{}", pfx, a.name));
                }
            }
            Item::Struct(s) => {
                if s.is_pub {
                    exports.public_items.insert(s.name.clone());
                } else {
                    exports.private_items.insert(s.name.clone());
                }
                if let Some(ref pfx) = dep_mangle_prefix {
                    exports.mangled_names.insert(s.name.clone(), format!("{}{}", pfx, s.name));
                }
            }
            Item::Fn(f) => {
                if f.is_pub {
                    exports.public_items.insert(f.name.clone());
                } else {
                    exports.private_items.insert(f.name.clone());
                }
                if let Some(ref pfx) = dep_mangle_prefix {
                    exports.mangled_names.insert(f.name.clone(), format!("{}{}", pfx, f.name));
                }
            }
            Item::Effect(e) => {
                if e.is_pub {
                    exports.public_items.insert(e.name.clone());
                } else {
                    exports.private_items.insert(e.name.clone());
                }
                if let Some(ref pfx) = dep_mangle_prefix {
                    exports.mangled_names.insert(e.name.clone(), format!("{}{}", pfx, e.name));
                }
            }
            Item::Enum(e) => {
                if e.is_pub {
                    exports.public_items.insert(e.name.clone());
                } else {
                    exports.private_items.insert(e.name.clone());
                }
                if let Some(ref pfx) = dep_mangle_prefix {
                    exports.mangled_names.insert(e.name.clone(), format!("{}{}", pfx, e.name));
                }
            }
            Item::Import(_) => {}
            Item::ExternBlock(_) => {}
        }
    }

    // 2. Resolve imports declared in this file
    // Maps alias or path prefix to target mangled name
    let mut import_rewrites: HashMap<String, String> = HashMap::new();

    for item in &ast.items {
        if let Item::Import(imp) = item {
            let resolved = resolve_import_path(
                &imp.path,
                &canonical,
                pkg_root,
                graph,
                pkg,
            )?;

            // Recursively load the imported target module if not already loaded
            let target_pkg = resolved.pkg_info.as_ref();
            let target_root = if let Some(tp) = target_pkg {
                &tp.root_dir
            } else {
                pkg_root
            };

            load_and_process_module(
                &resolved.file_path,
                target_root,
                target_pkg,
                graph,
                false,
                loaded_files,
                module_exports_map,
                collected_items,
            )?;

            // Privacy check and import mapping
            let target_canonical = resolved.file_path.canonicalize().unwrap();
            let target_exports = module_exports_map.get(&target_canonical)
                .ok_or_else(|| format!("Internal error: missing exports for '{}'", target_canonical.display()))?;

            // Check if importing a specific item: e.g. import matrix::helper;
            if imp.path.len() > 1 && !resolved.is_dependency {
                // If last segment matches a specific item in target
                let specific_name = imp.path.last().unwrap();
                if target_exports.private_items.contains(specific_name) {
                    return Err(format!(
                        "Privacy violation: cannot import private item '{}' from module '{}' (missing 'pub' modifier)",
                        specific_name, resolved.module_name
                    ));
                }
            }

            // Map imported public symbols
            let alias = imp.alias.as_ref().unwrap_or(&resolved.module_name);
            for pub_item in &target_exports.public_items {
                let resolved_name = target_exports.mangled_names.get(pub_item).cloned().unwrap_or_else(|| pub_item.clone());
                
                // Allow qualified path call: alias::pub_item -> resolved_name
                import_rewrites.insert(format!("{}::{}", alias, pub_item), resolved_name.clone());

                // If import was a direct module import without alias or specific symbol import, bring into scope
                import_rewrites.insert(pub_item.clone(), resolved_name);
            }

            // Also map private items so if someone attempts to use them, we can report a clear privacy error
            for priv_item in &target_exports.private_items {
                import_rewrites.insert(
                    format!("{}::{}", alias, priv_item),
                    format!("__PRIVATE_VIOLATION_{}_{}", alias, priv_item),
                );
            }
        }
    }

    module_exports_map.insert(canonical.clone(), exports);

    // 3. Rewrite items according to mangling and imports, checking for privacy violations
    for item in ast.items {
        if let Item::Import(_) = item {
            continue;
        }

        let mut item_to_add = item;

        // Apply internal mangling if this is a dependency package
        if let Some(ref pfx) = dep_mangle_prefix {
            mangle_item_decl(&mut item_to_add, pfx);
        }

        // Apply import rewrites and verify privacy
        rewrite_item_references(&mut item_to_add, &import_rewrites)?;

        collected_items.push(item_to_add);
    }

    Ok(())
}

/// Strict 1:1 Filesystem-to-Module Mapping
pub fn resolve_import_path(
    import_path: &[String],
    current_file: &Path,
    current_pkg_root: &Path,
    graph: Option<&ResolvedGraph>,
    current_pkg: Option<&ResolvedPackage>,
) -> Result<ResolvedModule, String> {
    if import_path.is_empty() {
        return Err("Import path cannot be empty".into());
    }

    let first_seg = &import_path[0];

    // Case 1: Check if first_seg matches a dependency package in the dependency graph
    let dep_pkg = if let Some(g) = graph {
        if let Some(cp) = current_pkg {
            g.find_dep_for_package(cp, first_seg)
        } else {
            g.packages.values().find(|p| p.name == *first_seg)
        }
    } else {
        None
    };

    if let Some(dep) = dep_pkg {
        if import_path.len() == 1 {
            // import math_lib; -> math_lib_dir/src/lib.tg
            let lib_tg = dep.root_dir.join("src").join("lib.tg");
            if !lib_tg.is_file() {
                return Err(format!(
                    "Dependency '{}' has no library entry point at '{}'",
                    first_seg, lib_tg.display()
                ));
            }
            let canon = lib_tg.canonicalize().unwrap_or(lib_tg);
            return Ok(ResolvedModule {
                file_path: canon,
                pkg_info: Some(dep.clone()),
                module_name: first_seg.clone(),
                is_dependency: true,
            });
        } else {
            // import math_lib::matrix; -> math_lib_dir/src/matrix.tg
            // import math_lib::linalg::matrix; -> math_lib_dir/src/linalg/matrix.tg
            let mut p = dep.root_dir.join("src");
            for seg in &import_path[1..import_path.len() - 1] {
                p.push(seg);
            }
            let last_seg = import_path.last().unwrap();
            p.push(format!("{}.tg", last_seg));

            if !p.is_file() {
                return Err(format!(
                    "Module '{}' in dependency '{}' does not exist at '{}'",
                    import_path[1..].join("::"),
                    first_seg,
                    p.display()
                ));
            }
            let canon = p.canonicalize().unwrap_or(p);
            return Ok(ResolvedModule {
                file_path: canon,
                pkg_info: Some(dep.clone()),
                module_name: last_seg.clone(),
                is_dependency: true,
            });
        }
    }

    // Case 2: Local submodule within the current package
    // import matrix; -> src/matrix.tg
    // import linalg::matrix; -> src/linalg/matrix.tg
    let mut candidate = current_pkg_root.join("src");
    for seg in &import_path[0..import_path.len() - 1] {
        candidate.push(seg);
    }
    let last_seg = import_path.last().unwrap();
    candidate.push(format!("{}.tg", last_seg));

    if candidate.is_file() {
        let canon = candidate.canonicalize().unwrap_or(candidate);
        return Ok(ResolvedModule {
            file_path: canon,
            pkg_info: current_pkg.cloned(),
            module_name: last_seg.clone(),
            is_dependency: false,
        });
    }

    // Fallback: check relative to current file's directory
    if let Some(parent) = current_file.parent() {
        let mut rel_candidate = parent.to_path_buf();
        for seg in &import_path[0..import_path.len() - 1] {
            rel_candidate.push(seg);
        }
        rel_candidate.push(format!("{}.tg", last_seg));
        if rel_candidate.is_file() {
            let canon = rel_candidate.canonicalize().unwrap_or(rel_candidate);
            return Ok(ResolvedModule {
                file_path: canon,
                pkg_info: current_pkg.cloned(),
                module_name: last_seg.clone(),
                is_dependency: false,
            });
        }
    }

    Err(format!(
        "Could not resolve import '{}': expected module file at '{}'",
        import_path.join("::"),
        candidate.display()
    ))
}

fn mangle_item_decl(item: &mut Item, prefix: &str) {
    match item {
        Item::TypeAlias(a) => {
            a.name = format!("{}{}", prefix, a.name);
            mangle_type_expr(&mut a.target, prefix);
        }
        Item::Struct(s) => {
            s.name = format!("{}{}", prefix, s.name);
            for f in &mut s.fields {
                mangle_type_expr(&mut f.ty, prefix);
            }
        }
        Item::Fn(f) => {
            f.name = format!("{}{}", prefix, f.name);
            for p in &mut f.params {
                mangle_type_expr(&mut p.ty, prefix);
            }
            if let Some(ref mut rt) = f.return_type {
                mangle_type_expr(rt, prefix);
            }
            mangle_block(&mut f.body, prefix);
        }
        Item::Effect(e) => {
            e.name = format!("{}{}", prefix, e.name);
            for op in &mut e.operations {
                for (_, ty) in &mut op.params {
                    mangle_type_expr(ty, prefix);
                }
                mangle_type_expr(&mut op.return_type, prefix);
            }
        }
        Item::Enum(e) => {
            e.name = format!("{}{}", prefix, e.name);
            for v in &mut e.variants {
                for p in &mut v.payload {
                    mangle_type_expr(p, prefix);
                }
            }
        }
        Item::Import(_) => {}
        Item::ExternBlock(_) => {}
    }
}

fn mangle_type_expr(ty: &mut TypeExpr, prefix: &str) {
    match ty {
        TypeExpr::Named(name, _) => {
            if !is_primitive_type(name) {
                *name = format!("{}{}", prefix, name);
            }
        }
        TypeExpr::Generic { name, args, .. } => {
            if !is_primitive_type(name) {
                *name = format!("{}{}", prefix, name);
            }
            for a in args {
                mangle_type_expr(a, prefix);
            }
        }
        TypeExpr::Refined { base, .. } => {
            if !is_primitive_type(base) {
                *base = format!("{}{}", prefix, base);
            }
        }
        TypeExpr::Relational { base, .. } => {
            if !is_primitive_type(base) {
                *base = format!("{}{}", prefix, base);
            }
        }
        TypeExpr::Ref { inner, .. } => {
            mangle_type_expr(inner, prefix);
        }
        TypeExpr::Ptr { inner, .. } => {
            mangle_type_expr(inner, prefix);
        }
        TypeExpr::Fn { params, return_type, .. } => {
            for p in params {
                mangle_type_expr(p, prefix);
            }
            mangle_type_expr(return_type, prefix);
        }
        TypeExpr::Array { elem, .. } => {
            mangle_type_expr(elem, prefix);
        }
        TypeExpr::Unit(_) => {}
    }
}

fn mangle_block(block: &mut Block, prefix: &str) {
    for stmt in &mut block.stmts {
        mangle_stmt(stmt, prefix);
    }
    if let Some(ref mut tr) = block.trailing_expr {
        mangle_expr(tr, prefix);
    }
}

fn mangle_stmt(stmt: &mut Stmt, prefix: &str) {
    match stmt {
        Stmt::Let { ty, init, .. } => {
            if let Some(ref mut t) = ty {
                mangle_type_expr(t, prefix);
            }
            mangle_expr(init, prefix);
        }
        Stmt::Assign { target, value, .. } => {
            mangle_expr(target, prefix);
            mangle_expr(value, prefix);
        }
        Stmt::Expr { expr, .. } => {
            mangle_expr(expr, prefix);
        }
        Stmt::Return { value, .. } => {
            if let Some(ref mut val) = value {
                mangle_expr(val, prefix);
            }
        }
    }
}

fn mangle_expr(expr: &mut Expr, prefix: &str) {
    match &mut expr.kind {
        ExprKind::Ident(_) => {}
        ExprKind::StructInit { name, fields } => {
            if !is_primitive_type(name) {
                *name = format!("{}{}", prefix, name);
            }
            for (_, f_expr) in fields {
                mangle_expr(f_expr, prefix);
            }
        }
        ExprKind::Binary { left, right, .. } => {
            mangle_expr(left, prefix);
            mangle_expr(right, prefix);
        }
        ExprKind::FieldAccess { target, .. } => {
            mangle_expr(target, prefix);
        }
        ExprKind::MethodCall { target, args, .. } => {
            mangle_expr(target, prefix);
            for a in args {
                mangle_expr(a, prefix);
            }
        }
        ExprKind::PathCall { args, .. } => {
            for a in args {
                mangle_expr(a, prefix);
            }
        }
        ExprKind::Call { callee, args } => {
            if let ExprKind::Ident(ref mut fname) = callee.kind {
                if !is_builtin_function(fname) {
                    *fname = format!("{}{}", prefix, fname);
                }
            } else {
                mangle_expr(callee, prefix);
            }
            for a in args {
                mangle_expr(a, prefix);
            }
        }
        ExprKind::MacroCall { args, .. } => {
            for a in args {
                mangle_expr(a, prefix);
            }
        }
        ExprKind::Cast { expr, target_ty } => {
            mangle_expr(expr, prefix);
            mangle_type_expr(target_ty, prefix);
        }
        ExprKind::Try(e) | ExprKind::EffectCall(e) | ExprKind::Await(e) | ExprKind::Ref { expr: e, .. } => {
            mangle_expr(e, prefix);
        }
        ExprKind::Handle { body, handlers } => {
            mangle_block(body, prefix);
            for h in handlers {
                for arm in &mut h.arms {
                    mangle_expr(&mut arm.body, prefix);
                }
            }
        }
        ExprKind::Array(elements) => {
            for e in elements {
                mangle_expr(e, prefix);
            }
        }
        ExprKind::Index { target, index } => {
            mangle_expr(target, prefix);
            mangle_expr(index, prefix);
        }
        ExprKind::Match { expr, arms } => {
            mangle_expr(expr, prefix);
            for arm in arms {
                mangle_expr(&mut arm.body, prefix);
            }
        }
        ExprKind::Nursery { body, .. } => {
            mangle_block(body, prefix);
        }
        ExprKind::Unsafe { body } => {
            mangle_block(body, prefix);
        }
        ExprKind::Deref(inner) => {
            mangle_expr(inner, prefix);
        }
        ExprKind::AddrOf { expr: inner, .. } => {
            mangle_expr(inner, prefix);
        }
        _ => {}
    }
}

fn rewrite_item_references(item: &mut Item, rewrites: &HashMap<String, String>) -> Result<(), String> {
    match item {
        Item::TypeAlias(a) => {
            rewrite_type_expr(&mut a.target, rewrites)?;
        }
        Item::Struct(s) => {
            for f in &mut s.fields {
                rewrite_type_expr(&mut f.ty, rewrites)?;
            }
        }
        Item::Fn(f) => {
            for p in &mut f.params {
                rewrite_type_expr(&mut p.ty, rewrites)?;
            }
            if let Some(ref mut rt) = f.return_type {
                rewrite_type_expr(rt, rewrites)?;
            }
            rewrite_block(&mut f.body, rewrites)?;
        }
        Item::Effect(e) => {
            for op in &mut e.operations {
                for (_, ty) in &mut op.params {
                    rewrite_type_expr(ty, rewrites)?;
                }
                rewrite_type_expr(&mut op.return_type, rewrites)?;
            }
        }
        Item::Enum(e) => {
            for v in &mut e.variants {
                for p in &mut v.payload {
                    rewrite_type_expr(p, rewrites)?;
                }
            }
        }
        Item::Import(_) => {}
        Item::ExternBlock(_) => {}
    }
    Ok(())
}

fn rewrite_type_expr(ty: &mut TypeExpr, rewrites: &HashMap<String, String>) -> Result<(), String> {
    match ty {
        TypeExpr::Named(name, span) => {
            if let Some(target) = rewrites.get(name) {
                if target.starts_with("__PRIVATE_VIOLATION_") {
                    return Err(format!("Privacy violation: type '{}' is private and cannot be accessed at line {}, col {}", name, span.line, span.column));
                }
                *name = target.clone();
            }
        }
        TypeExpr::Generic { name, args, span } => {
            if let Some(target) = rewrites.get(name) {
                if target.starts_with("__PRIVATE_VIOLATION_") {
                    return Err(format!("Privacy violation: generic type '{}' is private at line {}, col {}", name, span.line, span.column));
                }
                *name = target.clone();
            }
            for a in args {
                rewrite_type_expr(a, rewrites)?;
            }
        }
        TypeExpr::Refined { base, span, .. } => {
            if let Some(target) = rewrites.get(base) {
                if target.starts_with("__PRIVATE_VIOLATION_") {
                    return Err(format!("Privacy violation: refined base type '{}' is private at line {}, col {}", base, span.line, span.column));
                }
                *base = target.clone();
            }
        }
        TypeExpr::Relational { base, span, .. } => {
            if let Some(target) = rewrites.get(base) {
                if target.starts_with("__PRIVATE_VIOLATION_") {
                    return Err(format!("Privacy violation: relational base type '{}' is private at line {}, col {}", base, span.line, span.column));
                }
                *base = target.clone();
            }
        }
        TypeExpr::Ref { inner, .. } => {
            rewrite_type_expr(inner, rewrites)?;
        }
        TypeExpr::Ptr { inner, .. } => {
            rewrite_type_expr(inner, rewrites)?;
        }
        TypeExpr::Fn { params, return_type, .. } => {
            for p in params {
                rewrite_type_expr(p, rewrites)?;
            }
            rewrite_type_expr(return_type, rewrites)?;
        }
        TypeExpr::Array { elem, .. } => {
            rewrite_type_expr(elem, rewrites)?;
        }
        TypeExpr::Unit(_) => {}
    }
    Ok(())
}

fn rewrite_block(block: &mut Block, rewrites: &HashMap<String, String>) -> Result<(), String> {
    for stmt in &mut block.stmts {
        rewrite_stmt(stmt, rewrites)?;
    }
    if let Some(ref mut tr) = block.trailing_expr {
        rewrite_expr(tr, rewrites)?;
    }
    Ok(())
}

fn rewrite_stmt(stmt: &mut Stmt, rewrites: &HashMap<String, String>) -> Result<(), String> {
    match stmt {
        Stmt::Let { ty, init, .. } => {
            if let Some(ref mut t) = ty {
                rewrite_type_expr(t, rewrites)?;
            }
            rewrite_expr(init, rewrites)?;
        }
        Stmt::Assign { target, value, .. } => {
            rewrite_expr(target, rewrites)?;
            rewrite_expr(value, rewrites)?;
        }
        Stmt::Expr { expr, .. } => {
            rewrite_expr(expr, rewrites)?;
        }
        Stmt::Return { value, .. } => {
            if let Some(ref mut val) = value {
                rewrite_expr(val, rewrites)?;
            }
        }
    }
    Ok(())
}

fn rewrite_expr(expr: &mut Expr, rewrites: &HashMap<String, String>) -> Result<(), String> {
    match &mut expr.kind {
        ExprKind::StructInit { name, fields } => {
            if let Some(target) = rewrites.get(name) {
                if target.starts_with("__PRIVATE_VIOLATION_") {
                    return Err(format!("Privacy violation: struct '{}' is private and cannot be initialized", name));
                }
                *name = target.clone();
            }
            for (_, f_expr) in fields {
                rewrite_expr(f_expr, rewrites)?;
            }
        }
        ExprKind::Binary { left, right, .. } => {
            rewrite_expr(left, rewrites)?;
            rewrite_expr(right, rewrites)?;
        }
        ExprKind::FieldAccess { target, .. } => {
            rewrite_expr(target, rewrites)?;
        }
        ExprKind::MethodCall { target, args, .. } => {
            rewrite_expr(target, rewrites)?;
            for a in args {
                rewrite_expr(a, rewrites)?;
            }
        }
        ExprKind::PathCall { path, args } => {
            let full_path = path.join("::");
            if let Some(target) = rewrites.get(&full_path) {
                if target.starts_with("__PRIVATE_VIOLATION_") {
                    return Err(format!(
                        "Privacy violation: item '{}' in '{}' is private (missing 'pub' modifier)",
                        path.last().unwrap(), path[0]
                    ));
                }
                // Rewrite to direct function call
                for a in &mut *args {
                    rewrite_expr(a, rewrites)?;
                }
                let span = expr.span;
                let arg_list = std::mem::take(args);
                expr.kind = ExprKind::Call {
                    callee: Box::new(Expr {
                        kind: ExprKind::Ident(target.clone()),
                        span,
                    }),
                    args: arg_list,
                };
                return Ok(());
            }

            for a in args {
                rewrite_expr(a, rewrites)?;
            }
        }
        ExprKind::Call { callee, args } => {
            if let ExprKind::Ident(ref mut fname) = callee.kind {
                if let Some(target) = rewrites.get(fname) {
                    if target.starts_with("__PRIVATE_VIOLATION_") {
                        return Err(format!("Privacy violation: function '{}' is private and cannot be called", fname));
                    }
                    *fname = target.clone();
                }
            } else {
                rewrite_expr(callee, rewrites)?;
            }
            for a in args {
                rewrite_expr(a, rewrites)?;
            }
        }
        ExprKind::MacroCall { args, .. } => {
            for a in args {
                rewrite_expr(a, rewrites)?;
            }
        }
        ExprKind::Cast { expr: e, target_ty } => {
            rewrite_expr(e, rewrites)?;
            rewrite_type_expr(target_ty, rewrites)?;
        }
        ExprKind::Try(e) | ExprKind::EffectCall(e) | ExprKind::Await(e) | ExprKind::Ref { expr: e, .. } => {
            rewrite_expr(e, rewrites)?;
        }
        ExprKind::Handle { body, handlers } => {
            rewrite_block(body, rewrites)?;
            for h in handlers {
                for arm in &mut h.arms {
                    rewrite_expr(&mut arm.body, rewrites)?;
                }
            }
        }
        ExprKind::Array(elements) => {
            for e in elements {
                rewrite_expr(e, rewrites)?;
            }
        }
        ExprKind::Index { target, index } => {
            rewrite_expr(target, rewrites)?;
            rewrite_expr(index, rewrites)?;
        }
        ExprKind::Match { expr, arms } => {
            rewrite_expr(expr, rewrites)?;
            for arm in arms {
                rewrite_expr(&mut arm.body, rewrites)?;
            }
        }
        ExprKind::Nursery { body, .. } => {
            rewrite_block(body, rewrites)?;
        }
        ExprKind::Unsafe { body } => {
            rewrite_block(body, rewrites)?;
        }
        ExprKind::Deref(inner) => {
            rewrite_expr(inner, rewrites)?;
        }
        ExprKind::AddrOf { expr: inner, .. } => {
            rewrite_expr(inner, rewrites)?;
        }
        _ => {}
    }
    Ok(())
}

fn is_primitive_type(name: &str) -> bool {
    matches!(
        name,
        "u8" | "u16" | "u32" | "u64" | "i8" | "i16" | "i32" | "i64" | "f32" | "f64" | "bool" | "String" | "usize" | "isize" | "Percentage" | "Port" | "NonZeroU32" | "Byte"
    )
}

fn is_builtin_function(name: &str) -> bool {
    matches!(name, "println" | "print" | "resume" | "assert" | "panic")
}