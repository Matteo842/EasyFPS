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
    MSG, PM_REMOVE, SM_CXSCREEN, SWP_NOACTIVATE, SWP_SHOWWINDOW, SW_HIDE, SW_SHOWNOACTIVATE,
    WM_DESTROY, WM_PAINT, WNDCLASSW, WS_EX_LAYERED, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW,
    WS_EX_TOPMOST, WS_EX_TRANSPARENT, WS_POPUP,
};

const OVERLAY_MARGIN: i32 = 10;
const BACKGROUND_COLOR: u32 = 0x1A1A1A;
const BORDER_RADIUS: i32 = 6;
/// Vertical padding at the top and bottom of the overlay
const PADDING_TOP: i32 = 4;
const PADDING_BOTTOM: i32 = 2;

/// Overlay display data (thread-safe)
struct OverlayData {
    current_fps: f64,
    one_percent_low: f64,
    cpu_usage: f32,
    gpu_usage: f32,
    position: OverlayPosition,
    fps_color: FpsColor,
    size: OverlaySize,
    show_1_percent_low: bool,
    show_cpu_usage: bool,
    show_gpu_usage: bool,
    overlay_opacity: u8,
}

static OVERLAY_HWND: AtomicIsize = AtomicIsize::new(0);
static OVERLAY_VISIBLE: AtomicBool = AtomicBool::new(false);
static OVERLAY_DATA: once_cell::sync::Lazy<Mutex<OverlayData>> =
    once_cell::sync::Lazy::new(|| Mutex::new(OverlayData {
        current_fps: 0.0,
        one_percent_low: 0.0,
        cpu_usage: 0.0,
        gpu_usage: 0.0,
        position: OverlayPosition::TopRight,
        fps_color: FpsColor::White,
        size: OverlaySize::Medium,
        show_1_percent_low: true,
        show_cpu_usage: false,
        show_gpu_usage: false,
        overlay_opacity: 90,
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

pub fn show(fps: f64, one_percent_low: f64, cpu_usage: f32, gpu_usage: f32, settings: &Settings) {
    {
        let mut data = OVERLAY_DATA.lock();
        data.current_fps = fps;
        data.one_percent_low = one_percent_low;
        data.cpu_usage = cpu_usage;
        data.gpu_usage = gpu_usage;
        data.position = settings.position;
        data.fps_color = settings.fps_color;
        data.size = settings.size;
        data.show_1_percent_low = settings.show_1_percent_low;
        data.show_cpu_usage = settings.show_cpu_usage;
        data.show_gpu_usage = settings.show_gpu_usage;
        data.overlay_opacity = settings.overlay_opacity;
    }
    
    let hwnd_val = OVERLAY_HWND.load(Ordering::SeqCst);
    if hwnd_val != 0 {
        let hwnd = HWND(hwnd_val as isize);
        
        // Apply Opacity
        let alpha = (settings.overlay_opacity as f32 / 100.0 * 255.0) as u8;
        unsafe {
            let _ = SetLayeredWindowAttributes(hwnd, None, alpha, LWA_ALPHA);
        }
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
            let hwnd = HWND(hwnd_val as isize);
            unsafe {
                let _ = ShowWindow(hwnd, SW_HIDE);
            }
        }
    }
}

/// Counts how many lines will be displayed in the overlay.
fn count_visible_lines(data: &OverlayData) -> i32 {
    let mut lines = 1; // FPS is always shown
    if data.show_1_percent_low { lines += 1; }
    if data.show_cpu_usage { lines += 1; }
    if data.show_gpu_usage { lines += 1; }
    lines
}

/// Calculates the dimensions the overlay needs based on the current data.
/// Returns (width, height).
fn calculate_dimensions(data: &OverlayData) -> (i32, i32) {
    let (_default_width, _height, font_large, _font_small) = data.size.dimensions();
    let line_height = font_large + 4;
    let num_lines = count_visible_lines(data);

    // Width: estimate based on the widest possible content line
    // Each character is roughly font_large * 0.6 wide
    let char_w = (font_large as f32 * 0.6) as i32;
    let h_pad = 12; // 6px left + 6px right

    // FPS line: "FPS  " + up to 4-digit number → ~8 chars
    let fps_digits = if data.current_fps >= 1000.0 { 4 }
                     else if data.current_fps >= 100.0 { 3 }
                     else if data.current_fps >= 10.0 { 2 }
                     else { 1 };
    let fps_w = h_pad + char_w * (4 + fps_digits); // "FPS " + space + digits

    let mut max_width = fps_w;

    if data.show_1_percent_low {
        let low_digits = if data.one_percent_low >= 1000.0 { 4 }
                         else if data.one_percent_low >= 100.0 { 3 }
                         else if data.one_percent_low >= 10.0 { 2 }
                         else { 1 };
        let w = h_pad + char_w * (3 + low_digits); // "1% " + digits
        max_width = max_width.max(w);
    }
    if data.show_cpu_usage {
        // "CPU  100%" → 9 chars
        let w = h_pad + char_w * 9;
        max_width = max_width.max(w);
    }
    if data.show_gpu_usage {
        let w = h_pad + char_w * 9;
        max_width = max_width.max(w);
    }

    let total_height = PADDING_TOP + (line_height * num_lines) + PADDING_BOTTOM;

    (max_width, total_height)
}

fn update_window(hwnd: HWND, settings: &Settings) {
    let data = OVERLAY_DATA.lock();
    let (width, total_height) = calculate_dimensions(&*data);
    drop(data);
    
    let screen_width = unsafe { GetSystemMetrics(SM_CXSCREEN) };
    
    let (x, y) = match settings.position {
        OverlayPosition::TopRight => (screen_width - width - OVERLAY_MARGIN, OVERLAY_MARGIN),
        OverlayPosition::TopLeft => (OVERLAY_MARGIN, OVERLAY_MARGIN),
    };
    
    unsafe {
        // Force Z-Order: Bring to top first
        let _ = windows::Win32::UI::WindowsAndMessaging::BringWindowToTop(hwnd);
        // SWP_SHOWWINDOW ensures it stays visible even if something tried to hide it
        let _ = SetWindowPos(hwnd, HWND_TOPMOST, x, y, width, total_height, SWP_NOACTIVATE | SWP_SHOWWINDOW);
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
            let (_default_width, _height, font_large, _font_small) = data.size.dimensions();
            let (width, total_height) = calculate_dimensions(&*data);
            
            // Background
            let brush = CreateSolidBrush(windows::Win32::Foundation::COLORREF(BACKGROUND_COLOR));
            let pen = CreatePen(PS_SOLID, 1, windows::Win32::Foundation::COLORREF(BACKGROUND_COLOR));
            let old_brush = SelectObject(hdc, brush);
            let old_pen = SelectObject(hdc, pen);
            let _ = RoundRect(hdc, 0, 0, width, total_height, BORDER_RADIUS, BORDER_RADIUS);
            SelectObject(hdc, old_brush);
            SelectObject(hdc, old_pen);
            let _ = DeleteObject(brush);
            let _ = DeleteObject(pen);
            
            let _ = SetBkMode(hdc, TRANSPARENT);
            
            // Create font ONCE for all lines (avoids repeated GDI alloc/free)
            let font = CreateFontW(
                font_large, 0, 0, 0, 700, 0, 0, 0, 0, 0, 0, 0, 0,
                windows::core::w!("Segoe UI"),
            );
            let old_font = SelectObject(hdc, font);
            
            // Colors
            let line_height = font_large + 4;
            let label_color_ref = windows::Win32::Foundation::COLORREF(0xAAAAAA); // Light gray for labels
            let (r, g, b) = data.fps_color.to_rgb();
            let value_color_ref = windows::Win32::Foundation::COLORREF(
                 (b as u32) << 16 | (g as u32) << 8 | (r as u32)
            );

            let mut current_y = PADDING_TOP;

            // Helper to draw a line: "Label  Value"
            // Label is gray, Value is colored
            let draw_stat_line = |label: &str, value: String, y: i32| {
                // Draw Label (Gray)
                SetTextColor(hdc, label_color_ref);
                let label_wide: Vec<u16> = format!("{}  ", label).encode_utf16().collect();
                let _ = TextOutW(hdc, 6, y, &label_wide);
                
                // Calc label width to position value
                let mut size = windows::Win32::Foundation::SIZE::default();
                let _ = windows::Win32::Graphics::Gdi::GetTextExtentPoint32W(hdc, &label_wide, &mut size);
                
                // Draw Value (Colored)
                SetTextColor(hdc, value_color_ref);
                let value_wide: Vec<u16> = value.encode_utf16().collect();
                let _ = TextOutW(hdc, 6 + size.cx, y, &value_wide);
            };

            // FPS
            let fps_val = format!("{:.0}", data.current_fps);
            draw_stat_line("FPS", fps_val, current_y);
            current_y += line_height;

            // 1% low
            if data.show_1_percent_low {
                let val = format!("{:.0}", data.one_percent_low);
                draw_stat_line("1%", val, current_y);
                current_y += line_height;
            }

            // CPU
            if data.show_cpu_usage {
                let val = format!("{:.0}%", data.cpu_usage);
                draw_stat_line("CPU", val, current_y);
                current_y += line_height;
            }

            // GPU
            if data.show_gpu_usage {
                let val = format!("{:.0}%", data.gpu_usage);
                draw_stat_line("GPU", val, current_y);
            }
            
            // Restore and cleanup font (single deallocation)
            SelectObject(hdc, old_font);
            let _ = DeleteObject(font);
            
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
            hbrBackground: HBRUSH(0),
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
        );
        
        if hwnd.0 == 0 {
            return Err("CreateWindowExW failed".to_string());
        }
        
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
                HWND(hwnd_val as isize)
            );
        }
        OVERLAY_HWND.store(0, Ordering::SeqCst);
    }
}
