use crate::settings::{FpsColor, OverlayPosition, OverlaySize, Settings};
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
    MSG, PM_REMOVE, SM_CXSCREEN, SM_CYSCREEN, SWP_NOACTIVATE, SW_HIDE, SW_SHOWNOACTIVATE,
    WM_DESTROY, WM_PAINT, WNDCLASSW, WS_EX_LAYERED, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW,
    WS_EX_TOPMOST, WS_EX_TRANSPARENT, WS_POPUP,
};

const OVERLAY_MARGIN: i32 = 10;
const BACKGROUND_COLOR: u32 = 0x1A1A1A;
const BORDER_RADIUS: i32 = 6;

/// Overlay display data (thread-safe)
struct OverlayData {
    current_fps: f64,
    one_percent_low: f64,
    position: OverlayPosition,
    fps_color: FpsColor,
    size: OverlaySize,
    show_1_percent_low: bool,
}

static OVERLAY_HWND: AtomicIsize = AtomicIsize::new(0);
static OVERLAY_VISIBLE: AtomicBool = AtomicBool::new(false);
static OVERLAY_DATA: once_cell::sync::Lazy<Mutex<OverlayData>> =
    once_cell::sync::Lazy::new(|| Mutex::new(OverlayData {
        current_fps: 0.0,
        one_percent_low: 0.0,
        position: OverlayPosition::TopRight,
        fps_color: FpsColor::White,
        size: OverlaySize::Medium,
        show_1_percent_low: true,
    }));

pub fn init() -> Result<(), String> {
    std::thread::spawn(move || {
        if let Err(e) = run_overlay_window() {
            eprintln!("Overlay error: {}", e);
        }
    });
    std::thread::sleep(std::time::Duration::from_millis(100));
    Ok(())
}

pub fn show(fps: f64, one_percent_low: f64, settings: &Settings) {
    {
        let mut data = OVERLAY_DATA.lock();
        data.current_fps = fps;
        data.one_percent_low = one_percent_low;
        data.position = settings.position;
        data.fps_color = settings.fps_color;
        data.size = settings.size;
        data.show_1_percent_low = settings.show_1_percent_low;
    }
    
    let hwnd_val = OVERLAY_HWND.load(Ordering::SeqCst);
    if hwnd_val != 0 {
        let hwnd = HWND(hwnd_val as *mut std::ffi::c_void);
        
        if !OVERLAY_VISIBLE.load(Ordering::SeqCst) {
            OVERLAY_VISIBLE.store(true, Ordering::SeqCst);
            unsafe {
                let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
            }
        }
        
        // Update position and size
        unsafe {
            update_window(hwnd, settings);
            let _ = InvalidateRect(hwnd, None, true);
        }
    }
}

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

fn update_window(hwnd: HWND, settings: &Settings) {
    let (width, height, _, _) = settings.size.dimensions();
    let screen_width = unsafe { GetSystemMetrics(SM_CXSCREEN) };
    
    let (x, y) = match settings.position {
        OverlayPosition::TopRight => (screen_width - width - OVERLAY_MARGIN, OVERLAY_MARGIN),
        OverlayPosition::TopLeft => (OVERLAY_MARGIN, OVERLAY_MARGIN),
    };
    
    unsafe {
        let _ = SetWindowPos(hwnd, HWND_TOPMOST, x, y, width, height, SWP_NOACTIVATE);
    }
}

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
            
            let data = OVERLAY_DATA.lock();
            let (width, height, font_large, font_small) = data.size.dimensions();
            
            // Background
            let brush = CreateSolidBrush(windows::Win32::Foundation::COLORREF(BACKGROUND_COLOR));
            let pen = CreatePen(PS_SOLID, 1, windows::Win32::Foundation::COLORREF(BACKGROUND_COLOR));
            let old_brush = SelectObject(hdc, brush);
            let old_pen = SelectObject(hdc, pen);
            let _ = RoundRect(hdc, 0, 0, width, height, BORDER_RADIUS, BORDER_RADIUS);
            SelectObject(hdc, old_brush);
            SelectObject(hdc, old_pen);
            let _ = DeleteObject(brush);
            let _ = DeleteObject(pen);
            
            let _ = SetBkMode(hdc, TRANSPARENT);
            let (r, g, b) = data.fps_color.to_rgb();
            SetTextColor(hdc, windows::Win32::Foundation::COLORREF(
                (b as u32) << 16 | (g as u32) << 8 | (r as u32)
            ));
            
            // FPS number
            let font_fps = CreateFontW(
                font_large, 0, 0, 0, 700, 0, 0, 0, 0, 0, 0, 0, 0,
                windows::core::w!("Segoe UI"),
            );
            let old_font = SelectObject(hdc, font_fps);
            
            let fps_text = format!("{:.0}", data.current_fps);
            let fps_wide: Vec<u16> = fps_text.encode_utf16().collect();
            let _ = TextOutW(hdc, 6, 2, &fps_wide);
            
            // "FPS" label
            let font_label = CreateFontW(
                font_small, 0, 0, 0, 400, 0, 0, 0, 0, 0, 0, 0, 0,
                windows::core::w!("Segoe UI"),
            );
            SelectObject(hdc, font_label);
            
            let label_x = if data.current_fps >= 100.0 { 
                6 + (font_large as f32 * 1.8) as i32 
            } else if data.current_fps >= 10.0 { 
                6 + (font_large as f32 * 1.2) as i32 
            } else { 
                6 + (font_large as f32 * 0.7) as i32 
            };
            let fps_label: Vec<u16> = "FPS".encode_utf16().collect();
            let _ = TextOutW(hdc, label_x, 4, &fps_label);
            
            // 1% low
            if data.show_1_percent_low {
                SetTextColor(hdc, windows::Win32::Foundation::COLORREF(0x888888));
                let low_text = format!("1%: {:.0}", data.one_percent_low);
                let low_wide: Vec<u16> = low_text.encode_utf16().collect();
                let _ = TextOutW(hdc, 6, font_large + 2, &low_wide);
            }
            
            SelectObject(hdc, old_font);
            let _ = DeleteObject(font_fps);
            let _ = DeleteObject(font_label);
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

fn run_overlay_window() -> Result<(), String> {
    unsafe {
        let class_name = windows::core::w!("EasyFPS_Overlay");
        
        let wc = WNDCLASSW {
            lpfnWndProc: Some(overlay_wndproc),
            lpszClassName: class_name,
            hbrBackground: HBRUSH(std::ptr::null_mut()),
            ..Default::default()
        };
        
        RegisterClassW(&wc);
        
        let hwnd = CreateWindowExW(
            WS_EX_LAYERED | WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE | WS_EX_TRANSPARENT,
            class_name,
            windows::core::w!(""),
            WS_POPUP,
            0, 0, 100, 50,
            None, None, None, None,
        ).map_err(|e| format!("CreateWindowExW failed: {}", e))?;
        
        OVERLAY_HWND.store(hwnd.0 as isize, Ordering::SeqCst);
        
        SetLayeredWindowAttributes(hwnd, None, 230, LWA_ALPHA)
            .map_err(|e| format!("SetLayeredWindowAttributes failed: {}", e))?;
        
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

pub fn shutdown() {
    let hwnd_val = OVERLAY_HWND.load(Ordering::SeqCst);
    if hwnd_val != 0 {
        unsafe {
            let _ = windows::Win32::UI::WindowsAndMessaging::DestroyWindow(
                HWND(hwnd_val as *mut std::ffi::c_void)
            );
        }
        OVERLAY_HWND.store(0, Ordering::SeqCst);
    }
}
