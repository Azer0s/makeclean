use super::{BuildStatus, BuildTool, BuildToolProbe};
use crate::{
    build_tool_manager::BuildToolManager,
    fs::{dir_size, is_gitignored},
};
use anyhow::{bail, Context};
use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};
use tracing::debug;

pub fn register(manager: &mut BuildToolManager, probe_only: bool) -> anyhow::Result<()> {
    if !probe_only {
        let mix_is_installed = Command::new("mix")
            .arg("--version")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false);

        if !mix_is_installed {
            bail!("mix is not available");
        }
    }

    let probe = Box::new(MixProbe {});
    manager.register(probe);

    Ok(())
}

#[derive(Debug)]
pub struct MixProbe;

impl BuildToolProbe for MixProbe {
    fn probe(&self, path: &Path) -> Option<Box<dyn BuildTool>> {
        if path.join("mix.exs").is_file() {
            Some(Box::new(Mix::new(path)))
        } else {
            None
        }
    }

    fn applies_to(&self, name: &str) -> bool {
        // `name` should already be lowercase, but let's be defensive
        let name = name.to_lowercase();
        ["mix", "elixir", "ex", "exs"].contains(&name.as_str())
    }
}

#[derive(Debug)]
pub struct Mix {
    path: PathBuf,
}

static BUILD_DIR: &str = "_build";
static DEPS_DIR: &str = "deps";
static ELIXIR_LS_CACHE: &str = ".elixir_ls";

impl Mix {
    fn new(path: &Path) -> Self {
        Self {
            path: path.to_owned(),
        }
    }

    fn dir(&self, name: &str) -> Option<PathBuf> {
        let dir = self.path.join(name);
        if dir.is_dir() {
            // Directories are only considered if they are ignored by Git, as
            // `mix clean` should be good enough and removing any other
            // directories is a nice to have.
            if is_gitignored(&self.path, &dir) {
                Some(dir)
            } else {
                debug!(
                    "Skipping directory as not ignored by Git: {}",
                    dir.display()
                );
                None
            }
        } else {
            None
        }
    }
}

impl BuildTool for Mix {
    fn clean_project(&mut self, dry_run: bool) -> anyhow::Result<()> {
        let mut cmd = Command::new("mix");
        let cmd = cmd.args(["clean", "--deps"]).current_dir(&self.path);
        if dry_run {
            println!("{}: {:?}", self.path.display(), cmd);
        } else {
            let status = cmd.status().with_context(|| {
                format!(
                    "Failed to execute {:?} for project at {}",
                    cmd,
                    self.path.display()
                )
            })?;
            if !status.success() {
                bail!(
                    "Unexpected exit code {} for {:?} for project at {}",
                    status,
                    cmd,
                    self.path.display()
                );
            }
        }

        if let Some(cache_dir) = self.dir(ELIXIR_LS_CACHE) {
            if dry_run {
                println!("{}: rm -r {}", self.path.display(), cache_dir.display());
            } else {
                fs::remove_dir_all(cache_dir)?;
            }
        }

        Ok(())
    }

    fn status(&self) -> anyhow::Result<BuildStatus> {
        let size: u64 = [BUILD_DIR, DEPS_DIR, ELIXIR_LS_CACHE]
            .iter()
            .filter_map(|x| self.dir(x))
            .map(|dir| dir_size(&dir))
            .sum();

        let status = match size {
            0 => BuildStatus::Clean,
            freeable_bytes => BuildStatus::Built { freeable_bytes },
        };

        Ok(status)
    }

    fn project_name(&self) -> Option<anyhow::Result<String>> {
        // mix.exs, which contains the project name, is not easy to parse without Elixir.
        // While `mix run -e 'IO.puts(Mix.Project.config[:app])'` would work, it would
        // also compile the application, which is of course an unintended side effect.
        // To prevent false positives, we don't even try.
        None
    }
}

impl std::fmt::Display for Mix {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Mix")
    }
}

#[cfg(test)]
mod test {
    use assert_fs::{
        fixture::{FileWriteStr, PathChild},
        TempDir,
    };

    use super::*;

    #[test]
    fn elixir_ls_cache_is_only_removed_if_gitignored() {
        let root = TempDir::new().unwrap();

        root.child("normal")
            .child(".elixir_ls")
            .child("dummy")
            .write_str("dummy")
            .unwrap();
        root.child("normal")
            .child(".gitignore")
            .write_str(".elixir_ls/")
            .unwrap();

        root.child("not-ignored")
            .child(".elixir_ls")
            .child("dummy")
            .write_str("dummy")
            .unwrap();

        // In the normal case, status should report back that the project is not clean:
        let normal_status = Mix::new(&root.child("normal")).status().unwrap();
        assert!(matches!(normal_status, BuildStatus::Built{freeable_bytes} if freeable_bytes > 0));

        // If not ignored, however, the directory is not considered, so the project is clean:
        let not_ignored_status = Mix::new(&root.child("not-ignored")).status().unwrap();
        assert!(matches!(not_ignored_status, BuildStatus::Clean));
    }
}
