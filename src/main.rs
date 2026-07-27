mod config;
mod gui;
mod sandbox;
mod shortcut;
mod vault;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use config::AppConfig;
use std::fs;
use std::io::Write;

#[derive(Parser)]
#[command(
    name = "app-locker",
    author = "Antigravity Architect",
    version = "0.1.0",
    about = "Production-grade Linux App Locker & Sandbox CLI Utility in Rust",
    long_about = "Launches targeted desktop applications inside an isolated, encrypted workspace using FUSE (gocryptfs) and unprivileged Linux namespaces (bubblewrap)."
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// Launch the lightweight native desktop GUI application
    #[command(name = "gui")]
    Gui,

    /// Launch an application with a GUI password prompt (used by desktop launchers)
    #[command(name = "run-gui")]
    RunGui {
        /// Target application name (e.g. mousepad, gedit, brave)
        app_name: String,

        /// Custom binary path or executable name (defaults to app_name if omitted)
        #[arg(short, long)]
        exec: Option<String>,

        /// Override target config directory name under ~/.config/ and ~/.local/share/
        #[arg(short, long)]
        target_dir: Option<String>,

        /// Arguments passed to the target application
        #[arg(last = true)]
        args: Vec<String>,
    },

    /// Create a desktop launcher shortcut with a locked security icon
    #[command(name = "desktop")]
    Desktop {
        /// Target application name
        app_name: String,

        /// Custom executable binary
        #[arg(short, long)]
        exec: Option<String>,
    },

    /// Permanently remove an application vault, its encrypted storage, and desktop shortcuts
    #[command(name = "remove", alias = "delete")]
    Remove {
        /// Target application name
        app_name: String,

        /// Force removal without interactive confirmation
        #[arg(short, long)]
        force: bool,
    },

    /// Launch an application inside an isolated encrypted workspace via terminal
    #[command(name = "run")]
    Run {
        /// Target application name (e.g. mousepad, gedit, brave)
        app_name: String,

        /// Custom binary path or executable name (defaults to app_name if omitted)
        #[arg(short, long)]
        exec: Option<String>,

        /// Override target config directory name under ~/.config/ and ~/.local/share/
        #[arg(short, long)]
        target_dir: Option<String>,

        /// Arguments passed to the target application
        #[arg(last = true)]
        args: Vec<String>,
    },

    /// Explicitly initialize an encrypted vault for an application
    #[command(name = "init")]
    Init {
        /// Target application name
        app_name: String,
    },

    /// List all configured app vaults and mount states
    #[command(name = "list")]
    List,

    /// Explicitly unmount an active application vault
    #[command(name = "lock")]
    Lock {
        /// Target application name
        app_name: String,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let config = AppConfig::new().context("Failed to initialize AppLocker configuration")?;

    match cli.command {
        None | Some(Commands::Gui) => {
            println!("[app-locker] Launching App Locker Native GUI...");
            gui::run_gui()?;
        }
        Some(Commands::Desktop { app_name, exec }) => {
            let path = shortcut::create_desktop_shortcut(&app_name, exec.as_deref())?;
            println!(
                "[app-locker] Created desktop launcher with locked icon at: {}",
                path.display()
            );
        }
        Some(Commands::Remove { app_name, force }) => {
            let vault_path = config.get_vault_path(&app_name);
            if !vault_path.exists() {
                println!("[app-locker] Vault for '{}' does not exist.", app_name);
                return Ok(());
            }

            if !force {
                print!(
                    "[app-locker] WARNING: Permanently delete vault storage for '{}'? (y/N): ",
                    app_name
                );
                std::io::stdout().flush()?;
                let mut input = String::new();
                std::io::stdin().read_line(&mut input)?;
                let trimmed = input.trim().to_lowercase();
                if trimmed != "y" && trimmed != "yes" {
                    println!("[app-locker] Aborted removal.");
                    return Ok(());
                }
            }

            vault::remove_vault(&config, &app_name)?;
        }
        Some(Commands::RunGui {
            app_name,
            exec,
            target_dir,
            args,
        }) => {
            let target_exec = exec.unwrap_or_else(|| app_name.clone());
            let vault_path = config.get_vault_path(&app_name);
            let mount_path = config.get_mount_path(&app_name);

            // Step 1: Prompt for password using Zenity GUI dialog
            let title = format!("Unlock {} Vault", app_name);
            let passphrase = match vault::prompt_password_zenity(&title) {
                Ok(Some(p)) => p,
                Ok(None) => return Ok(()), // User canceled
                Err(_) => {
                    // Zenity fallback to terminal / prompt
                    vault::ensure_vault_initialized(&vault_path, &app_name)?;
                    let guard = vault::mount_vault(&vault_path, &mount_path, &app_name)?;
                    return sandbox::run_sandboxed_app(
                        &app_name,
                        &target_exec,
                        &args,
                        &guard.mount_path,
                        target_dir.as_deref(),
                    )
                    .map(|_| ());
                }
            };

            // Step 2: Attempt mount
            let guard =
                match vault::mount_vault_with_passphrase(&vault_path, &mount_path, &passphrase) {
                    Ok(g) => g,
                    Err(err) => {
                        vault::show_error_zenity(
                            "App Locker Error",
                            &format!("Failed to mount vault for '{}':\n{}", app_name, err),
                        );
                        return Err(err);
                    }
                };

            // Step 3: Run sandboxed application
            let _ = sandbox::run_sandboxed_app(
                &app_name,
                &target_exec,
                &args,
                &guard.mount_path,
                target_dir.as_deref(),
            );

            // Step 4: VaultGuard drops automatically here, unmounting
            drop(guard);
        }
        Some(Commands::Run {
            app_name,
            exec,
            target_dir,
            args,
        }) => {
            let target_exec = exec.unwrap_or_else(|| app_name.clone());
            let vault_path = config.get_vault_path(&app_name);
            let mount_path = config.get_mount_path(&app_name);

            vault::ensure_vault_initialized(&vault_path, &app_name)?;
            let guard = vault::mount_vault(&vault_path, &mount_path, &app_name)?;

            println!("[app-locker] App session starting for '{}'...", app_name);
            let status = sandbox::run_sandboxed_app(
                &app_name,
                &target_exec,
                &args,
                &guard.mount_path,
                target_dir.as_deref(),
            );

            match status {
                Ok(exit_code) => {
                    println!(
                        "[app-locker] App session finished with exit code: {}",
                        exit_code
                    );
                }
                Err(err) => {
                    eprintln!("[app-locker] Application sandbox error: {:#}", err);
                }
            }

            drop(guard);
        }
        Some(Commands::Init { app_name }) => {
            let vault_path = config.get_vault_path(&app_name);
            vault::ensure_vault_initialized(&vault_path, &app_name)?;
        }
        Some(Commands::List) => {
            println!("=== App Locker Vaults ===");
            if !config.vaults_dir.exists() {
                println!("No vaults directory found.");
                return Ok(());
            }

            let entries = fs::read_dir(&config.vaults_dir)?;
            let mut count = 0;
            for entry in entries {
                let entry = entry?;
                let path = entry.path();
                if path.is_dir() {
                    let app = entry.file_name().to_string_lossy().to_string();
                    let mount_path = config.get_mount_path(&app);
                    let is_mounted = mount_path.exists()
                        && fs::read_dir(&mount_path).map_or(false, |mut i| i.next().is_some());

                    let status = if is_mounted {
                        "MOUNTED (UNLOCKED)"
                    } else {
                        "LOCKED"
                    };
                    println!(
                        "- App: {:<15} Status: {:<20} Vault Path: {}",
                        app,
                        status,
                        path.display()
                    );
                    count += 1;
                }
            }

            if count == 0 {
                println!("No app vaults found.");
            }
        }
        Some(Commands::Lock { app_name }) => {
            let mount_path = config.get_mount_path(&app_name);
            if !mount_path.exists() {
                println!("App '{}' is not currently mounted.", app_name);
                return Ok(());
            }

            let mut guard = vault::VaultGuard::new(mount_path);
            guard.unmount()?;
        }
    }

    Ok(())
}
