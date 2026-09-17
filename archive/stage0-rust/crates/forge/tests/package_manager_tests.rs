use std::fs;
use tempfile::TempDir;

use forge::lockfile::compute_package_checksum;
use forge::manifest::{DependencySpec, DetailedDependency, Manifest, PackageMeta};
use forge::package::{compile_package_ast, has_no_std_pragma, resolve_import_path};
use forge::resolver::DependencyResolver;

#[test]
fn test_manifest_parsing_and_serialization() {
    let toml_content = r#"
[package]
name = "demo_pkg"
version = "0.2.0"
edition = "2026"
authors = ["Alice <alice@tungsten.org>"]
license = "MIT"
description = "A demo package"

[dependencies]
simple_dep = "1.0.0"
path_dep = { version = "0.1.0", path = "../path_dep" }
"#;

    let manifest = Manifest::from_str(toml_content).expect("Valid manifest parsing");
    assert_eq!(manifest.package.name, "demo_pkg");
    assert_eq!(manifest.package.version, "0.2.0");
    assert_eq!(manifest.package.edition.as_deref(), Some("2026"));
    assert_eq!(manifest.dependencies.len(), 2);

    match &manifest.dependencies["simple_dep"] {
        DependencySpec::Version(v) => assert_eq!(v, "1.0.0"),
        _ => panic!("Expected simple version dependency"),
    }

    match &manifest.dependencies["path_dep"] {
        DependencySpec::Detailed(d) => {
            assert_eq!(d.version.as_deref(), Some("0.1.0"));
            assert_eq!(d.path.as_deref(), Some("../path_dep"));
        }
        _ => panic!("Expected detailed dependency"),
    }

    let serialized = manifest.to_toml().expect("Serialization to TOML");
    let reloaded = Manifest::from_str(&serialized).expect("Reload from serialized TOML");
    assert_eq!(reloaded.package.name, "demo_pkg");
    assert_eq!(reloaded.dependencies.len(), 2);
}

#[test]
fn test_lockfile_generation_and_tamper_detection() {
    let temp = TempDir::new().unwrap();
    let pkg_dir = temp.path().join("my_lib");
    fs::create_dir_all(pkg_dir.join("src")).unwrap();

    let manifest = Manifest {
        package: PackageMeta {
            name: "my_lib".into(),
            version: "0.1.0".into(),
            edition: Some("2026".into()),
            authors: None,
            license: None,
            description: None,
        },
        dependencies: Default::default(),
    };
    manifest.write_file(&pkg_dir.join("Forge.toml")).unwrap();

    let code = "pub fn greet() -> String { \"hello\" }\n";
    fs::write(pkg_dir.join("src").join("lib.tg"), code).unwrap();

    // 1. Compute checksum
    let initial_hash = compute_package_checksum(&pkg_dir).unwrap();
    assert!(!initial_hash.is_empty());

    // 2. Build graph and generate lockfile
    let resolver = DependencyResolver::new();
    let graph = resolver.resolve(&pkg_dir.join("Forge.toml")).unwrap();
    let lock = graph.generate_lockfile();
    assert_eq!(lock.packages.len(), 1);
    assert_eq!(lock.packages[0].checksum, initial_hash);

    // 3. Verification succeeds
    assert!(graph.verify_lockfile(&lock).is_ok());

    // 4. Tamper with source file
    fs::write(pkg_dir.join("src").join("lib.tg"), "pub fn greet() -> String { \"tampered\" }\n").unwrap();

    // 5. Re-resolve and verify mismatch
    let resolver2 = DependencyResolver::new();
    let graph2 = resolver2.resolve(&pkg_dir.join("Forge.toml")).unwrap();
    let verify_res = graph2.verify_lockfile(&lock);
    assert!(verify_res.is_err(), "Checksum mismatch must be detected");
    assert!(verify_res.unwrap_err().contains("Checksum mismatch"));
}

