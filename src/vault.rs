use anyhow::{anyhow, Context, Result};
use std::fs;
use std::io::{IsTerminal, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use crate::config::{find_binary, AppConfig};
use crate::shortcut;

/// RAII Guard that manages the lifecycle of an active FUSE mount.
/// Guarantees that `fusermount -u <mount_path>` is executed and empty mount
/// directories are cleaned up on normal exit, panic, signal, or error.
pub struct VaultGuard {
    pub mount_path: PathBuf,
    pub is_mounted: bool,
}

impl VaultGuard {
    pub fn new(mount_path: PathBuf) -> Self {
        Self {
            mount_path,
            is_mounted: true,
        }
    }

    /// Explicitly unmount the vault.
    pub fn unmount(&mut self) -> Result<()> {
        if !self.is_mounted {
            return Ok(());
        }

        println!(
            "[app-locker] Unmounting vault at {}...",
            self.mount_path.display()
        );

        // Locate fusermount or fusermount3
        let fusermount_bin = find_binary("fusermount3")
            .or_else(|_| find_binary("fusermount"))
            .context("Failed to find fusermount binary")?;

        // Attempt normal unmount
        let status = Command::new(&fusermount_bin)
            .arg("-u")
            .arg(&self.mount_path)
            .status();

        let unmount_success = match status {
            Ok(s) => s.success(),
            Err(_) => false,
        };

        // Fallback to lazy unmount (-z) if initial unmount failed
        if !unmount_success {
            println!("[app-locker] Normal unmount failed, trying lazy unmount (-z)...");
            let _ = Command::new(&fusermount_bin)
                .arg("-u")
                .arg("-z")
                .arg(&self.mount_path)
                .status();
        }

        self.is_mounted = false;

        // Clean up empty mount directory
        if self.mount_path.exists() {
            let _ = fs::remove_dir(&self.mount_path);
        }

        println!("[app-locker] Vault unmounted and mountpoint cleaned up.");
        Ok(())
    }
}

impl Drop for VaultGuard {
    fn drop(&mut self) {
        if self.is_mounted {
            if let Err(e) = self.unmount() {
                eprintln!("[app-locker] Error during Drop unmount: {:#}", e);
            }
        }
    }
}

fn read_password_securely(prompt: &str) -> Result<String> {
    if std::io::stdin().is_terminal() {
        Ok(rpassword::prompt_password(prompt)?)
    } else {
        print!("{}", prompt);
        std::io::stdout().flush()?;
        let mut input = String::new();
        std::io::stdin().read_line(&mut input)?;
        Ok(input.trim_end_matches(&['\r', '\n'][..]).to_string())
    }
}

pub fn prompt_password_zenity(title: &str) -> Result<Option<String>> {
    let zenity_bin = find_binary("zenity")?;
    let output = Command::new(&zenity_bin)
        .arg("--password")
        .arg("--title")
        .arg(title)
        .output()?;

    if output.status.success() {
        let pass = String::from_utf8_lossy(&output.stdout)
            .trim_end_matches(&['\r', '\n'][..])
            .to_string();
        Ok(Some(pass))
    } else {
        Ok(None)
    }
}

pub fn show_error_zenity(title: &str, message: &str) {
    if let Ok(zenity_bin) = find_binary("zenity") {
        let _ = Command::new(&zenity_bin)
            .arg("--error")
            .arg("--title")
            .arg(title)
            .arg("--text")
            .arg(message)
            .status();
    }
}

/// Permanently unmounts and removes an application vault, storage, and desktop shortcuts.
pub fn remove_vault(config: &AppConfig, app_name: &str) -> Result<()> {
    let mount_path = config.get_mount_path(app_name);
    if mount_path.exists() {
        let mut guard = VaultGuard::new(mount_path.clone());
        let _ = guard.unmount();
    }

    let vault_path = config.get_vault_path(app_name);
    if vault_path.exists() {
        fs::remove_dir_all(&vault_path).with_context(|| {
            format!("Failed to delete vault storage at {}", vault_path.display())
        })?;
    }

    if mount_path.exists() {
        let _ = fs::remove_dir_all(&mount_path);
    }

    shortcut::remove_desktop_shortcut(app_name)?;

    println!(
        "[app-locker] Removed vault and desktop shortcuts for '{}'.",
        app_name
    );
    Ok(())
}

/// Initializes an encrypted gocryptfs vault with an explicit passphrase.
pub fn init_vault_with_passphrase(vault_path: &Path, passphrase: &str) -> Result<()> {
    if passphrase.is_empty() {
        return Err(anyhow!("Passphrase cannot be empty."));
    }

    fs::create_dir_all(vault_path).with_context(|| {
        format!(
            "Failed to create vault directory at {}",
            vault_path.display()
        )
    })?;

    let gocryptfs_bin = find_binary("gocryptfs")?;

    let mut child = Command::new(&gocryptfs_bin)
        .arg("-init")
        .arg(vault_path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .with_context(|| format!("Failed to execute {}", gocryptfs_bin.display()))?;

    if let Some(mut stdin) = child.stdin.take() {
        writeln!(stdin, "{}", passphrase)?;
    }

    let output = child.wait_with_output()?;
    if !output.status.success() {
        let err_msg = String::from_utf8_lossy(&output.stderr);
        return Err(anyhow!("gocryptfs -init failed: {}", err_msg.trim()));
    }

    Ok(())
}

/// Ensures that an encrypted gocryptfs vault exists at `vault_path`.
/// If not, prompts user for a passphrase and initializes the vault with `gocryptfs -init`.
pub fn ensure_vault_initialized(vault_path: &Path, app_name: &str) -> Result<()> {
    let conf_file = vault_path.join("gocryptfs.conf");
    if conf_file.exists() {
        return Ok(());
    }

    println!(
        "[app-locker] Vault for '{}' does not exist at {}.",
        app_name,
        vault_path.display()
    );
    println!("[app-locker] Initializing new encrypted vault...");

    // Securely prompt for passphrase
    let passphrase = read_password_securely("Enter new master passphrase: ")?;
    let confirm = read_password_securely("Confirm master passphrase: ")?;
    if passphrase != confirm {
        return Err(anyhow!(
            "Passphrases do not match. Aborting initialization."
        ));
    }

    init_vault_with_passphrase(vault_path, &passphrase)?;

    println!(
        "[app-locker] Successfully initialized encrypted vault for '{}'.",
        app_name
    );
    Ok(())
}

/// Mounts an encrypted vault with an explicit passphrase, returning a `VaultGuard`.
pub fn mount_vault_with_passphrase(
    vault_path: &Path,
    mount_path: &Path,
    passphrase: &str,
) -> Result<VaultGuard> {
    if !mount_path.exists() {
        fs::create_dir_all(mount_path).with_context(|| {
            format!(
                "Failed to create mount directory at {}",
                mount_path.display()
            )
        })?;
    }

    let gocryptfs_bin = find_binary("gocryptfs")?;

    let mut child = Command::new(&gocryptfs_bin)
        .arg(vault_path)
        .arg(mount_path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .with_context(|| format!("Failed to execute {}", gocryptfs_bin.display()))?;

    if let Some(mut stdin) = child.stdin.take() {
        writeln!(stdin, "{}", passphrase)?;
    }

    let output = child.wait_with_output()?;
    if !output.status.success() {
        let err_msg = String::from_utf8_lossy(&output.stderr);
        let _ = fs::remove_dir(mount_path);
        return Err(anyhow!("gocryptfs mount failed: {}", err_msg.trim()));
    }

    Ok(VaultGuard::new(mount_path.to_path_buf()))
}

/// Prompts for password and mounts the encrypted vault to `mount_path`, returning a `VaultGuard`.
pub fn mount_vault(vault_path: &Path, mount_path: &Path, app_name: &str) -> Result<VaultGuard> {
    let passphrase =
        read_password_securely(&format!("Enter passphrase for vault '{}': ", app_name))?;

    println!("[app-locker] Mounting encrypted vault...");
    let guard = mount_vault_with_passphrase(vault_path, mount_path, &passphrase)?;
    println!(
        "[app-locker] Vault mounted successfully at {}.",
        mount_path.display()
    );

    Ok(guard)
}
