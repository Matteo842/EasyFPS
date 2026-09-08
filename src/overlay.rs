use crate::fullscreen::MonitorBounds;
use crate::settings::{FpsColor, OverlayPosition, OverlaySize, Settings};
use parking_lot::Mutex;
use std::cell::RefCell;
use std::sync::atomic::{AtomicBool, AtomicIsize, Ordering};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::Graphics::Gdi::{
    BeginPaint, CreateFontW, CreatePen, CreateSolidBrush, DeleteObject, EndPaint, InvalidateRect,
    RoundRect, SelectObject, SetBkMode, SetTextColor, TextOutW, FONT_CHARSET, FONT_CLIP_PRECISION,
    FONT_OUTPUT_PRECISION, FONT_QUALITY, HBRUSH, HFONT, PAINTSTRUCT, PS_SOLID, TRANSPARENT,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DispatchMessageW, GetMessageW, PostQuitMessage,
    RegisterClassW, SetLayeredWindowAttributes, SetWindowPos, ShowWindow, TranslateMessage,
    HWND_TOPMOST, LWA_ALPHA, MSG, SWP_NOACTIVATE, SWP_SHOWWINDOW, SW_HIDE, SW_SHOWNOACTIVATE,
    WM_DESTROY, WM_PAINT, WNDCLASSW, WS_EX_LAYERED, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW,
    WS_EX_TOPMOST, WS_EX_TRANSPARENT, WS_POPUP,
};

const OVERLAY_MARGIN: i32 = 10;
const BACKGROUND_COLOR: u32 = 0x1A1A1A;
const BORDER_RADIUS: i32 = 6;
/// Vertical padding at the top and bottom of the overlay
const PADDING_TOP: i32 = 4;
const PADDING_BOTTOM: i32 = 2;
/// ClearType anti-aliasing quality for crisp overlay text.
const CLEARTYPE_QUALITY: u8 = 5;

thread_local! {
    /// Cached overlay font, recreated only when the font size changes. Avoids
    /// allocating and freeing a GDI font object on every single paint.
    static CACHED_FONT: RefCell<(HFONT, i32)> = const { RefCell::new((HFONT(std::ptr::null_mut()), 0)) };
}

/// Return a cached font for the given size, (re)creating it only when the size
/// changed since the last call. The returned handle is owned by the cache and
/// must not be deleted by the caller.
unsafe fn get_cached_font(size: i32) -> HFONT {
    CACHED_FONT.with(|cell| {
        let mut slot = cell.borrow_mut();
        if slot.1 != size || slot.0 .0.is_null() {
            if !slot.0 .0.is_null() {
                let _ = DeleteObject(slot.0.into());
            }
            let font = CreateFontW(
                size,
                0,
                0,
                0,
                700,
                0,
                0,
                0,
                FONT_CHARSET(0),
                FONT_OUTPUT_PRECISION(0),
                FONT_CLIP_PRECISION(0),
                FONT_QUALITY(CLEARTYPE_QUALITY),
                0,
                windows::core::w!("Segoe UI"),
            );
            *slot = (font, size);
        }
        slot.0
    })
}

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
    monitor: MonitorBounds,
}

/// A compact, comparable snapshot of everything that affects the rendered
/// overlay. Used to skip repaints/repositioning when nothing visible changed.
#[derive(Clone, Copy, PartialEq, Eq)]
struct RenderSignature {
    fps: i32,
    low: i32,
    cpu: i32,
    gpu: i32,
    position: u8,
    size: u8,
    color: u8,
    opacity: u8,
    flags: u8,
    monitor: (i32, i32, i32, i32),
}

static OVERLAY_HWND: AtomicIsize = AtomicIsize::new(0);
static OVERLAY_VISIBLE: AtomicBool = AtomicBool::new(false);
static LAST_SIGNATURE: Mutex<Option<RenderSignature>> = Mutex::new(None);
static OVERLAY_DATA: once_cell::sync::Lazy<Mutex<OverlayData>> = once_cell::sync::Lazy::new(|| {
    Mutex::new(OverlayData {
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
        monitor: MonitorBounds {
            left: 0,
            top: 0,
            width: 1920,
            height: 1080,
        },
    })
});

