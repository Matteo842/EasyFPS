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
    MSG, PM_REMOVE, SM_CXSCREEN, SWP_NOACTIVATE, SW_HIDE, SW_SHOWNOACTIVATE,
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
    cpu_usage: f32,
    gpu_usage: f32,
    position: OverlayPosition,
    fps_color: FpsColor,
    size: OverlaySize,
    show_1_percent_low: bool,
    show_cpu_usage: bool,
    show_gpu_usage: bool,
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
    }
    
    let hwnd_val = OVERLAY_HWND.load(Ordering::SeqCst);
    if hwnd_val != 0 {
        let hwnd = HWND(hwnd_val as isize);
        
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

fn calculate_dimensions(data: &OverlayData) -> (i32, i32, i32, i32) {
    let (_, height, font_large, font_small) = data.size.dimensions();
    
    // FPS Width
    let fps_num_width = if data.current_fps >= 100.0 {
        (font_large as f32 * 0.6 * 3.0) as i32
    } else if data.current_fps >= 10.0 {
        (font_large as f32 * 0.6 * 2.0) as i32
    } else {
        (font_large as f32 * 0.6) as i32
    };
    let fps_label_width = (font_small as f32 * 0.5 * 3.0) as i32;
    let fps_total_width = 6 + fps_num_width + 4 + fps_label_width + 6;

    let mut max_width = fps_total_width;
    let mut total_height = height;


    // Check additional lines width
    // Use approximation: char width ~ font_large * 0.6
    let estimate_width = |text_len: usize| -> i32 {
        6 + (font_large as f32 * 0.6 * text_len as f32) as i32 + 6
    };
    
    // Line height is now larger (font_large)
    let line_height = font_large + 4;

    if data.show_1_percent_low {
        // "1%: 100" -> 7 chars approx
        let w = estimate_width(8);
        max_width = max_width.max(w);
        total_height += line_height;
    }
    if data.show_cpu_usage {
        // "CPU: 100%" -> 9 chars
        let w = estimate_width(10);
        max_width = max_width.max(w);
        total_height += line_height;
    }
    if data.show_gpu_usage {
        // "GPU: 100%" -> 9 chars
        let w = estimate_width(10);
        max_width = max_width.max(w);
        total_height += line_height;
    }

    (max_width, total_height, fps_num_width, fps_label_width)
}

fn update_window(hwnd: HWND, settings: &Settings) {
    let data = OVERLAY_DATA.lock();
    let (default_width, height, font_large, _font_small) = settings.size.dimensions();
    
    // Calculate width based on content
    let (base_w, _, _, _) = calculate_dimensions(&*data);
    let width = base_w.min(default_width);
    
    // Calculate height based on enabled lines
    // Base height is for FPS line
    let mut total_height = height; 
    
    // Additional lines use font_large + padding
    let line_height = font_large + 4;
    
    if data.show_1_percent_low {
        total_height += line_height;
    }
    if data.show_cpu_usage {
        total_height += line_height;
    }
    if data.show_gpu_usage {
        total_height += line_height;
    }
    
    drop(data);
    
    let screen_width = unsafe { GetSystemMetrics(SM_CXSCREEN) };
    
    let (x, y) = match settings.position {
        OverlayPosition::TopRight => (screen_width - width - OVERLAY_MARGIN, OVERLAY_MARGIN),
        OverlayPosition::TopLeft => (OVERLAY_MARGIN, OVERLAY_MARGIN),
    };
    
    unsafe {
        let _ = SetWindowPos(hwnd, HWND_TOPMOST, x, y, width, total_height, SWP_NOACTIVATE);
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
            let (default_width, _height, font_large, _font_small) = data.size.dimensions();
            
            let (actual_width, total_height, _fps_num_width, _) = calculate_dimensions(&*data);
            
            // Use calculated width or default, whichever is smaller (to avoid too wide)
            let width = actual_width.min(default_width);
            
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
            
            // Shared Drawing State
            let mut current_y = 2; // Start with a small top padding
            let line_height = font_large + 4; 
            let label_color_ref = windows::Win32::Foundation::COLORREF(0xAAAAAA); // Light gray for labels
            let (r, g, b) = data.fps_color.to_rgb();
            let value_color_ref = windows::Win32::Foundation::COLORREF(
                 (b as u32) << 16 | (g as u32) << 8 | (r as u32)
            );

            // Helper to draw a line: "Label  Value"
            // Label is gray, Value is colored (white/green/whatever set in settings)
            // Both use the same Large Font
            let draw_stat_line = |label: &str, value: String, y: i32| {
                let font = CreateFontW(
                    font_large, 0, 0, 0, 700, 0, 0, 0, 0, 0, 0, 0, 0,
                    windows::core::w!("Segoe UI"),
                );
                let old_font_loop = SelectObject(hdc, font);
                
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
                
                SelectObject(hdc, old_font_loop);
                let _ = DeleteObject(font);
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
