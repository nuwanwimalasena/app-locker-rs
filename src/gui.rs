use anyhow::{anyhow, Result};
use eframe::egui;
use std::fs;

use crate::config::AppConfig;
use crate::sandbox;
use crate::shortcut;
use crate::vault;

pub fn run_gui() -> Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([540.0, 600.0])
            .with_title("App Locker"),
        ..Default::default()
    };

    eframe::run_native(
        "App Locker",
        options,
        Box::new(|_cc| Ok(Box::new(AppLockerGui::default()))),
    )
    .map_err(|e| anyhow!("GUI Error: {}", e))
}

#[derive(Default)]
struct AppLockerGui {
    passphrase_input: String,
    selected_app: Option<String>,
    confirm_remove_app: Option<String>,
    status_msg: String,

    // New Vault Modal State
    show_new_modal: bool,
    new_app_name: String,
    new_passphrase: String,
    new_confirm_passphrase: String,
}

impl eframe::App for AppLockerGui {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let config = match AppConfig::new() {
            Ok(c) => c,
            Err(e) => {
                egui::CentralPanel::default().show(ctx, |ui| {
                    ui.heading("Error initializing AppLocker configuration");
                    ui.label(e.to_string());
                });
                return;
            }
        };

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.heading("🔒 App Locker");
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button("➕ New Vault").clicked() {
                        self.show_new_modal = true;
                    }
                });
            });
            ui.label("Isolated, encrypted application launcher");
            ui.separator();

            // Status message display
            if !self.status_msg.is_empty() {
                ui.colored_label(egui::Color32::LIGHT_BLUE, &self.status_msg);
                ui.add_space(4.0);
            }

            // List existing vaults
            let mut vaults = Vec::new();
            if config.vaults_dir.exists() {
                if let Ok(entries) = fs::read_dir(&config.vaults_dir) {
                    for entry in entries.flatten() {
                        if entry.path().is_dir() {
                            let name = entry.file_name().to_string_lossy().to_string();
                            let mount_path = config.get_mount_path(&name);
                            let is_mounted = mount_path.exists()
                                && fs::read_dir(&mount_path)
                                    .map_or(false, |mut i| i.next().is_some());
                            vaults.push((name, is_mounted));
                        }
                    }
                }
            }

            vaults.sort_by(|a, b| a.0.cmp(&b.0));

            if vaults.is_empty() {
                ui.add_space(20.0);
                ui.label("No vaults created yet. Click '➕ New Vault' to get started.");
            } else {
                egui::ScrollArea::vertical()
                    .max_height(260.0)
                    .show(ui, |ui| {
                        for (app_name, is_mounted) in vaults {
                            ui.group(|ui| {
                                ui.horizontal(|ui| {
                                    ui.label(egui::RichText::new(&app_name).strong());

                                    if is_mounted {
                                        ui.colored_label(egui::Color32::GREEN, "● UNLOCKED");
                                    } else {
                                        ui.colored_label(egui::Color32::GRAY, "🔒 LOCKED");
                                    }

                                    ui.with_layout(
                                        egui::Layout::right_to_left(egui::Align::Center),
                                        |ui| {
                                            if ui.button("🗑️ Remove").clicked() {
                                                self.confirm_remove_app = Some(app_name.clone());
                                            }

                                            if ui.button("📌 Shortcut").clicked() {
                                                match shortcut::create_desktop_shortcut(
                                                    &app_name, None,
                                                ) {
                                                    Ok(p) => {
                                                        self.status_msg = format!(
                                                            "Desktop launcher created: {}",
                                                            p.display()
                                                        )
                                                    }
                                                    Err(e) => {
                                                        self.status_msg = format!(
                                                            "Error creating shortcut: {}",
                                                            e
                                                        )
                                                    }
                                                }
                                            }

                                            if is_mounted {
                                                if ui.button("Lock").clicked() {
                                                    let mount_path =
                                                        config.get_mount_path(&app_name);
                                                    let mut guard =
                                                        vault::VaultGuard::new(mount_path);
                                                    match guard.unmount() {
                                                        Ok(_) => {
                                                            self.status_msg =
                                                                format!("Locked '{}'", app_name)
                                                        }
                                                        Err(e) => {
                                                            self.status_msg = format!(
                                                                "Error locking '{}': {}",
                                                                app_name, e
                                                            )
                                                        }
                                                    }
                                                }
                                            } else if ui.button("Unlock & Launch").clicked() {
                                                self.selected_app = Some(app_name.clone());
                                                self.passphrase_input.clear();
                                            }
                                        },
                                    );
                                });
                            });
                            ui.add_space(4.0);
                        }
                    });
            }

            // Inline Delete Confirmation Section
            if let Some(ref app) = self.confirm_remove_app.clone() {
                ui.add_space(8.0);
                ui.separator();
                ui.colored_label(
                    egui::Color32::LIGHT_RED,
                    format!(
                        "⚠️ Permanently delete vault & encrypted data for '{}'?",
                        app
                    ),
                );
                ui.horizontal(|ui| {
                    if ui.button("Yes, Delete Vault").clicked() {
                        match vault::remove_vault(&config, app) {
                            Ok(_) => {
                                self.status_msg = format!("Vault '{}' permanently deleted.", app)
                            }
                            Err(e) => self.status_msg = format!("Error deleting vault: {}", e),
                        }
                        self.confirm_remove_app = None;
                    }

                    if ui.button("Cancel").clicked() {
                        self.confirm_remove_app = None;
                    }
                });
            }

            // Inline Unlock / Launch Section
            if let Some(ref app) = self.selected_app.clone() {
                ui.add_space(10.0);
                ui.separator();
                ui.heading(format!("Unlock & Launch '{}'", app));

                ui.horizontal(|ui| {
                    ui.label("Passphrase:");
                    let text_edit = ui.add(
                        egui::TextEdit::singleline(&mut self.passphrase_input)
                            .password(true)
                            .hint_text("Enter vault passphrase"),
                    );
                    if text_edit.lost_focus() && ctx.input(|i| i.key_pressed(egui::Key::Enter)) {
                        self.trigger_launch(&config, app);
                    }
                });

                ui.horizontal(|ui| {
                    if ui.button("🚀 Launch").clicked() {
                        self.trigger_launch(&config, app);
                    }

                    if ui.button("Cancel").clicked() {
                        self.selected_app = None;
                        self.passphrase_input.clear();
                    }
                });
            }

            // New Vault Modal / Panel
            if self.show_new_modal {
                ui.add_space(10.0);
                ui.separator();
                ui.heading("Create New App Vault");

                ui.horizontal(|ui| {
                    ui.label("App Name:");
                    ui.text_edit_singleline(&mut self.new_app_name);
                });

                ui.horizontal(|ui| {
                    ui.label("Passphrase:");
                    ui.add(egui::TextEdit::singleline(&mut self.new_passphrase).password(true));
                });

                ui.horizontal(|ui| {
                    ui.label("Confirm:");
                    ui.add(
                        egui::TextEdit::singleline(&mut self.new_confirm_passphrase).password(true),
                    );
                });

                ui.horizontal(|ui| {
                    if ui.button("Create Vault & Shortcut").clicked() {
                        let name = self.new_app_name.trim().to_string();
                        if name.is_empty() {
                            self.status_msg = "App name cannot be empty.".to_string();
                        } else if self.new_passphrase.is_empty() {
                            self.status_msg = "Passphrase cannot be empty.".to_string();
                        } else if self.new_passphrase != self.new_confirm_passphrase {
                            self.status_msg = "Passphrases do not match.".to_string();
                        } else {
                            let vault_path = config.get_vault_path(&name);
                            match vault::init_vault_with_passphrase(
                                &vault_path,
                                &self.new_passphrase,
                            ) {
                                Ok(_) => {
                                    let _ = shortcut::create_desktop_shortcut(&name, None);
                                    self.status_msg = format!(
                                        "Vault '{}' & Desktop Shortcut created successfully!",
                                        name
                                    );
                                    self.show_new_modal = false;
                                    self.new_app_name.clear();
                                    self.new_passphrase.clear();
                                    self.new_confirm_passphrase.clear();
                                }
                                Err(e) => {
                                    self.status_msg = format!("Error creating vault: {}", e);
                                }
                            }
                        }
                    }

                    if ui.button("Cancel").clicked() {
                        self.show_new_modal = false;
                    }
                });
            }
        });
    }
}

impl AppLockerGui {
    fn trigger_launch(&mut self, config: &AppConfig, app_name: &str) {
        let vault_path = config.get_vault_path(app_name);
        let mount_path = config.get_mount_path(app_name);
        let passphrase = self.passphrase_input.clone();
        let app = app_name.to_string();

        match vault::mount_vault_with_passphrase(&vault_path, &mount_path, &passphrase) {
            Ok(guard) => {
                self.status_msg = format!("Vault '{}' mounted. Spawning sandboxed app...", app);
                let app_exec = app.clone();
                std::thread::spawn(move || {
                    let _ =
                        sandbox::run_sandboxed_app(&app, &app_exec, &[], &guard.mount_path, None);
                });
                self.selected_app = None;
                self.passphrase_input.clear();
            }
            Err(e) => {
                self.status_msg = format!("Mount failed: {}", e);
            }
        }
    }
}
