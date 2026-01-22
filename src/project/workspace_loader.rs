use crate::ide::AnalysisHost;
use std::path::PathBuf;

use super::file_loader;

/// Loads workspace files on demand
pub struct WorkspaceLoader;

impl WorkspaceLoader {
    pub fn new() -> Self {
        Self
    }

    /// Loads all SysML and KerML files from a directory into an AnalysisHost.
    pub fn load_directory_into_host<P: Into<PathBuf>>(
        &self,
        path: P,
        host: &mut AnalysisHost,
    ) -> Result<(), String> {
        let path = path.into();
        if !path.exists() || !path.is_dir() {
            return Err(format!("Directory not found: {}", path.display()));
        }
        self.load_directory_recursive_into_host(&path, host)
    }

    fn load_directory_recursive_into_host(
        &self,
        dir: &PathBuf,
        host: &mut AnalysisHost,
    ) -> Result<(), String> {
        let paths = file_loader::collect_file_paths(dir)?;
        let mut errors = Vec::new();

        for path in paths {
            match file_loader::load_and_parse(&path) {
                Ok(file) => {
                    host.set_file(path, file);
                }
                Err(e) => {
                    errors.push(format!("{}: {}", path.display(), e));
                }
            }
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(format!(
                "Failed to load {} file(s):\n  {}",
                errors.len(),
                errors.join("\n  ")
            ))
        }
    }

    /// Loads a single file into an AnalysisHost.
    pub fn load_file_into_host<P: Into<PathBuf>>(
        &self,
        path: P,
        host: &mut AnalysisHost,
    ) -> Result<(), String> {
        let path = path.into();
        let file = file_loader::load_and_parse(&path)?;
        host.set_file(path, file);
        Ok(())
    }
}

impl Default for WorkspaceLoader {
    fn default() -> Self {
        Self::new()
    }
}
