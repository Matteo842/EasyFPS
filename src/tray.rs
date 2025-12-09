use tray_icon::{
    menu::{Menu, MenuEvent, MenuItem},
    TrayIcon, TrayIconBuilder, TrayIconEvent,
    Icon, MouseButton, MouseButtonState,
};
use std::time::Instant;
use std::sync::atomic::{AtomicU64, Ordering};

pub const MENU_SETTINGS: &str = "settings";
pub const MENU_EXIT: &str = "exit";

static mut TRAY_ICON: Option<TrayIcon> = None;

// Store last click time as u64 millis since app start
static LAST_CLICK_MS: AtomicU64 = AtomicU64::new(0);
static APP_START: once_cell::sync::Lazy<Instant> = once_cell::sync::Lazy::new(|| Instant::now());

fn create_green_icon() -> Icon {
    const SIZE: usize = 32;
    let mut rgba = vec![0u8; SIZE * SIZE * 4];
    
    for y in 0..SIZE {
        for x in 0..SIZE {
            let idx = (y * SIZE + x) * 4;
            let cx = SIZE as f32 / 2.0;
            let cy = SIZE as f32 / 2.0;
            let dist = ((x as f32 - cx).powi(2) + (y as f32 - cy).powi(2)).sqrt();
            let radius = SIZE as f32 / 2.0 - 2.0;
            
            if dist <= radius {
                rgba[idx] = 57;
                rgba[idx + 1] = 255;
                rgba[idx + 2] = 20;
                rgba[idx + 3] = 255;
                
                let in_f = (x >= 10 && x <= 13 && y >= 8 && y <= 24) ||
                          (x >= 10 && x <= 22 && y >= 8 && y <= 11) ||
                          (x >= 10 && x <= 19 && y >= 14 && y <= 17);
                
                if in_f {
                    rgba[idx] = 0;
                    rgba[idx + 1] = 0;
                    rgba[idx + 2] = 0;
                }
            }
        }
    }
    
    Icon::from_rgba(rgba, SIZE as u32, SIZE as u32).expect("Failed to create icon")
}

pub fn init() -> Result<(), String> {
    let menu = Menu::new();
    
    let settings_item = MenuItem::with_id(MENU_SETTINGS, "Impostazioni", true, None);
    let exit_item = MenuItem::with_id(MENU_EXIT, "Esci", true, None);
    
    menu.append(&settings_item).map_err(|e| format!("{}", e))?;
    menu.append(&exit_item).map_err(|e| format!("{}", e))?;
    
    let icon = create_green_icon();
    
    let tray_icon = TrayIconBuilder::new()
        .with_menu(Box::new(menu))
        .with_tooltip("EasyFPS - Doppio click = Impostazioni")
        .with_icon(icon)
        .build()
        .map_err(|e| format!("{}", e))?;
    
    unsafe {
        TRAY_ICON = Some(tray_icon);
    }
    
    // Initialize app start time
    let _ = *APP_START;
    
    Ok(())
}

pub fn check_menu_event() -> Option<String> {
    // Menu events (right-click menu)
    if let Ok(event) = MenuEvent::receiver().try_recv() {
        return Some(event.id.0.clone());
    }
    
    // Tray icon click events
    if let Ok(event) = TrayIconEvent::receiver().try_recv() {
        match event {
            TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } => {
                let now_ms = APP_START.elapsed().as_millis() as u64;
                let last_ms = LAST_CLICK_MS.swap(now_ms, Ordering::SeqCst);
                
                // Double click if within 500ms
                if now_ms.saturating_sub(last_ms) < 500 {
                    LAST_CLICK_MS.store(0, Ordering::SeqCst); // Reset
                    return Some(MENU_SETTINGS.to_string());
                }
            }
            _ => {}
        }
    }
    
    None
}

pub fn shutdown() {
    unsafe {
        TRAY_ICON = None;
    }
}
