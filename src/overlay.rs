use crate::settings::{FpsColor, OverlayPosition, Settings};
use parking_lot::Mutex;
use std::sync::atomic::{AtomicBool, AtomicIsize, Ordering};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::Graphics::Gdi::{
    BeginPaint, CreateFontW, CreateSolidBrush, DeleteObject, EndPaint,
    InvalidateRect, SelectObject, SetBkMode, SetTextColor, TextOutW, HBRUSH,
    PAINTSTRUCT, TRANSPARENT, RoundRect, CreatePen, PS_SOLID,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DispatchMessageW, GetSystemMetrics,
    PeekMessageW, PostQuitMessage, RegisterClassW, SetLayeredWindowAttributes,
    SetWindowPos, ShowWindow, TranslateMessage, HWND_TOPMOST, LWA_ALPHA,
    MSG, PM_REMOVE, SM_CXSCREEN, SM_CYSCREEN, SWP_NOSIZE, SW_HIDE, SW_SHOWNOACTIVATE,
    WM_DESTROY, WM_PAINT, WNDCLASSW, WS_EX_LAYERED, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW,
    WS_EX_TOPMOST, WS_EX_TRANSPARENT, WS_POPUP,
};

// Overlay dimensions - more compact
const OVERLAY_WIDTH: i32 = 85;
const OVERLAY_HEIGHT: i32 = 48;
const OVERLAY_MARGIN: i32 = 10;
const BACKGROUND_COLOR: u32 = 0x1A1A1A; // Dark gray #1A1A1A
const BORDER_RADIUS: i32 = 6;

/// Overlay display data (thread-safe)
struct OverlayData {
    current_fps: f64,
    one_percent_low: f64,
    position: OverlayPosition,
    fps_color: FpsColor,
    show_1_percent_low: bool,
}

// Global state using atomics for thread safety
static OVERLAY_HWND: AtomicIsize = AtomicIsize::new(0);
static OVERLAY_VISIBLE: AtomicBool = AtomicBool::new(false);
static OVERLAY_DATA: once_cell::sync::Lazy<Mutex<OverlayData>> =
    once_cell::sync::Lazy::new(|| Mutex::new(OverlayData {
        current_fps: 0.0,
        one_percent_low: 0.0,
        position: OverlayPosition::TopRight,
        fps_color: FpsColor::White,
        show_1_percent_low: true,
    }));

/// Initialize the overlay window
pub fn init() -> Result<(), String> {
    std::thread::spawn(move || {
        if let Err(e) = run_overlay_window() {
            eprintln!("Overlay error: {}", e);
        }
    });
    
    // Wait for window to be created
    std::thread::sleep(std::time::Duration::from_millis(100));
    
    Ok(())
}

/// Show the overlay with FPS value
pub fn show(fps: f64, one_percent_low: f64, settings: &Settings) {
    // Update data
    {
        let mut data = OVERLAY_DATA.lock();
        data.current_fps = fps;
        data.one_percent_low = one_percent_low;
        data.position = settings.position;
        data.fps_color = settings.fps_color;
        data.show_1_percent_low = settings.show_1_percent_low;
    }
    
    let hwnd_val = OVERLAY_HWND.load(Ordering::SeqCst);
    if hwnd_val != 0 {
        let hwnd = HWND(hwnd_val as *mut std::ffi::c_void);
        
        if !OVERLAY_VISIBLE.load(Ordering::SeqCst) {
            OVERLAY_VISIBLE.store(true, Ordering::SeqCst);
            unsafe {
                let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
                update_position(hwnd, settings.position);
            }
        }
        
        // Trigger repaint
        unsafe {
            let _ = InvalidateRect(hwnd, None, true);
        }
    }
}

/// Hide the overlay
pub fn hide() {
    if OVERLAY_VISIBLE.load(Ordering::SeqCst) {
        OVERLAY_VISIBLE.store(false, Ordering::SeqCst);
        let hwnd_val = OVERLAY_HWND.load(Ordering::SeqCst);
        if hwnd_val != 0 {
            let hwnd = HWND(hwnd_val as *mut std::ffi::c_void);
            unsafe {
                let _ = ShowWindow(hwnd, SW_HIDE);
            }
        }
    }
}

/// Update overlay position based on settings
fn update_position(hwnd: HWND, position: OverlayPosition) {
    unsafe {
        let screen_width = GetSystemMetrics(SM_CXSCREEN);
        let _screen_height = GetSystemMetrics(SM_CYSCREEN);
        
        let (x, y) = match position {
            OverlayPosition::TopRight => (screen_width - OVERLAY_WIDTH - OVERLAY_MARGIN, OVERLAY_MARGIN),
            OverlayPosition::TopLeft => (OVERLAY_MARGIN, OVERLAY_MARGIN),
        };
        
        let _ = SetWindowPos(
            hwnd,
            HWND_TOPMOST,
            x,
            y,
            OVERLAY_WIDTH,
            OVERLAY_HEIGHT,
            SWP_NOSIZE,
        );
    }
}

