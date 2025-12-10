# EasyFPS

**EasyFPS** is a lightweight, minimalist FPS (Frame Per Second) counter designed for fullscreen games on Windows.

Unlike many overlays that inject code into game processes (risking detection by anti-cheat software), EasyFPS leverages **ETW (Event Tracing for Windows)** to monitor performance passively and securely, with near-zero impact on system resources.

<img width="1280" height="720" alt="EasyFPS-commercial" src="https://github.com/user-attachments/assets/3549b555-7215-41cd-af95-3d1372ef3799" />

## 🚀 Features

* **Extremely Lightweight:** Written in Rust and optimized for performance (`lto` enabled, stripped binary).
* **Safe & Anti-Cheat Friendly:** Uses `ferrisetw` to read data directly from the Windows kernel without hooking into the game process.
* **Native Interface:** No heavy web frameworks. It uses native Windows APIs (Win32 GDI/DWM) for rendering.
* **System Tray Integration:** Runs quietly in the background, accessible via the system tray icon.
* **Configurable:** Settings are saved and loaded automatically.

## 📋 Requirements

* **Operating System:** Windows 10 or Windows 11.
* **Privileges:** Requires **Run as Administrator**.
    * *Technical Note:* Administrator privileges are mandatory because the software needs to access system-wide ETW sessions to calculate global FPS.

## 🛠️ Build Instructions

If you do not have a pre-compiled executable, you can build the source code yourself.

### Prerequisites
Ensure you have **Rust** installed. If not, open PowerShell and run:
`winget install Rustlang.Rustup`

### Compilation

1.  Open your terminal in the project folder.
2.  Run the build command for the optimized version:
    ```bash
    cargo build --release
    ```
3.  You will find the executable `easyfps.exe` in:
    `./target/release/easyfps.exe`

## 🎮 Usage

1.  Run `easyfps.exe` (accept the UAC prompt for Admin privileges).
2.  The icon will appear in the System Tray (bottom right, near the clock).
3.  Launch a fullscreen game: the FPS counter should automatically appear as an overlay.

## ⚙️ Project Structure

* **Core:** Built on `windows-rs` for OS interaction.
* **Monitoring:** Uses `ferrisetw` for event capturing.
* **Configuration:** Data is serialized using `serde` and `serde_json`.

## 📝 License

This project is distributed under the MIT License. See the LICENSE file for details.

---
*Author: Matteo842**
