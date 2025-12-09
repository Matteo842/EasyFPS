use windows::Win32::Foundation::{HWND, RECT};
use windows::Win32::Graphics::Dwm::{DwmGetWindowAttribute, DWMWA_CLOAKED};
use windows::Win32::UI::WindowsAndMessaging::{
    GetForegroundWindow, GetWindowLongW, GetWindowRect, GetWindowThreadProcessId,
    IsWindowVisible, GWL_EXSTYLE, GWL_STYLE, WS_EX_TOOLWINDOW, WS_POPUP,
};

/// Information about the current fullscreen application
#[derive(Debug, Clone)]
pub struct FullscreenApp {
    pub hwnd: isize,
    pub process_id: u32,
    pub width: i32,
    pub height: i32,
}

/// Check if there's a fullscreen application running
pub fn get_fullscreen_app() -> Option<FullscreenApp> {
    unsafe {
        let hwnd = GetForegroundWindow();
        if hwnd.0.is_null() {
            return None;
        }

        // Check if window is visible
        if !IsWindowVisible(hwnd).as_bool() {
            return None;
        }

        // Check if window is cloaked (virtual desktop)
        let mut cloaked: u32 = 0;
        let _ = DwmGetWindowAttribute(
            hwnd,
            DWMWA_CLOAKED,
            &mut cloaked as *mut _ as *mut _,
            std::mem::size_of::<u32>() as u32,
        );
        if cloaked != 0 {
            return None;
        }

        // Get window style
        let style = GetWindowLongW(hwnd, GWL_STYLE) as u32;
        let ex_style = GetWindowLongW(hwnd, GWL_EXSTYLE) as u32;

        // Skip tool windows
        if (ex_style & WS_EX_TOOLWINDOW.0) != 0 {
            return None;
        }

        // Get window rect
        let mut rect = RECT::default();
        if GetWindowRect(hwnd, &mut rect).is_err() {
            return None;
        }

        let window_width = rect.right - rect.left;
        let window_height = rect.bottom - rect.top;

        // Get monitor info for the window
        let (screen_width, screen_height) = get_primary_monitor_size();

        // Check if the window covers the entire screen
        let is_fullscreen = is_window_fullscreen(hwnd, &rect, screen_width, screen_height, style);

        if !is_fullscreen {
            return None;
        }

        // Get process ID
        let mut process_id: u32 = 0;
        GetWindowThreadProcessId(hwnd, Some(&mut process_id));

        Some(FullscreenApp {
            hwnd: hwnd.0 as isize,
            process_id,
            width: window_width,
            height: window_height,
        })
    }
}

/// Check if a window is fullscreen
fn is_window_fullscreen(_hwnd: HWND, rect: &RECT, screen_width: i32, screen_height: i32, style: u32) -> bool {
    let window_width = rect.right - rect.left;
    let window_height = rect.bottom - rect.top;

    // Method 1: Window covers or exceeds screen dimensions
    if window_width >= screen_width && window_height >= screen_height {
        // Additional check: window position should be at or near 0,0
        if rect.left <= 0 && rect.top <= 0 {
            return true;
        }
    }

    // Method 2: Borderless fullscreen (popup style, covering screen)
    if (style & WS_POPUP.0) != 0 {
        if window_width >= screen_width - 10 && window_height >= screen_height - 10 {
            return true;
        }
    }

    // Method 3: Check if window is "exclusive fullscreen" style
    // These windows typically have no border and exact screen size
    let has_no_border = (style & 0x00C00000) == 0; // WS_CAPTION
    if has_no_border && window_width == screen_width && window_height == screen_height {
        return true;
    }

    false
}

/// Get the primary monitor size
fn get_primary_monitor_size() -> (i32, i32) {
    use windows::Win32::UI::WindowsAndMessaging::{GetSystemMetrics, SM_CXSCREEN, SM_CYSCREEN};
    
    unsafe {
        let width = GetSystemMetrics(SM_CXSCREEN);
        let height = GetSystemMetrics(SM_CYSCREEN);
        (width, height)
    }
}

/// Get the name of a process by its ID
pub fn get_process_name(process_id: u32) -> Option<String> {
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::System::ProcessStatus::GetModuleBaseNameW;
    use windows::Win32::System::Threading::{OpenProcess, PROCESS_QUERY_INFORMATION, PROCESS_VM_READ};

    unsafe {
        let handle = OpenProcess(
            PROCESS_QUERY_INFORMATION | PROCESS_VM_READ,
            false,
            process_id,
        );

        if let Ok(handle) = handle {
            let mut buffer = [0u16; 260];
            let len = GetModuleBaseNameW(handle, None, &mut buffer);
            let _ = CloseHandle(handle);

            if len > 0 {
                return Some(String::from_utf16_lossy(&buffer[..len as usize]));
            }
        }

        None
    }
}
