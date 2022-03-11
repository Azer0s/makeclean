use super::{remove_dirs, status_from_dirs, BuildStatus, BuildTool, BuildToolProbe};
use crate::build_tool_manager::BuildToolManager;

use serde::Deserialize;
use std::{
    fs,
    path::{Path, PathBuf},
};


pub fn register(manager: &mut BuildToolManager) {
    let probe = Box::new(CargoProbe {});
    manager.register(probe);
}

#[derive(Debug)]
pub struct CargoProbe;

impl BuildToolProbe for CargoProbe {
    fn probe(&self, path: &Path) -> Option<Box<dyn BuildTool>> {
        if path.join("Cargo.toml").is_file() {
            Some(Box::new(Cargo::new(path)))
        } else {
            None
        }
    }

    fn applies_to(&self, name: &str) -> bool {
        // `name` should already be lowercase, but let's be defensive
        let name = name.to_lowercase();
        ["cargo", "rust", "rs"].contains(&name.as_str())
    }
}

#[derive(Debug)]
pub struct Cargo {
    path: PathBuf,
}

impl Cargo {
    fn new(path: &Path) -> Self {
        Self {
            path: path.to_owned(),
        }
    }
}

static EPHEMERAL_DIRS: &[&str] = &["target"];

impl BuildTool for Cargo {
    fn clean_project(&mut self, dry_run: bool) -> anyhow::Result<()> {
        // `cargo clean` exists, but according to its man page:
        // "With no options, cargo clean will delete the entire target directory.".
        // So removing the target directory directly instead of shelling out has
        // the same effect, and also works in case Cargo is not installed on the
        // system.

        remove_dirs(&self.path, EPHEMERAL_DIRS, dry_run)
    }

    fn status(&self) -> anyhow::Result<BuildStatus> {
        status_from_dirs(&self.path, EPHEMERAL_DIRS)
    }

    fn project_name(&self) -> Option<anyhow::Result<String>> {
        let toml_path = self.path.join("Cargo.toml");
        Some(read_project_name_from_cargo_toml(&toml_path))
    }
}

impl std::fmt::Display for Cargo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Cargo")
    }
}

fn read_project_name_from_cargo_toml(toml_path: &Path) -> anyhow::Result<String> {
    let cargo_toml: CargoToml = toml::from_str(&fs::read_to_string(toml_path)?)?;
    Ok(cargo_toml.package.name)
}

#[derive(Debug, Deserialize)]
struct CargoToml {
    package: Package,
}

#[derive(Debug, Deserialize)]
struct Package {
    name: String,
}