#[test]
fn test_filesystem_1_to_1_deterministic_mapping() {
    let temp = TempDir::new().unwrap();
    let root = temp.path();

    // Create math_lib package
    let math_lib_dir = root.join("math_lib");
    fs::create_dir_all(math_lib_dir.join("src").join("linalg")).unwrap();
    
    let math_manifest = Manifest {
        package: PackageMeta {
            name: "math_lib".into(),
            version: "0.1.0".into(),
            edition: None,
            authors: None,
            license: None,
            description: None,
        },
        dependencies: Default::default(),
    };
    math_manifest.write_file(&math_lib_dir.join("Forge.toml")).unwrap();

    // Files:
    // 1. math_lib/src/lib.tg
    fs::write(math_lib_dir.join("src").join("lib.tg"), "pub fn root_math() -> i64 { 1 }").unwrap();
    // 2. math_lib/src/matrix.tg
    fs::write(math_lib_dir.join("src").join("matrix.tg"), "pub fn matrix_math() -> i64 { 2 }").unwrap();
    // 3. math_lib/src/linalg/matrix.tg
    fs::write(math_lib_dir.join("src").join("linalg").join("matrix.tg"), "pub fn nested_matrix() -> i64 { 3 }").unwrap();

    // Create app package
    let app_dir = root.join("app_pkg");
    fs::create_dir_all(app_dir.join("src").join("linalg")).unwrap();

    let mut app_manifest = Manifest {
        package: PackageMeta {
            name: "app_pkg".into(),
            version: "0.1.0".into(),
            edition: None,
            authors: None,
            license: None,
            description: None,
        },
        dependencies: Default::default(),
    };
    app_manifest.add_dependency(
        "math_lib".into(),
        DependencySpec::Detailed(DetailedDependency {
            version: Some("0.1.0".into()),
            path: Some("../math_lib".into()),
            git: None,
            branch: None,
        }),
    );
    app_manifest.write_file(&app_dir.join("Forge.toml")).unwrap();

    // Local files in app:
    // 4. app_pkg/src/matrix.tg
    fs::write(app_dir.join("src").join("matrix.tg"), "pub fn local_matrix() -> i64 { 4 }").unwrap();
    // 5. app_pkg/src/linalg/matrix.tg
    fs::write(app_dir.join("src").join("linalg").join("matrix.tg"), "pub fn local_nested_matrix() -> i64 { 5 }").unwrap();

    // Resolve graph
    let resolver = DependencyResolver::new();
    let graph = resolver.resolve(&app_dir.join("Forge.toml")).unwrap();
    let app_pkg_info = graph.packages.get("app_pkg").unwrap();

    let app_main = app_dir.join("src").join("main.tg");

    // Case 1: import math_lib; -> math_lib_dir/src/lib.tg
    let res1 = resolve_import_path(&["math_lib".into()], &app_main, &app_dir, Some(&graph), Some(app_pkg_info)).unwrap();
    assert_eq!(res1.file_path, math_lib_dir.join("src").join("lib.tg").canonicalize().unwrap());

    // Case 2: import math_lib::matrix; -> math_lib_dir/src/matrix.tg
    let res2 = resolve_import_path(&["math_lib".into(), "matrix".into()], &app_main, &app_dir, Some(&graph), Some(app_pkg_info)).unwrap();
    assert_eq!(res2.file_path, math_lib_dir.join("src").join("matrix.tg").canonicalize().unwrap());

    // Case 3: import math_lib::linalg::matrix; -> math_lib_dir/src/linalg/matrix.tg
    let res3 = resolve_import_path(&["math_lib".into(), "linalg".into(), "matrix".into()], &app_main, &app_dir, Some(&graph), Some(app_pkg_info)).unwrap();
    assert_eq!(res3.file_path, math_lib_dir.join("src").join("linalg").join("matrix.tg").canonicalize().unwrap());

    // Case 4: Local submodule: import matrix; -> src/matrix.tg
    let res4 = resolve_import_path(&["matrix".into()], &app_main, &app_dir, Some(&graph), Some(app_pkg_info)).unwrap();
    assert_eq!(res4.file_path, app_dir.join("src").join("matrix.tg").canonicalize().unwrap());

    // Case 5: Local nested submodule: import linalg::matrix; -> src/linalg/matrix.tg
    let res5 = resolve_import_path(&["linalg".into(), "matrix".into()], &app_main, &app_dir, Some(&graph), Some(app_pkg_info)).unwrap();
    assert_eq!(res5.file_path, app_dir.join("src").join("linalg").join("matrix.tg").canonicalize().unwrap());
}

