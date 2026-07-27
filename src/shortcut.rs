use anyhow::{anyhow, Context, Result};
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

pub fn create_desktop_shortcut(app_name: &str, exec: Option<&str>) -> Result<PathBuf> {
    let home_dir = dirs::home_dir()
        .ok_or_else(|| anyhow!("Could not resolve user home directory"))?;

    // Determine current binary path
    let current_exe = std::env::current_exe()
        .context("Failed to get current executable path")?;

    let target_exec = exec.unwrap_or(app_name);

    // Resolve or generate badged icon
    let icon_spec = match resolve_badged_icon(app_name, target_exec) {
        Ok(path) => {
            println!("[app-locker] Generated custom badged icon at {}", path);
            path
        }
        Err(e) => {
            eprintln!("[app-locker] Note on icon resolution for '{}': {:#}", app_name, e);
            "security-high".to_string()
        }
    };

    let desktop_entry_content = format!(
        "[Desktop Entry]\n\
         Version=1.0\n\
         Type=Application\n\
         Name={} (Locked)\n\
         Comment=Launch {} inside an encrypted, isolated App Locker sandbox\n\
         Exec=\"{}\" run-gui \"{}\" --exec \"{}\"\n\
         Icon={}\n\
         Terminal=false\n\
         Categories=Utility;Security;\n",
        app_name,
        app_name,
        current_exe.display(),
        app_name,
        target_exec,
        icon_spec
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

pub fn remove_desktop_shortcut(app_name: &str) -> Result<()> {
    if let Some(home_dir) = dirs::home_dir() {
        let desktop_file_name = format!("app-locker-{}.desktop", app_name);
        let app_path = home_dir.join(".local/share/applications").join(&desktop_file_name);
        if app_path.exists() {
            let _ = fs::remove_file(app_path);
        }

        let desktop_copy = home_dir.join("Desktop").join(&desktop_file_name);
        if desktop_copy.exists() {
            let _ = fs::remove_file(desktop_copy);
        }

        let custom_icon = home_dir
            .join(".local/share/app-locker/icons")
            .join(format!("{}-locked.png", app_name));
        if custom_icon.exists() {
            let _ = fs::remove_file(custom_icon);
        }
    }
    Ok(())
}

/// Finds the original app icon on system and overlays a lock badge onto it.
fn resolve_badged_icon(app_name: &str, target_exec: &str) -> Result<String> {
    let original_icon_path = find_original_app_icon(app_name, target_exec)?;

    let home_dir = dirs::home_dir()
        .ok_or_else(|| anyhow!("Could not resolve home directory"))?;

    let icons_dir = home_dir.join(".local/share/app-locker/icons");
    fs::create_dir_all(&icons_dir)?;

    let badged_icon_path = icons_dir.join(format!("{}-locked.png", app_name));

    // Load original image
    let mut img = image::open(&original_icon_path)
        .with_context(|| format!("Failed to open icon at {}", original_icon_path.display()))?
        .to_rgba8();

    // Composite lock badge
    draw_lock_badge(&mut img);

    // Save badged image
    img.save(&badged_icon_path)
        .with_context(|| format!("Failed to save badged icon to {}", badged_icon_path.display()))?;

    Ok(badged_icon_path.to_string_lossy().to_string())
}

/// Searches standard Linux icon directories for the original application icon.
fn find_original_app_icon(app_name: &str, target_exec: &str) -> Result<PathBuf> {
    let icon_names = [
        target_exec.to_string(),
        format!("{}-browser", app_name),
        format!("org.gnome.{}", app_name),
        app_name.to_string(),
    ];

    let search_roots = [
        PathBuf::from("/usr/share/icons"),
        PathBuf::from("/usr/share/pixmaps"),
        dirs::home_dir().unwrap_or_default().join(".local/share/icons"),
    ];

    for name in &icon_names {
        let file_name = format!("{}.png", name);
        for root in &search_roots {
            if let Ok(path) = search_file_recursive(root, &file_name) {
                return Ok(path);
            }
        }
    }

    Err(anyhow!("Could not locate PNG icon file for '{}'", app_name))
}

fn search_file_recursive(dir: &Path, target_name: &str) -> Result<PathBuf> {
    if !dir.exists() || !dir.is_dir() {
        return Err(anyhow!("Directory does not exist"));
    }

    let mut stack = vec![dir.to_path_buf()];
    while let Some(current_dir) = stack.pop() {
        if let Ok(entries) = fs::read_dir(&current_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    // Skip large non-app dirs to stay fast
                    if let Some(name) = path.file_name().and_then(|s| s.to_str()) {
                        if name == "cursors" || name == "emblems" || name == "mimetypes" {
                            continue;
                        }
                    }
                    stack.push(path);
                } else if path.file_name().and_then(|s| s.to_str()) == Some(target_name) {
                    return Ok(path);
                }
            }
        }
    }

    Err(anyhow!("File not found"))
}

/// Overlay a lock badge onto the bottom-right corner of an icon image.
fn draw_lock_badge(img: &mut image::RgbaImage) {
    let (width, height) = img.dimensions();
    let size = width.min(height) as f32;

    let radius = (size * 0.22) as i32;
    let center_x = (width as f32 - radius as f32 * 1.15) as i32;
    let center_y = (height as f32 - radius as f32 * 1.15) as i32;

    // Draw dark crimson circle background for the lock badge
    for dy in -radius..=radius {
        for dx in -radius..=radius {
            if dx * dx + dy * dy <= radius * radius {
                let px = center_x + dx;
                let py = center_y + dy;
                if px >= 0 && px < width as i32 && py >= 0 && py < height as i32 {
                    img.put_pixel(px as u32, py as u32, image::Rgba([220, 38, 38, 245]));
                }
            }
        }
    }

    // Draw white lock body inside badge
    let lock_w = (radius as f32 * 0.9) as i32;
    let lock_h = (radius as f32 * 0.75) as i32;
    let lock_left = center_x - lock_w / 2;
    let lock_top = center_y - lock_h / 4;

    for y in lock_top..(lock_top + lock_h) {
        for x in lock_left..(lock_left + lock_w) {
            if x >= 0 && x < width as i32 && y >= 0 && y < height as i32 {
                img.put_pixel(x as u32, y as u32, image::Rgba([255, 255, 255, 255]));
            }
        }
    }

    // Draw white lock shackle (arch)
    let shackle_r = (lock_w as f32 * 0.4) as i32;
    let shackle_cx = center_x;
    let shackle_cy = lock_top;
    for dy in -shackle_r..=0 {
        for dx in -shackle_r..=shackle_r {
            let dist_sq = dx * dx + dy * dy;
            let inner_r = shackle_r - 2;
            if dist_sq <= shackle_r * shackle_r && dist_sq >= inner_r * inner_r {
                let px = shackle_cx + dx;
                let py = shackle_cy + dy;
                if px >= 0 && px < width as i32 && py >= 0 && py < height as i32 {
                    img.put_pixel(px as u32, py as u32, image::Rgba([255, 255, 255, 255]));
                }
            }
        }
    }
}
