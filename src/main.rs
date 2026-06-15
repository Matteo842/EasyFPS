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
    DispatchMessageW, MsgWaitForMultipleObjectsEx, PeekMessageW, TranslateMessage, MSG,
    MWMO_INPUTAVAILABLE, PM_REMOVE, QS_ALLINPUT, WM_QUIT,
};

/// How long the game can stay out of the foreground before we tear down the
/// PresentMon capture to free resources (in main-loop ticks of ~16ms).
const IDLE_TICKS_BEFORE_STOP: u32 = 125; // ~2 seconds

fn main() {
    // Emergency handler for Ctrl+C or terminal close.
    // Ensures the ETW session is cleaned up if the program is killed.
    ctrlc::set_handler(move || {
        fps_capture::shutdown();
        overlay::shutdown();
        tray::shutdown();
        std::process::exit(0);
    })
    .expect("Error setting Ctrl-C handler");

    // Load settings
    let settings = Arc::new(Mutex::new(Settings::load()));

    // Initialize tray first (needs to be on main thread)
    if let Err(e) = tray::init() {
        show_error_message(&format!("Tray initialization error: {}", e));
        return;
    }

    // Initialize overlay
    if let Err(e) = overlay::init() {
        show_error_message(&format!("Overlay initialization error: {}", e));
        return;
    }

    // Initialize FPS capture
    if let Err(e) = fps_capture::init() {
        // If it fails (e.g. no admin), show error but try to continue
        show_error_message(&format!(
            "FPS initialization error (Admin required?): {}",
            e
        ));
    }

    // Clone settings for the callback
    let settings_for_callback = Arc::clone(&settings);

    // Initialize System Monitor
    let mut sys_monitor = monitor::SystemMonitor::new();
    let mut last_stats_update = Instant::now();

    let mut last_update = Instant::now();
    let mut idle_ticks: u32 = 0;

    // Main message loop
    loop {
        // Block until a message arrives or ~16ms elapses, instead of busy
        // spinning. This keeps idle CPU usage near zero while still updating
        // the overlay at ~60Hz.
        unsafe {
            MsgWaitForMultipleObjectsEx(None, 16, QS_ALLINPUT, MWMO_INPUTAVAILABLE);

            let mut msg = MSG::default();
            while PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE).as_bool() {
                if msg.message == WM_QUIT {
                    break;
                }
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }

        // Check for tray menu events
        if let Some(menu_id) = tray::check_menu_event() {
            match menu_id.as_str() {
                tray::MENU_SETTINGS if !gui::is_open() => {
                    let current_settings = settings.lock().clone();
                    let settings_clone = Arc::clone(&settings_for_callback);

                    gui::open(current_settings, move |new_settings| {
                        let mut s = settings_clone.lock();
                        *s = new_settings;
                    });
                }
                tray::MENU_EXIT => {
                    break;
                }
                _ => {}
            }
        }

        // Update overlay every ~16ms (~60 updates per second for smooth UI)
        if last_update.elapsed() >= Duration::from_millis(16) {
            last_update = Instant::now();

            let current_settings = settings.lock().clone();

            // Update stats every 1 second
            if last_stats_update.elapsed() >= Duration::from_millis(1000) {
                sys_monitor.update(
                    current_settings.show_cpu_usage,
                    current_settings.show_gpu_usage,
                );
                last_stats_update = Instant::now();
            }

            // Check for fullscreen app
            if let Some(app) = fullscreen::get_fullscreen_app() {
                idle_ticks = 0;

                // Get FPS for the fullscreen app
                let fps_data = fps_capture::get_fps_for_process(app.process_id);

                let (fps, one_percent_low) = match fps_data {
                    Some(data) => (data.fps, data.one_percent_low),
                    None => (0.0, 0.0),
                };

                // Show overlay with FPS and Stats on the game's monitor
                overlay::show(
                    fps,
                    one_percent_low,
                    sys_monitor.get_cpu_usage(),
                    sys_monitor.get_gpu_usage(),
                    app.monitor,
                    &current_settings,
                );
            } else {
                // No fullscreen app, hide overlay
                overlay::hide();

                // After a grace period with no game, stop PresentMon so we
                // stop consuming resources in the background.
                if idle_ticks < IDLE_TICKS_BEFORE_STOP {
                    idle_ticks += 1;
                    if idle_ticks == IDLE_TICKS_BEFORE_STOP {
                        fps_capture::set_target_process(0);
                    }
                }
            }
        }

        // Small sleep to prevent 100% CPU usage
        std::thread::sleep(Duration::from_millis(2));
    }

    // Cleanup when loop ends
    fps_capture::shutdown();
    overlay::shutdown();
    tray::shutdown();
}

fn show_error_message(message: &str) {
    use windows::core::PCWSTR;
    use windows::Win32::UI::WindowsAndMessaging::{MessageBoxW, MB_ICONERROR, MB_OK};

    let msg: Vec<u16> = message.encode_utf16().chain(std::iter::once(0)).collect();
    let title: Vec<u16> = "EasyFPS Error"
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();

    unsafe {
        MessageBoxW(
            None,
            PCWSTR(msg.as_ptr()),
            PCWSTR(title.as_ptr()),
            MB_OK | MB_ICONERROR,
        );
    }
}
