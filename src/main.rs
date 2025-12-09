#![windows_subsystem = "windows"]

mod fps_capture;
mod fullscreen;
mod gui;
mod overlay;
mod settings;
mod tray;

use parking_lot::Mutex;
use settings::Settings;
use std::sync::Arc;
use std::time::{Duration, Instant};
use windows::Win32::UI::WindowsAndMessaging::{
    DispatchMessageW, PeekMessageW, TranslateMessage, MSG, PM_REMOVE,
};

fn main() {
    // Load settings
    let settings = Arc::new(Mutex::new(Settings::load()));
    
    // Initialize tray first (needs to be on main thread)
    if let Err(e) = tray::init() {
        eprintln!("Failed to initialize tray: {}", e);
        show_error_message(&format!("Errore inizializzazione tray: {}", e));
        return;
    }
    
    // Initialize overlay
    if let Err(e) = overlay::init() {
        eprintln!("Failed to initialize overlay: {}", e);
        show_error_message(&format!("Errore inizializzazione overlay: {}", e));
        return;
    }
    
    // Initialize FPS capture
    if let Err(e) = fps_capture::init() {
        eprintln!("Failed to initialize FPS capture: {}", e);
        // Continue anyway - might work without admin or show error
    }
    
    // Clone settings for the callback
    let settings_for_callback = Arc::clone(&settings);
    
    let mut last_update = Instant::now();
    
    // Main message loop
    loop {
        // Process Windows messages (required for tray icon to work)
        unsafe {
            let mut msg = MSG::default();
            while PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE).as_bool() {
                if msg.message == windows::Win32::UI::WindowsAndMessaging::WM_QUIT {
                    break;
                }
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }
        
        // Check for tray menu events
        if let Some(menu_id) = tray::check_menu_event() {
            match menu_id.as_str() {
                tray::MENU_SETTINGS => {
                    if !gui::is_open() {
                        let current_settings = settings.lock().clone();
                        let settings_clone = Arc::clone(&settings_for_callback);
                        
                        gui::open(current_settings, move |new_settings| {
                            let mut s = settings_clone.lock();
                            *s = new_settings;
                        });
                    }
                }
                tray::MENU_EXIT => {
                    break;
                }
                _ => {}
            }
        }
        
        // Update overlay every ~16ms
        if last_update.elapsed() >= Duration::from_millis(16) {
            last_update = Instant::now();
            
            // Check for fullscreen app
            let current_settings = settings.lock().clone();
            
            if let Some(app) = fullscreen::get_fullscreen_app() {
                // Get FPS for the fullscreen app
                let fps_data = fps_capture::get_fps_for_process(app.process_id);
                
                let (fps, one_percent_low) = match fps_data {
                    Some(data) => (data.fps, data.one_percent_low),
                    None => (0.0, 0.0),
                };
                
                // Show overlay with FPS
                overlay::show(fps, one_percent_low, &current_settings);
            } else {
                // No fullscreen app, hide overlay
                overlay::hide();
            }
        }
        
        // Small sleep to prevent 100% CPU
        std::thread::sleep(Duration::from_millis(1));
    }
    
    // Cleanup
    fps_capture::shutdown();
    overlay::shutdown();
    tray::shutdown();
}

fn show_error_message(message: &str) {
    use windows::Win32::UI::WindowsAndMessaging::{MessageBoxW, MB_OK, MB_ICONERROR};
    use windows::core::PCWSTR;
    
    let msg: Vec<u16> = message.encode_utf16().chain(std::iter::once(0)).collect();
    let title: Vec<u16> = "EasyFPS Error".encode_utf16().chain(std::iter::once(0)).collect();
    
    unsafe {
        MessageBoxW(
            None,
            PCWSTR(msg.as_ptr()),
            PCWSTR(title.as_ptr()),
            MB_OK | MB_ICONERROR,
        );
    }
}
