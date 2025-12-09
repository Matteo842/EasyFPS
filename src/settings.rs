use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

/// Overlay position on screen
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum OverlayPosition {
    TopRight,
    TopLeft,
}

impl Default for OverlayPosition {
    fn default() -> Self {
        Self::TopRight
    }
}

/// FPS text color
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FpsColor {
    White,
    Green, // Bright green #39FF14
}

impl Default for FpsColor {
    fn default() -> Self {
        Self::White
    }
}

impl FpsColor {
    /// Get RGB values for this color
    pub fn to_rgb(&self) -> (u8, u8, u8) {
        match self {
            FpsColor::White => (255, 255, 255),
            FpsColor::Green => (57, 255, 20), // #39FF14
        }
    }
}

/// Overlay size
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum OverlaySize {
    Small,
    Medium,
    Large,
}

impl Default for OverlaySize {
    fn default() -> Self {
        Self::Medium
    }
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
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            position: OverlayPosition::TopRight,
            fps_color: FpsColor::White,
            size: OverlaySize::Medium,
            start_with_windows: false,
            show_1_percent_low: true,
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
            match fs::read_to_string(&path) {
                Ok(content) => {
                    match serde_json::from_str(&content) {
                        Ok(settings) => return settings,
                        Err(_) => {}
                    }
                }
                Err(_) => {}
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
        
        fs::write(&path, content)
            .map_err(|e| format!("Failed to write settings file: {}", e))?;
        
        Ok(())
    }
    
    /// Set or remove the Windows startup registry entry
    pub fn set_startup_registry(&self) -> Result<(), String> {
        use std::process::Command;
        
        let exe_path = std::env::current_exe()
            .map_err(|e| format!("Failed to get exe path: {}", e))?;
        let exe_path_str = exe_path.to_string_lossy();
        
        if self.start_with_windows {
            let output = Command::new("reg")
                .args([
                    "add",
                    r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run",
                    "/v", "EasyFPS",
                    "/t", "REG_SZ",
                    "/d", &format!("\"{}\"", exe_path_str),
                    "/f"
                ])
                .output()
                .map_err(|e| format!("Failed to run reg command: {}", e))?;
            
            if !output.status.success() {
                return Err("Failed to add registry entry".to_string());
            }
        } else {
            let _ = Command::new("reg")
                .args([
                    "delete",
                    r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run",
                    "/v", "EasyFPS",
                    "/f"
                ])
                .output();
        }
        
        Ok(())
    }
}
