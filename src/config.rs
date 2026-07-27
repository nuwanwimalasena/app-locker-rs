use anyhow::{anyhow, Result};
use std::fs;
use std::path::PathBuf;

pub struct AppConfig {
    #[allow(dead_code)]
    pub base_dir: PathBuf,
    pub vaults_dir: PathBuf,
    pub mounts_dir: PathBuf,
}

impl AppConfig {
    pub fn new() -> Result<Self> {
        let data_dir = dirs::data_dir()
            .ok_or_else(|| anyhow!("Could not determine user data directory (~/.local/share)"))?;

        let base_dir = data_dir.join("app-locker");
        let vaults_dir = base_dir.join("vaults");
        let mounts_dir = base_dir.join("mounts");

        // Ensure base directories exist
        fs::create_dir_all(&vaults_dir)?;
        fs::create_dir_all(&mounts_dir)?;

        Ok(Self {
            base_dir,
            vaults_dir,
            mounts_dir,
        })
    }

    pub fn get_vault_path(&self, app_name: &str) -> PathBuf {
        self.vaults_dir.join(app_name)
    }

    pub fn get_mount_path(&self, app_name: &str) -> PathBuf {
        self.mounts_dir.join(app_name)
    }
}

/// Helper to locate system binaries in PATH or user directories.
pub fn find_binary(binary_name: &str) -> Result<PathBuf> {
    // Check PATH environment variable
    if let Ok(path_var) = std::env::var("PATH") {
        for dir in path_var.split(':') {
            let candidate = PathBuf::from(dir).join(binary_name);
            if candidate.is_file() {
                return Ok(candidate);
            }
        }
    }

    // Check fallback locations
    if let Some(home) = dirs::home_dir() {
        let local_bin = home.join(".local/bin").join(binary_name);
        if local_bin.is_file() {
            return Ok(local_bin);
        }
    }

    let standard_paths = [
        PathBuf::from("/usr/bin").join(binary_name),
        PathBuf::from("/usr/local/bin").join(binary_name),
        PathBuf::from("/bin").join(binary_name),
        PathBuf::from("/tmp").join(binary_name),
    ];

    for path in &standard_paths {
        if path.is_file() {
            return Ok(path.clone());
        }
    }

    Err(anyhow!(
        "Required system binary '{}' not found in PATH or standard locations.",
        binary_name
    ))
}
