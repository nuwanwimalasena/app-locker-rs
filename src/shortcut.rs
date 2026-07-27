use anyhow::{anyhow, Context, Result};
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;

pub fn create_desktop_shortcut(app_name: &str, exec: Option<&str>) -> Result<PathBuf> {
    let home_dir =
        dirs::home_dir().ok_or_else(|| anyhow!("Could not resolve user home directory"))?;

    // Determine current binary path
    let current_exe = std::env::current_exe().context("Failed to get current executable path")?;

    let target_exec = exec.unwrap_or(app_name);

    let desktop_entry_content = format!(
        "[Desktop Entry]\n\
         Version=1.0\n\
         Type=Application\n\
         Name={} (Locked)\n\
         Comment=Launch {} inside an encrypted, isolated App Locker sandbox\n\
         Exec=\"{}\" run-gui \"{}\" --exec \"{}\"\n\
         Icon=security-high\n\
         Terminal=false\n\
         Categories=Utility;Security;\n",
        app_name,
        app_name,
        current_exe.display(),
        app_name,
        target_exec
    );

    let apps_dir = home_dir.join(".local/share/applications");
    fs::create_dir_all(&apps_dir)?;

    let desktop_file_name = format!("app-locker-{}.desktop", app_name);
    let target_path = apps_dir.join(&desktop_file_name);

    fs::write(&target_path, &desktop_entry_content)?;

    // Set executable permissions
    let mut perms = fs::metadata(&target_path)?.permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&target_path, perms)?;

    // Also copy to ~/Desktop if ~/Desktop exists
    let desktop_dir = home_dir.join("Desktop");
    if desktop_dir.exists() && desktop_dir.is_dir() {
        let desktop_copy = desktop_dir.join(&desktop_file_name);
        let _ = fs::write(&desktop_copy, &desktop_entry_content);
        let _ = fs::set_permissions(&desktop_copy, fs::Permissions::from_mode(0o755));
    }

    Ok(target_path)
}