fn build_signature(data: &OverlayData) -> RenderSignature {
    let flags = (data.show_1_percent_low as u8)
        | ((data.show_cpu_usage as u8) << 1)
        | ((data.show_gpu_usage as u8) << 2);
    RenderSignature {
        fps: data.current_fps.round() as i32,
        low: data.one_percent_low.round() as i32,
        cpu: data.cpu_usage.round() as i32,
        gpu: data.gpu_usage.round() as i32,
        position: data.position as u8,
        size: data.size as u8,
        color: data.fps_color as u8,
        opacity: data.overlay_opacity,
        flags,
        monitor: (
            data.monitor.left,
            data.monitor.top,
            data.monitor.width,
            data.monitor.height,
        ),
    }
}

pub fn init() -> Result<(), String> {
    std::thread::spawn(move || {
        if let Err(e) = run_overlay_window() {
            eprintln!("Overlay error: {}", e);
        }
    });
    std::thread::sleep(std::time::Duration::from_millis(100));
    Ok(())
}

pub fn show(
    fps: f64,
    one_percent_low: f64,
    cpu_usage: f32,
    gpu_usage: f32,
    monitor: MonitorBounds,
    settings: &Settings,
) {
    let signature = {
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
        data.monitor = monitor;
        build_signature(&data)
    };

    let hwnd_val = OVERLAY_HWND.load(Ordering::SeqCst);
    if hwnd_val == 0 {
        return;
    }
    let hwnd = HWND(hwnd_val as *mut _);

    let was_visible = OVERLAY_VISIBLE.load(Ordering::SeqCst);

    // Skip all GDI work when nothing that affects the rendered overlay changed.
    let mut last = LAST_SIGNATURE.lock();
    let unchanged = was_visible && *last == Some(signature);
    if unchanged {
        return;
    }
    *last = Some(signature);
    drop(last);

    unsafe {
        // Apply opacity
        let alpha = (settings.overlay_opacity as f32 / 100.0 * 255.0) as u8;
        let _ = SetLayeredWindowAttributes(
            hwnd,
            windows::Win32::Foundation::COLORREF(0),
            alpha,
            LWA_ALPHA,
        );

        if !was_visible {
            OVERLAY_VISIBLE.store(true, Ordering::SeqCst);
            let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
        }

        // Update position and size, then repaint
        update_window(hwnd);
        let _ = InvalidateRect(Some(hwnd), None, true);
    }
}

pub fn hide() {
    if OVERLAY_VISIBLE.load(Ordering::SeqCst) {
        OVERLAY_VISIBLE.store(false, Ordering::SeqCst);
        // Force a fresh repaint/reposition the next time the overlay is shown.
        *LAST_SIGNATURE.lock() = None;
        let hwnd_val = OVERLAY_HWND.load(Ordering::SeqCst);
        if hwnd_val != 0 {
            let hwnd = HWND(hwnd_val as *mut _);
            unsafe {
                let _ = ShowWindow(hwnd, SW_HIDE);
            }
        }
    }
}

