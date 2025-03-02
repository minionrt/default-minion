use std::fs;
use std::path::Path;

mod json;

use json::*;

#[derive(Debug)]
pub struct ImageMetadata {
    pub devcontainer: DevContainer,
}

pub fn load<P: AsRef<Path>>(directory: P) -> Result<DevContainer, Box<dyn std::error::Error>> {
    let devcontainer_json_path = find_devcontainer_json(directory)
        .ok_or("No devcontainer.json found in the specified directory")?;

    let devcontainer_json = fs::File::open(devcontainer_json_path)?;
    serde_json::from_reader(&devcontainer_json)
        .map_err(|e| format!("Failed to parse devcontainer.json: {}", e).into())
}
