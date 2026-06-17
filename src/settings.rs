use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

/// Overlay position on screen
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum OverlayPosition {
    #[default]
    TopRight,
    TopLeft,
}

/// FPS text color
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum FpsColor {
    #[default]
    White,
    Green, // Bright green #39FF14
}

impl FpsColor {
    /// Get RGB values for this color
    pub fn to_rgb(self) -> (u8, u8, u8) {
        match self {
            FpsColor::White => (255, 255, 255),
            FpsColor::Green => (57, 255, 20), // #39FF14
        }
    }
}

/// Overlay size
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum OverlaySize {
    Small,
    #[default]
    Medium,
    Large,
}

impl OverlaySize {
    /// Get dimensions (width, height, font_large, font_small)
    pub fn dimensions(&self) -> (i32, i32, i32, i32) {
        match self {
            OverlaySize::Small => (75, 42, 20, 10),
            OverlaySize::Medium => (95, 52, 26, 12),
            OverlaySize::Large => (120, 65, 32, 14),
        }
    }
}

/// Application settings
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Settings {
    /// Overlay position (top-right or top-left)
    pub position: OverlayPosition,

    /// FPS text color
    pub fps_color: FpsColor,

    /// Overlay size
    pub size: OverlaySize,

    /// Start with Windows
    pub start_with_windows: bool,

    /// Show 1% low FPS
    pub show_1_percent_low: bool,

    /// Show CPU Usage
    pub show_cpu_usage: bool,

    /// Show GPU Usage
    pub show_gpu_usage: bool,

    /// Overlay Opacity (40-100)
    pub overlay_opacity: u8,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            position: OverlayPosition::TopRight,
            fps_color: FpsColor::White,
            size: OverlaySize::Medium,
            start_with_windows: false,
            show_1_percent_low: true,
            show_cpu_usage: false,
            show_gpu_usage: false,
            overlay_opacity: 90,
        }
    }
}

impl Settings {
    /// Get the config file path
    fn config_path() -> PathBuf {
        dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("EasyFPS")
            .join("settings.json")
    }

    /// Load settings from disk, or return defaults
    pub fn load() -> Self {
        let path = Self::config_path();

        if path.exists() {
            if let Ok(content) = fs::read_to_string(&path) {
                if let Ok(settings) = serde_json::from_str(&content) {
                    return settings;
                }
            }
        }

        Self::default()
    }

    /// Save settings to disk
    pub fn save(&self) -> Result<(), String> {
        let path = Self::config_path();

        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .map_err(|e| format!("Failed to create config directory: {}", e))?;
        }

        let content = serde_json::to_string_pretty(self)
            .map_err(|e| format!("Failed to serialize settings: {}", e))?;

        fs::write(&path, content).map_err(|e| format!("Failed to write settings file: {}", e))?;

        Ok(())
    }

    /// Set or remove the Windows startup registry entry.
    /// Uses the Win32 registry API directly (no `reg.exe` subprocess), which is
    /// faster, spawns no console window, and reports failures precisely.
    pub fn set_startup_registry(&self) -> Result<(), String> {
        use windows::core::w;
        use windows::Win32::System::Registry::{
            RegCloseKey, RegDeleteValueW, RegOpenKeyExW, RegSetValueExW, HKEY, HKEY_CURRENT_USER,
            KEY_SET_VALUE, REG_SZ,
        };

        let exe_path =
            std::env::current_exe().map_err(|e| format!("Failed to get exe path: {}", e))?;

        let subkey = w!("Software\\Microsoft\\Windows\\CurrentVersion\\Run");
        let value_name = w!("EasyFPS");

        unsafe {
            let mut hkey = HKEY::default();
            RegOpenKeyExW(HKEY_CURRENT_USER, subkey, None, KEY_SET_VALUE, &mut hkey)
                .ok()
                .map_err(|e| format!("Failed to open Run key: {}", e))?;

            let result = if self.start_with_windows {
                // REG_SZ data must be a null-terminated UTF-16 string.
                let wide: Vec<u16> = exe_path
                    .to_string_lossy()
                    .encode_utf16()
                    .chain(std::iter::once(0))
                    .collect();
                let bytes = std::slice::from_raw_parts(
                    wide.as_ptr() as *const u8,
                    wide.len() * std::mem::size_of::<u16>(),
                );
                RegSetValueExW(hkey, value_name, None, REG_SZ, Some(bytes))
                    .ok()
                    .map_err(|e| format!("Failed to set Run value: {}", e))
            } else {
                // Removing a value that does not exist is not an error for us.
                let _ = RegDeleteValueW(hkey, value_name);
                Ok(())
            };

            let _ = RegCloseKey(hkey);
            result
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_sane() {
        let s = Settings::default();
        assert_eq!(s.position, OverlayPosition::TopRight);
        assert_eq!(s.size, OverlaySize::Medium);
        assert!(s.show_1_percent_low);
        assert_eq!(s.overlay_opacity, 90);
    }

    #[test]
    fn color_rgb_values() {
        assert_eq!(FpsColor::White.to_rgb(), (255, 255, 255));
        assert_eq!(FpsColor::Green.to_rgb(), (57, 255, 20));
    }

    #[test]
    fn sizes_increase_monotonically() {
        let (_, _, small, _) = OverlaySize::Small.dimensions();
        let (_, _, medium, _) = OverlaySize::Medium.dimensions();
        let (_, _, large, _) = OverlaySize::Large.dimensions();
        assert!(small < medium);
        assert!(medium < large);
    }

    #[test]
    fn settings_roundtrip_json() {
        let s = Settings {
            position: OverlayPosition::TopLeft,
            fps_color: FpsColor::Green,
            size: OverlaySize::Large,
            start_with_windows: true,
            show_1_percent_low: false,
            show_cpu_usage: true,
            show_gpu_usage: true,
            overlay_opacity: 75,
        };
        let json = serde_json::to_string(&s).unwrap();
        let back: Settings = serde_json::from_str(&json).unwrap();
        assert_eq!(back.position, s.position);
        assert_eq!(back.fps_color, s.fps_color);
        assert_eq!(back.size, s.size);
        assert_eq!(back.overlay_opacity, 75);
        assert!(back.show_cpu_usage);
    }
}
