# `app-locker-rs`

Production-grade Linux Application Locker & Sandbox utility written in Rust. `app-locker-rs` launches targeted applications inside an isolated, encrypted workspace using FUSE at-rest encryption ([`gocryptfs`](https://github.com/rfjakob/gocryptfs)) and unprivileged Linux namespaces ([`bubblewrap`](https://github.com/containers/bubblewrap)).

Includes a **Native Desktop GUI**, **Automatic Locked Desktop Launcher Generation**, and **Feature-Rich CLI**.

---

## Architecture & Core Features

- **Automated Desktop Launchers with Password Dialog**:
  - Generates Linux system desktop shortcuts (`.desktop` files) in `~/.local/share/applications/` and `~/Desktop/`.
  - Configured with a locked security icon (`security-high`).
  - Clicking the launcher opens a small, native GTK password prompt modal (`zenity`), unlocks the FUSE vault, launches the sandboxed app, and automatically locks/unmounts on process exit.
- **Lightweight Native Desktop GUI**: Built directly in Rust (`eframe`/`egui`). Includes a **`📌 Shortcut`** button next to each vault to generate desktop shortcuts with one click.
- **At-Rest Encryption (`gocryptfs`)**: Application configurations and data files are encrypted on disk at `~/.local/share/app-locker/vaults/<app-name>`.
- **Process & Namespace Isolation (`bwrap`)**:
  - Read-only root filesystem (`/`)
  - Bound `/dev` and `/proc`
  - Isolated temporary directory (`/tmp`) and shared memory (`/dev/shm`)
  - Isolated PID namespace (`--unshare-pid`)
  - Lifecycle tied to parent process (`--die-with-parent`)
  - Decrypted vault mounted directly to application's `~/.config/<app-name>` and `~/.local/share/<app-name>` directories.
  - GUI pass-through for X11, Wayland, D-Bus, fonts, and themes.
- **RAII Mount Guard (`VaultGuard`)**: Rust `Drop` trait implementation guarantees calling `fusermount -u <mount_path>` (with lazy `-z` fallback) and removing temporary mount directories on exit, panic, signal, or error.

---

## Prerequisites & System Dependencies

Ensure the following system binaries are installed and available in your `PATH` or standard binary paths (`/usr/bin`, `~/.local/bin`):

| Dependency | Purpose | Package Name (Debian/Ubuntu) |
| :--- | :--- | :--- |
| `rustc` & `cargo` | Rust compiler & package manager | `cargo` / `rustup` |
| `gocryptfs` | FUSE encrypted filesystem | `gocryptfs` |
| `bwrap` | Unprivileged namespace sandbox engine | `bubblewrap` |
| `fusermount` / `fusermount3` | FUSE filesystem unmount tool | `fuse3` / `fuse` |
| `zenity` | Native GTK password dialog for desktop launchers | `zenity` |

---

## Building & Installation

Clone the repository and build the release binary using Cargo:

```bash
cd app-locker-rs
cargo build --release
```

The compiled binary will be located at `./target/release/app-locker-rs`.

Optional: Install the binary to `~/.local/bin`:

```bash
cp ./target/release/app-locker-rs ~/.local/bin/app-locker
```

---

## How to Run

### 1. Create Desktop Launcher (with Locked Icon & Password Dialog)

Generate a desktop launcher for an application:

```bash
# Create launcher for Brave Browser
./target/release/app-locker-rs desktop brave --exec brave-browser

# Create launcher for Gedit
./target/release/app-locker-rs desktop gedit
```

*The launcher appears in your system Application Menu and on `~/Desktop` with a **Locked Security Icon**. Double-clicking it opens a password prompt dialog before launching.*

### 2. Launch Native Desktop GUI

Simply launch `app-locker` without arguments (or click **`📌 Shortcut`** in the GUI):

```bash
./target/release/app-locker-rs
```

### 3. Terminal Execution

Launch an application inside an encrypted, isolated workspace via CLI:

```bash
./target/release/app-locker-rs run brave --exec brave-browser
```