#[test]
fn test_visibility_and_privacy_enforcement() {
    let temp = TempDir::new().unwrap();
    let root = temp.path();

    // Create math_pkg
    let math_dir = root.join("math_pkg");
    fs::create_dir_all(math_dir.join("src")).unwrap();

    let math_manifest = Manifest {
        package: PackageMeta {
            name: "math_pkg".into(),
            version: "0.1.0".into(),
            edition: None,
            authors: None,
            license: None,
            description: None,
        },
        dependencies: Default::default(),
    };
    math_manifest.write_file(&math_dir.join("Forge.toml")).unwrap();

    let lib_code = r#"
    fn fast_sqrt_helper(x: i64) -> i64 {
        x
    }

    pub fn vector_norm(x: i64) -> i64 {
        fast_sqrt_helper(x)
    }
    "#;
    fs::write(math_dir.join("src").join("lib.tg"), lib_code).unwrap();

    // Create app_pkg that calls public vector_norm
    let app_dir = root.join("app_pkg");
    fs::create_dir_all(app_dir.join("src")).unwrap();

    let mut app_manifest = Manifest {
        package: PackageMeta {
            name: "app_pkg".into(),
            version: "0.1.0".into(),
            edition: None,
            authors: None,
            license: None,
            description: None,
        },
        dependencies: Default::default(),
    };
    app_manifest.add_dependency(
        "math_pkg".into(),
        DependencySpec::Detailed(DetailedDependency {
            version: Some("0.1.0".into()),
            path: Some("../math_pkg".into()),
            git: None,
            branch: None,
        }),
    );
    app_manifest.write_file(&app_dir.join("Forge.toml")).unwrap();

    // 1. Valid program: calls pub vector_norm
    let valid_main = r#"
    import math_pkg;

    fn main() {
        let norm = math_pkg::vector_norm(42);
    }
    "#;
    fs::write(app_dir.join("src").join("main.tg"), valid_main).unwrap();

    let resolver = DependencyResolver::new();
    let graph = resolver.resolve(&app_dir.join("Forge.toml")).unwrap();
    let ast = compile_package_ast(&app_dir.join("src").join("main.tg"), Some(&graph)).unwrap();
    let check_res = tungsten_typeck::check(&ast);
    assert!(check_res.is_ok(), "Valid public call must succeed: {:?}", check_res.err());

    // 2. Privacy violation: calls private fast_sqrt_helper via path
    let invalid_main = r#"
    import math_pkg;

    fn main() {
        let secret = math_pkg::fast_sqrt_helper(42);
    }
    "#;
    fs::write(app_dir.join("src").join("main.tg"), invalid_main).unwrap();
    let compile_err = compile_package_ast(&app_dir.join("src").join("main.tg"), Some(&graph));
    assert!(compile_err.is_err(), "Accessing private item must be rejected");
    assert!(compile_err.unwrap_err().contains("Privacy violation"));

    // 3. Privacy violation: importing private item directly
    let invalid_import = r#"
    import math_pkg::fast_sqrt_helper;

    fn main() {
        let secret = fast_sqrt_helper(42);
    }
    "#;
    fs::write(app_dir.join("src").join("main.tg"), invalid_import).unwrap();
    let import_err = compile_package_ast(&app_dir.join("src").join("main.tg"), Some(&graph));
    assert!(import_err.is_err(), "Importing private item must be rejected");
}

