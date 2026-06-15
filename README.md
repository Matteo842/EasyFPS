# EasyFPS

**EasyFPS** is a lightweight, minimalist FPS (Frame Per Second) counter designed for fullscreen games on Windows.

It is designed to be a simpler, faster alternative to heavy overlay software. EasyFPS leverages the industry-standard **Intel PresentMon** (embedded directly into the executable) to monitor performance passively and securely, with near-zero impact on system resources.

<img width="1280" height="720" alt="EasyFPS-commercial" src="https://github.com/user-attachments/assets/3549b555-7215-41cd-af95-3d1372ef3799" />

## 💡 Motivation

The idea for EasyFPS was born from the simple fact that the Windows Game Bar and other overlay tools can be heavy, cumbersome, and annoying to use. I often found myself frustrated by having to open complex software filled with hundreds of unnecessary options just to see my framerate.

I wanted a true **plug-and-play** solution: something that requires zero configuration, no setup, and just works the moment you launch it. That is the core philosophy behind this project.

## 🚀 Features

* **Portable & Self-Contained:** A single `.exe` file. **PresentMon** is embedded directly into the binary and automatically managed. No extra installations required.
* **Extremely Lightweight:** Written in Rust and optimized for performance.
* **Accurate Monitoring:** Powered by Intel's PresentMon for precise FPS and 1% Low metrics.
* **Safe & Anti-Cheat Friendly:** Reads performance telemetry passively without injecting code or hooking into the game process.
* **Native Interface:** Uses native generic Windows APIs (Win32 GDI/DWM) for a clean, non-intrusive overlay.
* **Multi-Monitor Aware:** Detects the monitor the game is running on and places the overlay on the correct screen.
* **System Tray Integration:** Runs quietly in the background, accessible completely via the system tray.

## 📋 Requirements

* **Operating System:** Windows 10 or Windows 11.
* **Privileges:** **Run as Administrator** is recommended (and often required) to ensure PresentMon can access performance data for all processes.

## 🛠️ Build Instructions

If you want to build the source code yourself (instead of downloading a release):

1.  **Install Rust:** `winget install Rustlang.Rustup`
2.  **Clone the repo:** `git clone ...`
3.  **Build:**
    ```bash
    cargo build --release
    ```
4.  **Run:** The executable will be in `./target/release/easyfps.exe`.
    * *Note: The build process automatically embeds the included `PresentMon.exe` into the final binary.*

### Development notes

* **Admin manifest:** Only **release** builds embed the `requireAdministrator` manifest. Debug builds (`cargo build`, `cargo test`) run without forced elevation so the test harness can launch normally. Set `EASYFPS_FORCE_MANIFEST=1` to embed it in debug builds too.
* **Tests & lints:** `cargo test`, `cargo clippy --all-targets -- -D warnings`, and `cargo fmt --all -- --check` are run in CI (see `.github/workflows/ci.yml`).
* **Debug logging:** Logging is opt-in. Set the `EASYFPS_DEBUG` environment variable to write diagnostics to `%LOCALAPPDATA%\EasyFPS\debug.log`.
* **PresentMon:** Built against the embedded Intel PresentMon binary; the CSV parser tolerates the column-name differences across PresentMon versions.

## 🎮 Usage

1.  Run `easyfps.exe` (Run as Administrator recommended).
2.  The icon will appear in the System Tray.
3.  Launch any fullscreen game: the FPS counter will automatically appear overlaying the game.
4.  Right-click the tray icon to access Settings or Exit.

## ⚙️ Tech Stack

* **Language:** Rust 🦀
* **Core:** `windows-rs` for OS interaction.
* **Backend:** Embedded **Intel PresentMon** subprocess for telemetry.
* **GUI:** Native Win32 API.

## 📝 License

This project is distributed under the MIT License. See the LICENSE file for details.

---
*Author: Matteo842*
