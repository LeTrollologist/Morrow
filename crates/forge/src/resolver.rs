use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use semver::{Version, VersionReq};

use crate::lockfile::{compute_package_checksum, LockedPackage, Lockfile};
use crate::manifest::{DependencySpec, Manifest};

#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct ResolvedPackage {
    pub id: String,
    pub name: String,
    pub version: Version,
    pub manifest_path: PathBuf,
    pub root_dir: PathBuf,
    pub dependencies: HashMap<String, DependencySpec>,
    pub resolved_deps: HashMap<String, String>, // alias_name -> package_id (e.g. "math_pkg" -> "math_pkg@1.0.0")
    pub checksum: String,
    pub source: String,
}

#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct ResolvedGraph {
    pub root_name: String,
    pub root_id: String,
    pub packages: HashMap<String, ResolvedPackage>,
    pub build_order: Vec<String>,
}

impl ResolvedGraph {
    pub fn get_root(&self) -> Option<&ResolvedPackage> {
        self.packages.get(&self.root_id)
    }

    pub fn find_dep_for_package(&self, pkg: &ResolvedPackage, dep_alias: &str) -> Option<&ResolvedPackage> {
        if let Some(dep_id) = pkg.resolved_deps.get(dep_alias) {
            self.packages.get(dep_id)
        } else {
            // Fallback: look up by package name directly
            self.packages.values().find(|p| p.name == dep_alias)
        }
    }

    pub fn generate_lockfile(&self) -> Lockfile {
        let mut lock = Lockfile::new();
        for id in &self.build_order {
            if let Some(pkg) = self.packages.get(id) {
                let mut dep_strings = Vec::new();
                for (dep_name, dep_id) in &pkg.resolved_deps {
                    if let Some(dep_pkg) = self.packages.get(dep_id) {
                        dep_strings.push(format!("{} {}", dep_name, dep_pkg.version));
                    }
                }
                dep_strings.sort();

                lock.upsert_package(LockedPackage {
                    name: pkg.name.clone(),
                    version: pkg.version.to_string(),
                    source: pkg.source.clone(),
                    checksum: pkg.checksum.clone(),
                    dependencies: dep_strings,
                });
            }
        }
        lock
    }

    pub fn verify_lockfile(&self, lock: &Lockfile) -> Result<(), String> {
        for pkg in self.packages.values() {
            let ver_str = pkg.version.to_string();
            if let Some(locked) = lock.find_package(&pkg.name, Some(&ver_str)) {
                if locked.version != ver_str {
                    return Err(format!(
                        "Lockfile out of sync for '{}': locked version '{}' does not match resolved version '{}'. Run 'forge lock' to update.",
                        pkg.name, locked.version, pkg.version
                    ));
                }
                if locked.checksum != pkg.checksum {
                    return Err(format!(
                        "Checksum mismatch for package '{} v{}' (source files modified since lockfile generated). Run 'forge lock' to update.",
                        pkg.name, pkg.version
                    ));
                }
            } else {
                return Err(format!(
                    "Package '{} v{}' is not present in Forge.lock. Run 'forge lock' to update.",
                    pkg.name, pkg.version
                ));
            }
        }
        Ok(())
    }
}

pub struct DependencyResolver {
    packages: HashMap<String, ResolvedPackage>,
    visiting: Vec<String>,
    visited: HashSet<String>,
    build_order: Vec<String>,
}

impl DependencyResolver {
    pub fn new() -> Self {
        Self {
            packages: HashMap::new(),
            visiting: Vec::new(),
            visited: HashSet::new(),
            build_order: Vec::new(),
        }
    }