#[test]
fn test_diamond_dependency_resolution_and_type_segregation() {
    let temp = TempDir::new().unwrap();
    let root = temp.path();

    // 1. math_pkg v1.0.0: struct Vector { x: i64, y: i64 }
    let math_v1 = root.join("math_v1");
    fs::create_dir_all(math_v1.join("src")).unwrap();
    let m1_manifest = Manifest {
        package: PackageMeta {
            name: "math_pkg".into(),
            version: "1.0.0".into(),
            edition: None,
            authors: None,
            license: None,
            description: None,
        },
        dependencies: Default::default(),
    };
    m1_manifest.write_file(&math_v1.join("Forge.toml")).unwrap();
    let m1_code = r#"
    pub struct Vector {
        x: i64,
        y: i64,
    }

    pub fn make_vec_v1(x: i64, y: i64) -> Vector {
        Vector { x: x, y: y }
    }
    "#;
    fs::write(math_v1.join("src").join("lib.tg"), m1_code).unwrap();

    // 2. math_pkg v2.0.0: struct Vector { x: i64, y: i64, z: i64 }
    let math_v2 = root.join("math_v2");
    fs::create_dir_all(math_v2.join("src")).unwrap();
    let m2_manifest = Manifest {
        package: PackageMeta {
            name: "math_pkg".into(),
            version: "2.0.0".into(),
            edition: None,
            authors: None,
            license: None,
            description: None,
        },
        dependencies: Default::default(),
    };
    m2_manifest.write_file(&math_v2.join("Forge.toml")).unwrap();
    let m2_code = r#"
    pub struct Vector {
        x: i64,
        y: i64,
        z: i64,
    }

    pub fn make_vec_v2(x: i64, y: i64, z: i64) -> Vector {
        Vector { x: x, y: y, z: z }
    }
    "#;
    fs::write(math_v2.join("src").join("lib.tg"), m2_code).unwrap();

    // 3. lib_a depends on math_pkg v1
    let lib_a = root.join("lib_a");
    fs::create_dir_all(lib_a.join("src")).unwrap();
    let mut la_manifest = Manifest {
        package: PackageMeta {
            name: "lib_a".into(),
            version: "0.1.0".into(),
            edition: None,
            authors: None,
            license: None,
            description: None,
        },
        dependencies: Default::default(),
    };
    la_manifest.add_dependency(
        "math_pkg".into(),
        DependencySpec::Detailed(DetailedDependency {
            version: Some("^1.0".into()),
            path: Some("../math_v1".into()),
            git: None,
            branch: None,
        }),
    );
    la_manifest.write_file(&lib_a.join("Forge.toml")).unwrap();
    let la_code = r#"
    import math_pkg;

    pub fn get_v1_sum() -> i64 {
        let v = math_pkg::make_vec_v1(10, 20);
        v.x + v.y
    }
    "#;
    fs::write(lib_a.join("src").join("lib.tg"), la_code).unwrap();

    // 4. lib_b depends on math_pkg v2
    let lib_b = root.join("lib_b");
    fs::create_dir_all(lib_b.join("src")).unwrap();
    let mut lb_manifest = Manifest {
        package: PackageMeta {
            name: "lib_b".into(),
            version: "0.1.0".into(),
            edition: None,
            authors: None,
            license: None,
            description: None,
        },
        dependencies: Default::default(),
    };
    lb_manifest.add_dependency(
        "math_pkg".into(),
        DependencySpec::Detailed(DetailedDependency {
            version: Some("^2.0".into()),
            path: Some("../math_v2".into()),
            git: None,
            branch: None,
        }),
    );
    lb_manifest.write_file(&lib_b.join("Forge.toml")).unwrap();
    let lb_code = r#"
    import math_pkg;

    pub fn get_v2_sum() -> i64 {
        let v = math_pkg::make_vec_v2(10, 20, 30);
        v.x + v.y + v.z
    }
    "#;
    fs::write(lib_b.join("src").join("lib.tg"), lb_code).unwrap();

    // 5. app depends on both lib_a and lib_b
    let app = root.join("app");
    fs::create_dir_all(app.join("src")).unwrap();
    let mut app_manifest = Manifest {
        package: PackageMeta {
            name: "app".into(),
            version: "0.1.0".into(),
            edition: None,
            authors: None,
            license: None,
            description: None,
        },
        dependencies: Default::default(),
    };
    app_manifest.add_dependency(
        "lib_a".into(),
        DependencySpec::Detailed(DetailedDependency {
            version: Some("0.1.0".into()),
            path: Some("../lib_a".into()),
            git: None,
            branch: None,
        }),
    );
    app_manifest.add_dependency(
        "lib_b".into(),
        DependencySpec::Detailed(DetailedDependency {
            version: Some("0.1.0".into()),
            path: Some("../lib_b".into()),
            git: None,
            branch: None,
        }),
    );
    app_manifest.write_file(&app.join("Forge.toml")).unwrap();

    let app_code = r#"
    import lib_a;
    import lib_b;

    fn main() {
        let s1 = lib_a::get_v1_sum();
        let s2 = lib_b::get_v2_sum();
    }
    "#;
    fs::write(app.join("src").join("main.tg"), app_code).unwrap();

    // Resolve diamond dependency graph
    let resolver = DependencyResolver::new();
    let graph = resolver.resolve(&app.join("Forge.toml")).unwrap();

    // Verify both math_pkg versions exist in graph and lockfile
    assert!(graph.packages.contains_key("math_pkg@1.0.0"));
    assert!(graph.packages.contains_key("math_pkg@2.0.0"));

    let lock = graph.generate_lockfile();
    assert_eq!(lock.packages.iter().filter(|p| p.name == "math_pkg").count(), 2);

    // Compile and typecheck unified AST
    let ast = compile_package_ast(&app.join("src").join("main.tg"), Some(&graph)).unwrap();
    let check_res = tungsten_typeck::check(&ast);
    assert!(check_res.is_ok(), "Diamond dependency compilation must succeed: {:?}", check_res.err());
}