/// Counts how many lines will be displayed in the overlay.
fn count_visible_lines(data: &OverlayData) -> i32 {
    let mut lines = 1; // FPS is always shown
    if data.show_1_percent_low {
        lines += 1;
    }
    if data.show_cpu_usage {
        lines += 1;
    }
    if data.show_gpu_usage {
        lines += 1;
    }
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

    // Fixed at 3 digits minimum so the overlay width does not jitter
    // when FPS fluctuates between 2 and 3 digits (e.g. 90-110 FPS).
    let fps_digits = if data.current_fps.round() >= 1000.0 {
        4
    } else {
        3
    };
    let fps_w = h_pad + char_w * (4 + fps_digits); // "FPS " + space + digits

    let mut max_width = fps_w;

    if data.show_1_percent_low {
        let low_digits = if data.one_percent_low.round() >= 1000.0 {
            4
        } else {
            3
        };
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

fn update_window(hwnd: HWND) {
    let data = OVERLAY_DATA.lock();
    let (width, total_height) = calculate_dimensions(&data);
    let position = data.position;
    let monitor = data.monitor;
    drop(data);

    // Position the overlay relative to the monitor the game is running on
    // (correct for multi-monitor and mixed-resolution setups).
    let (x, y) = match position {
        OverlayPosition::TopRight => (
            monitor.left + monitor.width - width - OVERLAY_MARGIN,
            monitor.top + OVERLAY_MARGIN,
        ),
        OverlayPosition::TopLeft => (monitor.left + OVERLAY_MARGIN, monitor.top + OVERLAY_MARGIN),
    };

    unsafe {
        // Force Z-Order: Bring to top first
        let _ = windows::Win32::UI::WindowsAndMessaging::BringWindowToTop(hwnd);
        // SWP_SHOWWINDOW ensures it stays visible even if something tried to hide it
        let _ = SetWindowPos(
            hwnd,
            Some(HWND_TOPMOST),
            x,
            y,
            width,
            total_height,
            SWP_NOACTIVATE | SWP_SHOWWINDOW,
        );
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
            let (width, total_height) = calculate_dimensions(&data);

            // Background
            let brush = CreateSolidBrush(windows::Win32::Foundation::COLORREF(BACKGROUND_COLOR));
            let pen = CreatePen(
                PS_SOLID,
                1,
                windows::Win32::Foundation::COLORREF(BACKGROUND_COLOR),
            );
            let old_brush = SelectObject(hdc, brush.into());
            let old_pen = SelectObject(hdc, pen.into());
            let _ = RoundRect(hdc, 0, 0, width, total_height, BORDER_RADIUS, BORDER_RADIUS);
            SelectObject(hdc, old_brush);
            SelectObject(hdc, old_pen);
            let _ = DeleteObject(brush.into());
            let _ = DeleteObject(pen.into());

            let _ = SetBkMode(hdc, TRANSPARENT);

            // Reuse a cached font (recreated only when the size changes) to
            // avoid GDI alloc/free churn on every paint.
            let font = get_cached_font(font_large);
            let old_font = SelectObject(hdc, font.into());

            // Colors
            let line_height = font_large + 4;
            let label_color_ref = windows::Win32::Foundation::COLORREF(0xAAAAAA); // Light gray for labels
            let (r, g, b) = data.fps_color.to_rgb();
            let value_color_ref = windows::Win32::Foundation::COLORREF(
                (b as u32) << 16 | (g as u32) << 8 | (r as u32),
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
                let _ = windows::Win32::Graphics::Gdi::GetTextExtentPoint32W(
                    hdc,
                    &label_wide,
                    &mut size,
                );

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

            // Restore the previous font; the cached font is kept alive in the
            // thread-local cache and reused on the next paint.
            SelectObject(hdc, old_font);

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
            hbrBackground: HBRUSH::default(),
            ..Default::default()
        };

        RegisterClassW(&wc);

        let hwnd = CreateWindowExW(
            WS_EX_LAYERED | WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE | WS_EX_TRANSPARENT,
            class_name,
            windows::core::w!(""),
            WS_POPUP,
            0,
            0,
            100,
            50,
            None,
            None,
            None,
            None,
        )
        .map_err(|e| format!("CreateWindowExW failed: {}", e))?;

        if hwnd.0.is_null() {
            return Err("CreateWindowExW failed".to_string());
        }

        OVERLAY_HWND.store(hwnd.0 as isize, Ordering::SeqCst);

        SetLayeredWindowAttributes(
            hwnd,
            windows::Win32::Foundation::COLORREF(0),
            230,
            LWA_ALPHA,
        )
        .map_err(|e| format!("SetLayeredWindowAttributes failed: {}", e))?;

        // Block on the message queue. The overlay only repaints when the main
        // thread calls InvalidateRect/SetWindowPos, which wake this loop, so
        // there is no need to spin.
        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }

        // Release the cached font owned by this thread.
        CACHED_FONT.with(|cell| {
            let mut slot = cell.borrow_mut();
            if !slot.0 .0.is_null() {
                let _ = DeleteObject(slot.0.into());
                *slot = (HFONT(std::ptr::null_mut()), 0);
            }
        });
        Ok(())
    }
}

pub fn shutdown() {
    let hwnd_val = OVERLAY_HWND.load(Ordering::SeqCst);
    if hwnd_val != 0 {
        unsafe {
            let _ =
                windows::Win32::UI::WindowsAndMessaging::DestroyWindow(HWND(hwnd_val as *mut _));
        }
        OVERLAY_HWND.store(0, Ordering::SeqCst);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_test_data(fps: f64, low: f64, size: OverlaySize) -> OverlayData {
        OverlayData {
            current_fps: fps,
            one_percent_low: low,
            cpu_usage: 0.0,
            gpu_usage: 0.0,
            position: OverlayPosition::TopRight,
            fps_color: FpsColor::White,
            size,
            show_1_percent_low: true,
            show_cpu_usage: false,
            show_gpu_usage: false,
            overlay_opacity: 90,
            monitor: MonitorBounds {
                left: 0,
                top: 0,
                width: 1920,
                height: 1080,
            },
        }
    }

    #[test]
    fn fps_fluctuation_under_1000_does_not_change_width() {
        let mut data = make_test_data(60.0, 45.0, OverlaySize::Medium);
        let (w_60, h_60) = calculate_dimensions(&data);

        data.current_fps = 99.0;
        data.one_percent_low = 85.0;
        let (w_99, _) = calculate_dimensions(&data);

        data.current_fps = 100.0;
        data.one_percent_low = 95.0;
        let (w_100, _) = calculate_dimensions(&data);

        data.current_fps = 144.0;
        data.one_percent_low = 105.0;
        let (w_144, _) = calculate_dimensions(&data);

        data.current_fps = 5.0;
        data.one_percent_low = 2.0;
        let (w_5, _) = calculate_dimensions(&data);

        data.current_fps = 999.0;
        data.one_percent_low = 950.0;
        let (w_999, _) = calculate_dimensions(&data);

        // Width must be identical across all values from 0 to 999 FPS
        assert_eq!(w_60, w_99);
        assert_eq!(w_99, w_100);
        assert_eq!(w_100, w_144);
        assert_eq!(w_144, w_5);
        assert_eq!(w_5, w_999);

        assert_eq!(h_60, calculate_dimensions(&data).1);
    }

    #[test]
    fn fps_above_1000_expands_width() {
        let data_999 = make_test_data(999.0, 500.0, OverlaySize::Medium);
        let (w_999, _) = calculate_dimensions(&data_999);

        let data_1000 = make_test_data(1000.0, 500.0, OverlaySize::Medium);
        let (w_1000, _) = calculate_dimensions(&data_1000);

        assert!(w_1000 > w_999);
    }

    #[test]
    fn sizes_scale_dimensions() {
        let data_s = make_test_data(100.0, 90.0, OverlaySize::Small);
        let data_m = make_test_data(100.0, 90.0, OverlaySize::Medium);
        let data_l = make_test_data(100.0, 90.0, OverlaySize::Large);

        let (w_s, h_s) = calculate_dimensions(&data_s);
        let (w_m, h_m) = calculate_dimensions(&data_m);
        let (w_l, h_l) = calculate_dimensions(&data_l);

        assert!(w_s < w_m);
        assert!(w_m < w_l);
        assert!(h_s < h_m);
        assert!(h_m < h_l);
    }
}
