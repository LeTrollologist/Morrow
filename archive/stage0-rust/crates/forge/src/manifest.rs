use std::collections::HashMap;
use std::fs;
use std::path::Path;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Manifest {
    pub package: PackageMeta,
    #[serde(default)]
    pub dependencies: HashMap<String, DependencySpec>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PackageMeta {
    pub name: String,
    pub version: String,
    #[serde(default)]
    pub edition: Option<String>,
    #[serde(default)]
    pub authors: Option<Vec<String>>,
    #[serde(default)]
    pub license: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum DependencySpec {
    Version(String),
    Detailed(DetailedDependency),
}

impl DependencySpec {
    pub fn version_req(&self) -> Option<&str> {
        match self {
            DependencySpec::Version(v) => Some(v.as_str()),
            DependencySpec::Detailed(d) => d.version.as_deref(),
        }
    }

    pub fn path(&self) -> Option<&str> {
        match self {
            DependencySpec::Version(_) => None,
            DependencySpec::Detailed(d) => d.path.as_deref(),
        }
    }

    #[allow(dead_code)]
    pub fn git(&self) -> Option<&str> {
        match self {
            DependencySpec::Version(_) => None,
            DependencySpec::Detailed(d) => d.git.as_deref(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct DetailedDependency {
    #[serde(default)]
    pub version: Option<String>,
    #[serde(default)]
    pub path: Option<String>,
    #[serde(default)]
    pub git: Option<String>,
    #[serde(default)]
    pub branch: Option<String>,
}

impl Manifest {
    pub fn from_file(path: &Path) -> Result<Self, String> {
        let content = fs::read_to_string(path)
            .map_err(|e| format!("Could not read manifest at '{}': {}", path.display(), e))?;
        Self::from_str(&content)
    }

    pub fn from_str(content: &str) -> Result<Self, String> {
        toml::from_str(content)
            .map_err(|e| format!("Failed to parse Forge.toml: {}", e))
    }

    pub fn to_toml(&self) -> Result<String, String> {
        toml::to_string_pretty(self)
            .map_err(|e| format!("Failed to serialize Forge.toml: {}", e))
    }

    pub fn write_file(&self, path: &Path) -> Result<(), String> {
        let content = self.to_toml()?;
        fs::write(path, content)
            .map_err(|e| format!("Could not write manifest to '{}': {}", path.display(), e))
    }

    pub fn add_dependency(&mut self, name: String, spec: DependencySpec) {
        self.dependencies.insert(name, spec);
    }
}