    pub fn resolve(mut self, root_manifest_path: &Path) -> Result<ResolvedGraph, String> {
        let root_manifest = Manifest::from_file(root_manifest_path)?;
        let root_dir = root_manifest_path.parent().unwrap_or_else(|| Path::new(".")).canonicalize()
            .map_err(|e| format!("Failed to canonicalize root dir: {}", e))?;

        let root_name = root_manifest.package.name.clone();
        let root_version = Version::parse(&root_manifest.package.version)
            .map_err(|e| format!("Invalid version in root manifest '{}': {}", root_manifest.package.version, e))?;
        let root_checksum = compute_package_checksum(&root_dir)?;
        let root_id = root_name.clone();

        self.resolve_node(
            &root_id,
            &root_name,
            &root_manifest,
            root_manifest_path,
            &root_dir,
            "root",
            root_version,
            root_checksum,
        )?;

        Ok(ResolvedGraph {
            root_name,
            root_id,
            packages: self.packages,
            build_order: self.build_order,
        })
    }

    fn resolve_node(
        &mut self,
        pkg_id: &str,
        pkg_name: &str,
        manifest: &Manifest,
        manifest_path: &Path,
        root_dir: &Path,
        source: &str,
        version: Version,
        checksum: String,
    ) -> Result<(), String> {
        if self.visiting.contains(&pkg_id.to_string()) {
            let cycle = self.visiting.join(" -> ");
            return Err(format!("Circular dependency detected: {} -> {}", cycle, pkg_id));
        }

        if self.visited.contains(pkg_id) {
            return Ok(());
        }

        self.visiting.push(pkg_id.to_string());

        let mut resolved_deps = HashMap::new();

        // Recurse into dependencies
        for (dep_name, dep_spec) in &manifest.dependencies {
            let dep_dir = if let Some(path_str) = dep_spec.path() {
                root_dir.join(path_str).canonicalize()
                    .map_err(|e| format!("Could not find dependency '{}' at path '{}': {}", dep_name, path_str, e))?
            } else {
                return Err(format!(
                    "Dependency '{}' has no local path specified (remote registry resolution not yet configured)",
                    dep_name
                ));
            };

            let dep_manifest_path = dep_dir.join("Forge.toml");
            if !dep_manifest_path.exists() {
                return Err(format!("Missing Forge.toml for dependency '{}' in '{}'", dep_name, dep_dir.display()));
            }

            let dep_manifest = Manifest::from_file(&dep_manifest_path)?;
            if dep_manifest.package.name != *dep_name {
                return Err(format!(
                    "Package name mismatch: expected '{}', found '{}' in '{}'",
                    dep_name, dep_manifest.package.name, dep_manifest_path.display()
                ));
            }

            let dep_version = Version::parse(&dep_manifest.package.version)
                .map_err(|e| format!("Invalid version '{}' for package '{}': {}", dep_manifest.package.version, dep_name, e))?;

            // Validate version constraint
            if let Some(req_str) = dep_spec.version_req() {
                let req = VersionReq::parse(req_str)
                    .map_err(|e| format!("Invalid version requirement '{}' for dependency '{}': {}", req_str, dep_name, e))?;
                if !req.matches(&dep_version) {
                    return Err(format!(
                        "Version constraint mismatch for '{}': required '{}', but found version '{}'",
                        dep_name, req_str, dep_version
                    ));
                }
            }

            let dep_id = format!("{}@{}", dep_name, dep_version);
            resolved_deps.insert(dep_name.clone(), dep_id.clone());

            let dep_source = format!("path+{}", dep_dir.display());
            let dep_checksum = compute_package_checksum(&dep_dir)?;

            self.resolve_node(
                &dep_id,
                dep_name,
                &dep_manifest,
                &dep_manifest_path,
                &dep_dir,
                &dep_source,
                dep_version,
                dep_checksum,
            )?;
        }

        let pkg = ResolvedPackage {
            id: pkg_id.to_string(),
            name: pkg_name.to_string(),
            version: version.clone(),
            manifest_path: manifest_path.to_path_buf(),
            root_dir: root_dir.to_path_buf(),
            dependencies: manifest.dependencies.clone(),
            resolved_deps,
            checksum,
            source: source.to_string(),
        };

        self.visiting.pop();
        self.visited.insert(pkg_id.to_string());
        self.packages.insert(pkg_id.to_string(), pkg);
        self.build_order.push(pkg_id.to_string());

        Ok(())
    }
}