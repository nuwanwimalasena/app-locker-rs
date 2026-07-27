use anyhow::{anyhow, Context, Result};
use std::fs;
use std::path::Path;
use std::process::{Command, ExitStatus};

use crate::config::find_binary;

pub fn run_sandboxed_app(
    app_name: &str,
    app_exec: &str,
    app_args: &[String],
    mount_path: &Path,
    custom_target_dir: Option<&str>,
) -> Result<ExitStatus> {
    let bwrap_bin = find_binary("bwrap")?;
    let target_bin = find_binary(app_exec)
        .or_else(|_| find_binary(app_name))
        .with_context(|| format!("Target application binary '{}' not found", app_exec))?;

    let home_dir =
        dirs::home_dir().ok_or_else(|| anyhow!("Could not resolve user home directory"))?;

    // Determine target config/data directory relative to user home
    let target_subpath = match custom_target_dir {
        Some(dir) => dir.to_string(),
        None => resolve_known_app_dir(app_name, app_exec),
    };

    let target_config_dir = home_dir.join(".config").join(&target_subpath);
    let target_data_dir = home_dir.join(".local/share").join(&target_subpath);

    // Create mount points on host so bwrap can bind mount onto them
    fs::create_dir_all(&target_config_dir).with_context(|| {
        format!(
            "Failed to create host config directory: {}",
            target_config_dir.display()
        )
    })?;
    fs::create_dir_all(&target_data_dir).with_context(|| {
        format!(
            "Failed to create host data directory: {}",
            target_data_dir.display()
        )
    })?;

    println!(
        "[app-locker] Launching sandboxed process: {} (Target Config: {})",
        target_bin.display(),
        target_config_dir.display()
    );

    let mut cmd = Command::new(&bwrap_bin);

    // Required Sandbox Isolation Flags
    cmd.arg("--ro-bind").arg("/").arg("/"); // Read-only root
    cmd.arg("--dev").arg("/dev"); // Bind /dev
    cmd.arg("--proc").arg("/proc"); // Bind /proc
    cmd.arg("--tmpfs").arg("/tmp"); // Isolated /tmp
    cmd.arg("--tmpfs").arg("/dev/shm"); // Isolated shared memory for browser IPC / GPU rendering
    cmd.arg("--unshare-pid"); // Isolated PID namespace
    cmd.arg("--die-with-parent"); // Ensure sandbox dies with parent

    // Vault Binds for target app configuration & data
    cmd.arg("--bind").arg(mount_path).arg(&target_config_dir);
    cmd.arg("--bind").arg(mount_path).arg(&target_data_dir);

    // Desktop GUI & Environment Pass-through
    if let Ok(display) = std::env::var("DISPLAY") {
        cmd.arg("--setenv").arg("DISPLAY").arg(display);
        cmd.arg("--ro-bind-try")
            .arg("/tmp/.X11-unix")
            .arg("/tmp/.X11-unix");
    }

    if let Ok(wayland) = std::env::var("WAYLAND_DISPLAY") {
        cmd.arg("--setenv").arg("WAYLAND_DISPLAY").arg(&wayland);
    }

    if let Ok(xauth) = std::env::var("XAUTHORITY") {
        cmd.arg("--setenv").arg("XAUTHORITY").arg(&xauth);
        cmd.arg("--ro-bind-try").arg(&xauth).arg(&xauth);
    }

    if let Ok(xdg_runtime) = std::env::var("XDG_RUNTIME_DIR") {
        cmd.arg("--setenv").arg("XDG_RUNTIME_DIR").arg(&xdg_runtime);
        cmd.arg("--bind-try").arg(&xdg_runtime).arg(&xdg_runtime);
    }

    if let Ok(dbus) = std::env::var("DBUS_SESSION_BUS_ADDRESS") {
        cmd.arg("--setenv")
            .arg("DBUS_SESSION_BUS_ADDRESS")
            .arg(dbus);
    }

    // Preserve UI aesthetics / font bindings
    cmd.arg("--ro-bind-try")
        .arg("/usr/share/fonts")
        .arg("/usr/share/fonts");
    cmd.arg("--ro-bind-try").arg("/etc/fonts").arg("/etc/fonts");
    cmd.arg("--ro-bind-try")
        .arg("/usr/share/icons")
        .arg("/usr/share/icons");
    cmd.arg("--ro-bind-try")
        .arg("/usr/share/themes")
        .arg("/usr/share/themes");

    // Pass environment variables
    if let Ok(lang) = std::env::var("LANG") {
        cmd.arg("--setenv").arg("LANG").arg(lang);
    }
    cmd.arg("--setenv")
        .arg("HOME")
        .arg(home_dir.to_string_lossy().as_ref());

    // Executable binary
    cmd.arg(&target_bin);

    // Check if binary is a Chromium-based browser needing --no-sandbox inside bwrap
    let is_chromium = app_exec.contains("brave")
        || app_name.contains("brave")
        || app_exec.contains("chrome")
        || app_name.contains("chrome")
        || app_exec.contains("chromium")
        || app_name.contains("chromium");

    let has_no_sandbox_arg = app_args.iter().any(|a| a == "--no-sandbox");

    if is_chromium && !has_no_sandbox_arg {
        println!("[app-locker] Chromium browser engine detected: Automatically applying '--no-sandbox' flag for namespace compatibility.");
        cmd.arg("--no-sandbox");
    }

    for arg in app_args {
        cmd.arg(arg);
    }

    let status = cmd.status().with_context(|| {
        format!(
            "Failed to run bubblewrap sandbox with {}",
            target_bin.display()
        )
    })?;

    Ok(status)
}

/// Helper to map common application names to their standard config subdirectories.
fn resolve_known_app_dir(app_name: &str, app_exec: &str) -> String {
    let lower_name = app_name.to_lowercase();
    let lower_exec = app_exec.to_lowercase();

    if lower_name.contains("brave") || lower_exec.contains("brave") {
        "BraveSoftware".to_string()
    } else if lower_name.contains("chrome") || lower_exec.contains("chrome") {
        "google-chrome".to_string()
    } else if lower_name.contains("chromium") || lower_exec.contains("chromium") {
        "chromium".to_string()
    } else if lower_name.contains("code") || lower_exec.contains("code") {
        "Code".to_string()
    } else {
        app_name.to_string()
    }
}
