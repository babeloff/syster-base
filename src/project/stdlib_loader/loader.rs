use crate::ide::AnalysisHost;
use crate::project::file_loader;
use crate::syntax::SyntaxFile;
use rayon::prelude::*;
use std::path::PathBuf;

/// Loads the SysML standard library into an AnalysisHost.
pub fn load_into_host(stdlib_path: &PathBuf, host: &mut AnalysisHost) -> Result<(), String> {
    if !stdlib_path.exists() || !stdlib_path.is_dir() {
        return Ok(());
    }

    // Collect all file paths first
    let file_paths = file_loader::collect_file_paths(stdlib_path)?;

    // Parse files in parallel
    let results: Vec<_> = file_paths
        .par_iter()
        .map(|path| (path, parse_file(path)))
        .collect();

    // Add successfully parsed files
    for (_path, result) in results {
        if let Ok((path, file)) = result {
            host.set_file(path, file);
        }
    }

    Ok(())
}

fn parse_file(path: &PathBuf) -> Result<(PathBuf, SyntaxFile), String> {
    file_loader::load_and_parse(path).map(|file| (path.clone(), file))
}
