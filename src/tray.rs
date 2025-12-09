use tray_icon::{
    menu::{Menu, MenuEvent, MenuItem},
    TrayIcon, TrayIconBuilder, TrayIconEvent,
    Icon, MouseButton, MouseButtonState,
};
use std::time::Instant;
use parking_lot::Mutex;

/// Tray menu item IDs
pub const MENU_SETTINGS: &str = "settings";
pub const MENU_EXIT: &str = "exit";

/// Global tray icon (must stay on main thread)
static mut TRAY_ICON: Option<TrayIcon> = None;

/// Track last click for double-click detection
static LAST_CLICK: once_cell::sync::Lazy<Mutex<Option<Instant>>> = 
    once_cell::sync::Lazy::new(|| Mutex::new(None));

/// Create a green icon for the tray (32x32 RGBA)
fn create_green_icon() -> Icon {
    const SIZE: usize = 32;
    let mut rgba = vec![0u8; SIZE * SIZE * 4];
    
    // Create a green circle with "F" letter
    for y in 0..SIZE {
        for x in 0..SIZE {
            let idx = (y * SIZE + x) * 4;
            let cx = SIZE as f32 / 2.0;
            let cy = SIZE as f32 / 2.0;
            let dist = ((x as f32 - cx).powi(2) + (y as f32 - cy).powi(2)).sqrt();
            let radius = SIZE as f32 / 2.0 - 2.0;
            
            if dist <= radius {
                // Green circle (#39FF14)
                rgba[idx] = 57;      // R
                rgba[idx + 1] = 255; // G
                rgba[idx + 2] = 20;  // B
                rgba[idx + 3] = 255; // A
                
                // Draw "F" in black
                let in_f = (x >= 10 && x <= 13 && y >= 8 && y <= 24) || // Vertical bar
                          (x >= 10 && x <= 22 && y >= 8 && y <= 11) ||  // Top horizontal
                          (x >= 10 && x <= 19 && y >= 14 && y <= 17);   // Middle horizontal
                
                if in_f {
                    rgba[idx] = 0;      // R
                    rgba[idx + 1] = 0;  // G
                    rgba[idx + 2] = 0;  // B
                }
            } else {
                // Transparent
                rgba[idx] = 0;
                rgba[idx + 1] = 0;
                rgba[idx + 2] = 0;
                rgba[idx + 3] = 0;
            }
        }
    }
    
    Icon::from_rgba(rgba, SIZE as u32, SIZE as u32).expect("Failed to create icon")
}

/// Initialize the system tray (must be called from main thread)
pub fn init() -> Result<(), String> {
    // Create menu
    let menu = Menu::new();
    
    let settings_item = MenuItem::with_id(MENU_SETTINGS, "Impostazioni", true, None);
    let exit_item = MenuItem::with_id(MENU_EXIT, "Esci", true, None);
    
    menu.append(&settings_item).map_err(|e| format!("Failed to add menu item: {}", e))?;
    menu.append(&exit_item).map_err(|e| format!("Failed to add menu item: {}", e))?;
    
    // Create tray icon
    let icon = create_green_icon();
    
    let tray_icon = TrayIconBuilder::new()
        .with_menu(Box::new(menu))
        .with_tooltip("EasyFPS - Doppio click per impostazioni")
        .with_icon(icon)
        .build()
        .map_err(|e| format!("Failed to create tray icon: {}", e))?;
    
    // Store tray icon
    unsafe {
        TRAY_ICON = Some(tray_icon);
    }
    
    Ok(())
}

/// Check for menu events (non-blocking)
pub fn check_menu_event() -> Option<String> {
    // Check menu events first (right-click menu)
    if let Ok(event) = MenuEvent::receiver().try_recv() {
        return Some(event.id.0.clone());
    }
    
    // Check tray icon click events for double-click detection
    if let Ok(event) = TrayIconEvent::receiver().try_recv() {
        if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = event {
            let now = Instant::now();
            let mut last = LAST_CLICK.lock();
            
            if let Some(last_time) = *last {
                // Check if double click (within 400ms)
                if now.duration_since(last_time).as_millis() < 400 {
                    *last = None;
                    return Some(MENU_SETTINGS.to_string());
                }
            }
            *last = Some(now);
        }
    }
    
    None
}

/// Shutdown the tray
pub fn shutdown() {
    unsafe {
        TRAY_ICON = None;
    }
}