/// Window procedure for the overlay
unsafe extern "system" fn overlay_wndproc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match msg {
        WM_PAINT => {
            let mut ps = PAINTSTRUCT::default();
            let hdc = BeginPaint(hwnd, &mut ps);
            
            // Get current data
            let data = OVERLAY_DATA.lock();
            
            // Fill background with dark gray (rounded rectangle effect)
            let brush = CreateSolidBrush(windows::Win32::Foundation::COLORREF(BACKGROUND_COLOR));
            let pen = CreatePen(PS_SOLID, 1, windows::Win32::Foundation::COLORREF(BACKGROUND_COLOR));
            let old_brush = SelectObject(hdc, brush);
            let old_pen = SelectObject(hdc, pen);
            
            let _ = RoundRect(hdc, 0, 0, OVERLAY_WIDTH, OVERLAY_HEIGHT, BORDER_RADIUS, BORDER_RADIUS);
            
            SelectObject(hdc, old_brush);
            SelectObject(hdc, old_pen);
            let _ = DeleteObject(brush);
            let _ = DeleteObject(pen);
            
            // Set text properties
            let _ = SetBkMode(hdc, TRANSPARENT);
            let (r, g, b) = data.fps_color.to_rgb();
            SetTextColor(hdc, windows::Win32::Foundation::COLORREF(
                (b as u32) << 16 | (g as u32) << 8 | (r as u32)
            ));
            
            // Create font for FPS number (large, bold)
            let font_large = CreateFontW(
                24, 0, 0, 0, 700, // Height, width, escapement, orientation, weight (bold)
                0, 0, 0, // Italic, underline, strikeout
                0, 0, 0, 0, 0, // Charset, precision, clip, quality, pitch
                windows::core::w!("Segoe UI"),
            );
            
            let old_font = SelectObject(hdc, font_large);
            
            // Draw FPS number
            let fps_text = format!("{:.0}", data.current_fps);
            let fps_wide: Vec<u16> = fps_text.encode_utf16().collect();
            let _ = TextOutW(hdc, 8, 2, &fps_wide);
            
            // Draw "FPS" label smaller, next to number
            let font_small = CreateFontW(
                11, 0, 0, 0, 400,
                0, 0, 0,
                0, 0, 0, 0, 0,
                windows::core::w!("Segoe UI"),
            );
            SelectObject(hdc, font_small);
            
            // Position FPS label based on number width
            let fps_label_x = if data.current_fps >= 100.0 { 52 } else if data.current_fps >= 10.0 { 40 } else { 28 };
            let fps_label: Vec<u16> = "FPS".encode_utf16().collect();
            let _ = TextOutW(hdc, fps_label_x, 6, &fps_label);
            
            // Draw 1% low if enabled
            if data.show_1_percent_low {
                SetTextColor(hdc, windows::Win32::Foundation::COLORREF(0x888888)); // Gray
                let low_text = format!("1%: {:.0}", data.one_percent_low);
                let low_wide: Vec<u16> = low_text.encode_utf16().collect();
                let _ = TextOutW(hdc, 8, 28, &low_wide);
            }
            
            SelectObject(hdc, old_font);
            let _ = DeleteObject(font_large);
            let _ = DeleteObject(font_small);
            
            drop(data);
            
            let _ = EndPaint(hwnd, &ps);
            LRESULT(0)
        }
        WM_DESTROY => {
            PostQuitMessage(0);
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

/// Run the overlay window message loop
fn run_overlay_window() -> Result<(), String> {
    unsafe {
        // Register window class
        let class_name = windows::core::w!("EasyFPS_Overlay");
        
        let wc = WNDCLASSW {
            lpfnWndProc: Some(overlay_wndproc),
            lpszClassName: class_name,
            hbrBackground: HBRUSH(std::ptr::null_mut()),
            ..Default::default()
        };
        
        if RegisterClassW(&wc) == 0 {
            return Err("Failed to register overlay window class".to_string());
        }
        
        // Create layered window
        let hwnd = CreateWindowExW(
            WS_EX_LAYERED | WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE | WS_EX_TRANSPARENT,
            class_name,
            windows::core::w!("EasyFPS Overlay"),
            WS_POPUP,
            0, 0,
            OVERLAY_WIDTH, OVERLAY_HEIGHT,
            None,
            None,
            None,
            None,
        ).map_err(|e| format!("Failed to create overlay window: {}", e))?;
        
        // Store hwnd as isize for thread-safe access
        OVERLAY_HWND.store(hwnd.0 as isize, Ordering::SeqCst);
        
        // Set window transparency (semi-transparent)
        SetLayeredWindowAttributes(hwnd, None, 230, LWA_ALPHA)
            .map_err(|e| format!("Failed to set layered window attributes: {}", e))?;
        
        // Message loop
        let mut msg = MSG::default();
        loop {
            while PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE).as_bool() {
                if msg.message == windows::Win32::UI::WindowsAndMessaging::WM_QUIT {
                    return Ok(());
                }
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
            
            std::thread::sleep(std::time::Duration::from_millis(16));
        }
    }
}

/// Shutdown the overlay
pub fn shutdown() {
    let hwnd_val = OVERLAY_HWND.load(Ordering::SeqCst);
    if hwnd_val != 0 {
        let hwnd = HWND(hwnd_val as *mut std::ffi::c_void);
        unsafe {
            let _ = windows::Win32::UI::WindowsAndMessaging::DestroyWindow(hwnd);
        }
        OVERLAY_HWND.store(0, Ordering::SeqCst);
    }
}
