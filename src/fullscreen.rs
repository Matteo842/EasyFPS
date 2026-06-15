use windows::Win32::Foundation::{HWND, RECT};
use windows::Win32::Graphics::Dwm::{DwmGetWindowAttribute, DWMWA_CLOAKED};
use windows::Win32::Graphics::Gdi::{
    GetMonitorInfoW, MonitorFromWindow, MONITORINFO, MONITOR_DEFAULTTONEAREST,
};
use windows::Win32::UI::WindowsAndMessaging::{
    GetForegroundWindow, GetWindowLongW, GetWindowRect, GetWindowThreadProcessId, IsWindowVisible,
    GWL_EXSTYLE, GWL_STYLE, WS_EX_TOOLWINDOW,
};

/// Well-known non-game processes that can appear fullscreen but should be ignored.
/// All entries must be lowercase for case-insensitive comparison.
const EXCLUDED_PROCESSES: &[&str] = &[
    // Windows shell & system
    "explorer.exe",
    "shellexperiencehost.exe",
    "startmenuexperiencehost.exe",
    "searchhost.exe",
    "searchui.exe",
    "systemsettings.exe",
    "textinputhost.exe",
    "lockapp.exe",
    "logonui.exe",
    "dwm.exe",
    // UWP host (Settings, Store, Photos, etc.)
    "applicationframehost.exe",
    // Browsers
    "chrome.exe",
    "msedge.exe",
    "firefox.exe",
    "opera.exe",
    "brave.exe",
    "vivaldi.exe",
    // Media players
    "vlc.exe",
    "wmplayer.exe",
    "mpc-hc64.exe",
    "mpc-hc.exe",
    // Editors / dev tools
    "code.exe",
    "devenv.exe",
    "rider64.exe",
    // Office
    "winword.exe",
    "excel.exe",
    "powerpnt.exe",
    // File managers
    "totalcmd64.exe",
    "totalcmd.exe",
    // Task manager
    "taskmgr.exe",
];

/// Bounds of a monitor in virtual-screen coordinates.
#[derive(Debug, Clone, Copy)]
pub struct MonitorBounds {
    pub left: i32,
    pub top: i32,
    pub width: i32,
    pub height: i32,
}

/// Information about the current fullscreen application
#[allow(dead_code)]
pub struct FullscreenApp {
    pub hwnd: isize,
    pub process_id: u32,
    pub width: i32,
    pub height: i32,
    /// The monitor the fullscreen window lives on. Used to place the overlay
    /// on the correct screen in multi-monitor setups.
    pub monitor: MonitorBounds,
}

/// Check if there's a fullscreen application running
pub fn get_fullscreen_app() -> Option<FullscreenApp> {
    unsafe {
        let hwnd = GetForegroundWindow();
        if hwnd.0 == 0 {
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

        // Get the bounds of the monitor the window is actually on (handles
        // multi-monitor setups and mixed resolutions correctly).
        let monitor = get_window_monitor_bounds(hwnd);

        // Check if the window covers that monitor entirely.
        let is_fullscreen = is_window_fullscreen(hwnd, &rect, monitor.width, monitor.height, style);

        if !is_fullscreen {
            return None;
        }

        // Get process ID
        let mut process_id: u32 = 0;
        GetWindowThreadProcessId(hwnd, Some(&mut process_id));

        // Exclude known non-game processes
        if let Some(name) = get_process_name(process_id) {
            let name_lower = name.to_lowercase();
            if EXCLUDED_PROCESSES
                .iter()
                .any(|&excluded| name_lower == excluded)
            {
                return None;
            }
        }

        Some(FullscreenApp {
            hwnd: hwnd.0 as isize,
            process_id,
            width: window_width,
            height: window_height,
            monitor,
        })
    }
}

/// Get the bounds of the monitor that the given window is on.
fn get_window_monitor_bounds(hwnd: HWND) -> MonitorBounds {
    unsafe {
        let hmon = MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST);
        let mut info = MONITORINFO {
            cbSize: std::mem::size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };
        if GetMonitorInfoW(hmon, &mut info).as_bool() {
            let r = info.rcMonitor;
            return MonitorBounds {
                left: r.left,
                top: r.top,
                width: r.right - r.left,
                height: r.bottom - r.top,
            };
        }
    }

    // Fallback to the primary monitor size.
    let (width, height) = get_primary_monitor_size();
    MonitorBounds {
        left: 0,
        top: 0,
        width,
        height,
    }
}

/// Check if a window is fullscreen
fn is_window_fullscreen(
    _hwnd: HWND,
    rect: &RECT,
    screen_width: i32,
    screen_height: i32,
    _style: u32,
) -> bool {
    let window_width = rect.right - rect.left;
    let window_height = rect.bottom - rect.top;

    // Robust Check: If the window is roughly the size of the screen (or bigger), it's fullscreen.
    // We allow a margin of error (e.g. 30 pixels) for borders or weird scaling.
    let width_diff = (window_width - screen_width).abs();
    let height_diff = (window_height - screen_height).abs();

    if width_diff < 30 && height_diff < 30 {
        return true;
    }

    // Also check if it's BIGGER than screen (e.g. running at higher res DSR/VSR)
    if window_width >= screen_width && window_height >= screen_height {
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
    use windows::Win32::System::Threading::{
        OpenProcess, PROCESS_QUERY_INFORMATION, PROCESS_VM_READ,
    };

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

#[cfg(test)]
mod tests {
    use super::*;

    fn rect(w: i32, h: i32) -> RECT {
        RECT {
            left: 0,
            top: 0,
            right: w,
            bottom: h,
        }
    }

    #[test]
    fn exact_match_is_fullscreen() {
        let r = rect(1920, 1080);
        assert!(is_window_fullscreen(HWND(0), &r, 1920, 1080, 0));
    }

    #[test]
    fn within_margin_is_fullscreen() {
        // A few pixels off (borderless quirks) should still count.
        let r = rect(1920 - 10, 1080 - 5);
        assert!(is_window_fullscreen(HWND(0), &r, 1920, 1080, 0));
    }

    #[test]
    fn larger_than_screen_is_fullscreen() {
        // DSR/VSR can render above native resolution.
        let r = rect(2560, 1440);
        assert!(is_window_fullscreen(HWND(0), &r, 1920, 1080, 0));
    }

    #[test]
    fn small_window_is_not_fullscreen() {
        let r = rect(800, 600);
        assert!(!is_window_fullscreen(HWND(0), &r, 1920, 1080, 0));
    }
}
