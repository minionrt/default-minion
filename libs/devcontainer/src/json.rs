//! https://containers.dev/implementors/json_reference/

use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// The contents of a devcontainer.json file
#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DevContainer {
    pub image: Option<String>,
}

/// Find a devcontainer.json file in the specified directory
///
/// Searched locations, in order of precedence:
///
/// 1. `.devcontainer/devcontainer.json`
/// 2. `.devcontainer.json`
/// 3. `.devcontainer/<folder>/devcontainer.json` (where <folder> is a sub-folder, one level deep)
///
/// It is valid that these files may exist in more than one location, but for now, only the first
/// one found will be returned.
/// See https://containers.dev/implementors/spec/#devcontainerjson for more information.
pub fn find_devcontainer_json<P: AsRef<Path>>(directory: P) -> Option<PathBuf> {
    let directory = directory.as_ref();

    let paths_to_check = vec![
        directory.join(".devcontainer/devcontainer.json"),
        directory.join(".devcontainer.json"),
    ];

    // Check for .devcontainer/<folder>/devcontainer.json (one level deep)
    let devcontainer_dir = directory.join(".devcontainer");
    if devcontainer_dir.exists() && devcontainer_dir.is_dir() {
        if let Ok(entries) = fs::read_dir(&devcontainer_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    let devcontainer_json = path.join("devcontainer.json");
                    if devcontainer_json.exists() {
                        return Some(devcontainer_json);
                    }
                }
            }
        }
    }

    paths_to_check.into_iter().find(|path| path.exists())
}
