#![windows_subsystem = "windows"]

mod fps_capture;
mod fullscreen;
mod gui;
mod monitor;
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
    // <<< NUOVO: Gestore di emergenza per Ctrl+C o chiusura terminale
    // Questo impedisce che la sessione ETW rimanga attiva se il programma viene ucciso
    ctrlc::set_handler(move || {
        // Non usiamo println! qui perché in modalità GUI non si vede, 
        // ma puliamo le risorse critiche.
        fps_capture::shutdown();
        overlay::shutdown();
        tray::shutdown();
        std::process::exit(0);
    }).expect("Error setting Ctrl-C handler");

    // Load settings
    let settings = Arc::new(Mutex::new(Settings::load()));
    
    // Initialize tray first (needs to be on main thread)
    if let Err(e) = tray::init() {
        show_error_message(&format!("Errore inizializzazione tray: {}", e));
        return;
    }
    
    // Initialize overlay
    if let Err(e) = overlay::init() {
        show_error_message(&format!("Errore inizializzazione overlay: {}", e));
        return;
    }
    
    // Initialize FPS capture
    if let Err(e) = fps_capture::init() {
        // Se fallisce (es. no admin), mostriamo errore ma proviamo a continuare
        show_error_message(&format!("Errore inizializzazione FPS (Admin richiesto?): {}", e));
    }
    
    // Clone settings for the callback
    let settings_for_callback = Arc::clone(&settings);
    
    // Initialize System Monitor
    let mut sys_monitor = monitor::SystemMonitor::new();
    let mut last_stats_update = Instant::now();

    let mut last_update = Instant::now();
    
    // Main message loop
    loop {
        // Process Windows messages (required for tray icon to work)
        unsafe {
            let mut msg = MSG::default();
            // PeekMessage non blocca, permette al loop di girare
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
                    // L'utente ha cliccato Exit, usciamo dal loop pulitamente
                    break; 
                }
                _ => {}
            }
        }
        
        // Update overlay every ~16ms (circa 60 update al secondo per l'UI)
        if last_update.elapsed() >= Duration::from_millis(16) {
            last_update = Instant::now();
            
            let current_settings = settings.lock().clone();
            
            // Update stats every 1 second
            if last_stats_update.elapsed() >= Duration::from_millis(1000) {
                sys_monitor.update(current_settings.show_cpu_usage, current_settings.show_gpu_usage);
                last_stats_update = Instant::now();
            }

            // Check for fullscreen app
            if let Some(app) = fullscreen::get_fullscreen_app() {
                // Get FPS for the fullscreen app
                // Qui chiamiamo la funzione che abbiamo sistemato in fps_capture.rs
                let fps_data = fps_capture::get_fps_for_process(app.process_id);
                
                let (fps, one_percent_low) = match fps_data {
                    Some(data) => (data.fps, data.one_percent_low),
                    None => (0.0, 0.0), // Se non abbiamo dati (ancora), mostriamo 0
                };
                
                // Show overlay with FPS and Stats
                overlay::show(
                    fps, 
                    one_percent_low, 
                    sys_monitor.get_cpu_usage(), 
                    sys_monitor.get_gpu_usage(), 
                    &current_settings
                );
            } else {
                // No fullscreen app, hide overlay
                overlay::hide();
            }
        }
        
        // Small sleep to prevent 100% CPU usage
        // Importante: non dormire troppo o l'overlay lagga
        std::thread::sleep(Duration::from_millis(2)); 
    }
    
    // <<< PULIZIA FINALE: Questa parte viene eseguita quando il loop finisce (Break)
    fps_capture::shutdown(); // Spegni ETW
    overlay::shutdown();     // Spegni Overlay DX11
    tray::shutdown();        // Rimuovi icona
}

fn show_error_message(message: &str) {
    use windows::Win32::UI::WindowsAndMessaging::{MessageBoxW, MB_OK, MB_ICONERROR};
    use windows::core::PCWSTR;
    
    // Converti stringa Rust in stringa Wide (Windows Unicode)
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