#[test]
fn test_no_std_pragma_detection() {
    let source_with_pragma = "#![no_std]\n\nfn main() {}\n";
    assert!(has_no_std_pragma(source_with_pragma));

    let source_with_comment_pragma = "//! [no_std]\n\nfn main() {}\n";
    assert!(has_no_std_pragma(source_with_comment_pragma));

    let source_normal = "// Regular Tungsten file\nfn main() {}\n";
    assert!(!has_no_std_pragma(source_normal));
}

#[test]
fn test_cycle_detection() {
    let temp = TempDir::new().unwrap();
    let root = temp.path();

    // Package A -> B
    let pkg_a = root.join("pkg_a");
    fs::create_dir_all(pkg_a.join("src")).unwrap();
    let mut ma = Manifest {
        package: PackageMeta {
            name: "pkg_a".into(),
            version: "0.1.0".into(),
            edition: None,
            authors: None,
            license: None,
            description: None,
        },
        dependencies: Default::default(),
    };
    ma.add_dependency(
        "pkg_b".into(),
        DependencySpec::Detailed(DetailedDependency {
            version: Some("0.1.0".into()),
            path: Some("../pkg_b".into()),
            git: None,
            branch: None,
        }),
    );
    ma.write_file(&pkg_a.join("Forge.toml")).unwrap();
    fs::write(pkg_a.join("src").join("lib.tg"), "pub fn a() {}\n").unwrap();

    // Package B -> A (Cycle)
    let pkg_b = root.join("pkg_b");
    fs::create_dir_all(pkg_b.join("src")).unwrap();
    let mut mb = Manifest {
        package: PackageMeta {
            name: "pkg_b".into(),
            version: "0.1.0".into(),
            edition: None,
            authors: None,
            license: None,
            description: None,
        },
        dependencies: Default::default(),
    };
    mb.add_dependency(
        "pkg_a".into(),
        DependencySpec::Detailed(DetailedDependency {
            version: Some("0.1.0".into()),
            path: Some("../pkg_a".into()),
            git: None,
            branch: None,
        }),
    );
    mb.write_file(&pkg_b.join("Forge.toml")).unwrap();
    fs::write(pkg_b.join("src").join("lib.tg"), "pub fn b() {}\n").unwrap();

    let resolver = DependencyResolver::new();
    let res = resolver.resolve(&pkg_a.join("Forge.toml"));
    assert!(res.is_err(), "Circular dependency must be caught");
    let err = res.unwrap_err();
    assert!(err.contains("Circular dependency detected"));
}
