use crate::settings::{FpsColor, OverlayPosition, OverlaySize, Settings};
use std::sync::atomic::{AtomicBool, Ordering};
use windows::core::PCWSTR;
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::Win32::Graphics::Gdi::*;

static GUI_OPEN: AtomicBool = AtomicBool::new(false);

// Control IDs
const ID_POS_RIGHT: i32 = 101;
const ID_POS_LEFT: i32 = 102;
const ID_COLOR_WHITE: i32 = 103;
const ID_COLOR_GREEN: i32 = 104;
const ID_SIZE_SMALL: i32 = 105;
const ID_SIZE_MEDIUM: i32 = 106;
const ID_SIZE_LARGE: i32 = 107;
const ID_SHOW_1LOW: i32 = 108;
const ID_STARTUP: i32 = 109;
const ID_SAVE: i32 = 110;
const ID_CANCEL: i32 = 111;

// Button check states
const BST_CHECKED_VAL: usize = 1;

thread_local! {
    static CURRENT_SETTINGS: std::cell::RefCell<Option<Settings>> = std::cell::RefCell::new(None);
    static SAVE_CALLBACK: std::cell::RefCell<Option<Box<dyn FnOnce(Settings) + Send>>> = std::cell::RefCell::new(None);
}

pub fn is_open() -> bool {
    GUI_OPEN.load(Ordering::SeqCst)
}

pub fn open(settings: Settings, on_save: impl FnOnce(Settings) + Send + 'static) {
    if GUI_OPEN.swap(true, Ordering::SeqCst) {
        return;
    }
    
    std::thread::spawn(move || {
        CURRENT_SETTINGS.with(|s| *s.borrow_mut() = Some(settings));
        SAVE_CALLBACK.with(|c| *c.borrow_mut() = Some(Box::new(on_save)));
        
        unsafe {
            create_settings_window();
        }
        
        GUI_OPEN.store(false, Ordering::SeqCst);
    });
}

unsafe fn create_settings_window() {
    let class_name = windows::core::w!("EasyFPS_Settings");
    
    let wc = WNDCLASSEXW {
        cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
        style: CS_HREDRAW | CS_VREDRAW,
        lpfnWndProc: Some(settings_wndproc),
        hbrBackground: HBRUSH(GetStockObject(BLACK_BRUSH).0),
        lpszClassName: class_name,
        ..Default::default()
    };
    
    RegisterClassExW(&wc);
    
    let hwnd = CreateWindowExW(
        WS_EX_TOPMOST,
        class_name,
        windows::core::w!("EasyFPS - Impostazioni"),
        WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU,
        CW_USEDEFAULT, CW_USEDEFAULT,
        320, 320,
        None, None, None, None,
    );
    
    if let Ok(hwnd) = hwnd {
        let _ = ShowWindow(hwnd, SW_SHOW);
        let _ = UpdateWindow(hwnd);
        
        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
}

unsafe fn create_controls(hwnd: HWND) {
    let settings = CURRENT_SETTINGS.with(|s| s.borrow().clone().unwrap_or_default());
    
    let button_class = windows::core::w!("BUTTON");
    let static_class = windows::core::w!("STATIC");
    
    // Title
    create_label(hwnd, static_class, "Impostazioni EasyFPS", 10, 10, 280, 20);
    
    // Position
    create_label(hwnd, static_class, "Posizione:", 10, 45, 80, 20);
    create_radio(hwnd, button_class, "Destra", ID_POS_RIGHT, 100, 45, 80, 20, 
                 settings.position == OverlayPosition::TopRight, true);
    create_radio(hwnd, button_class, "Sinistra", ID_POS_LEFT, 190, 45, 80, 20,
                 settings.position == OverlayPosition::TopLeft, false);
    
    // Color
    create_label(hwnd, static_class, "Colore:", 10, 75, 80, 20);
    create_radio(hwnd, button_class, "Bianco", ID_COLOR_WHITE, 100, 75, 80, 20,
                 settings.fps_color == FpsColor::White, true);
    create_radio(hwnd, button_class, "Verde", ID_COLOR_GREEN, 190, 75, 80, 20,
                 settings.fps_color == FpsColor::Green, false);
    
    // Size
    create_label(hwnd, static_class, "Dimensione:", 10, 105, 80, 20);
    create_radio(hwnd, button_class, "Piccolo", ID_SIZE_SMALL, 100, 105, 65, 20,
                 settings.size == OverlaySize::Small, true);
    create_radio(hwnd, button_class, "Medio", ID_SIZE_MEDIUM, 170, 105, 55, 20,
                 settings.size == OverlaySize::Medium, false);
    create_radio(hwnd, button_class, "Grande", ID_SIZE_LARGE, 230, 105, 65, 20,
                 settings.size == OverlaySize::Large, false);
    
    // Checkboxes
    create_checkbox(hwnd, button_class, "Mostra 1% Low FPS", ID_SHOW_1LOW, 10, 145, 200, 20,
                    settings.show_1_percent_low);
    create_checkbox(hwnd, button_class, "Avvia con Windows", ID_STARTUP, 10, 175, 200, 20,
                    settings.start_with_windows);
    
    // Buttons
    let _ = CreateWindowExW(
        WINDOW_EX_STYLE::default(),
        button_class,
        windows::core::w!("Salva"),
        WS_CHILD | WS_VISIBLE | WINDOW_STYLE(BS_PUSHBUTTON as u32),
        70, 220, 80, 30,
        hwnd, HMENU(ID_SAVE as _), None, None,
    );
    
    let _ = CreateWindowExW(
        WINDOW_EX_STYLE::default(),
        button_class,
        windows::core::w!("Annulla"),
        WS_CHILD | WS_VISIBLE | WINDOW_STYLE(BS_PUSHBUTTON as u32),
        170, 220, 80, 30,
        hwnd, HMENU(ID_CANCEL as _), None, None,
    );
}

unsafe fn create_label(hwnd: HWND, class: PCWSTR, text: &str, x: i32, y: i32, w: i32, h: i32) {
    let text_wide: Vec<u16> = text.encode_utf16().chain(std::iter::once(0)).collect();
    let _ = CreateWindowExW(
        WINDOW_EX_STYLE::default(),
        class,
        PCWSTR(text_wide.as_ptr()),
        WS_CHILD | WS_VISIBLE,
        x, y, w, h,
        hwnd, None, None, None,
    );
}

unsafe fn create_radio(hwnd: HWND, class: PCWSTR, text: &str, id: i32, x: i32, y: i32, w: i32, h: i32, checked: bool, group: bool) {
    let text_wide: Vec<u16> = text.encode_utf16().chain(std::iter::once(0)).collect();
    let style = if group {
        WS_CHILD | WS_VISIBLE | WINDOW_STYLE(BS_AUTORADIOBUTTON as u32) | WS_GROUP
    } else {
        WS_CHILD | WS_VISIBLE | WINDOW_STYLE(BS_AUTORADIOBUTTON as u32)
    };
    
    if let Ok(ctrl) = CreateWindowExW(
        WINDOW_EX_STYLE::default(),
        class,
        PCWSTR(text_wide.as_ptr()),
        style,
        x, y, w, h,
        hwnd, HMENU(id as _), None, None,
    ) {
        if checked {
            SendMessageW(ctrl, BM_SETCHECK, WPARAM(BST_CHECKED_VAL), LPARAM(0));
        }
    }
}

unsafe fn create_checkbox(hwnd: HWND, class: PCWSTR, text: &str, id: i32, x: i32, y: i32, w: i32, h: i32, checked: bool) {
    let text_wide: Vec<u16> = text.encode_utf16().chain(std::iter::once(0)).collect();
    
    if let Ok(ctrl) = CreateWindowExW(
        WINDOW_EX_STYLE::default(),
        class,
        PCWSTR(text_wide.as_ptr()),
        WS_CHILD | WS_VISIBLE | WINDOW_STYLE(BS_AUTOCHECKBOX as u32),
        x, y, w, h,
        hwnd, HMENU(id as _), None, None,
    ) {
        if checked {
            SendMessageW(ctrl, BM_SETCHECK, WPARAM(BST_CHECKED_VAL), LPARAM(0));
        }
    }
}

unsafe fn is_checked(hwnd: HWND, id: i32) -> bool {
    if let Ok(ctrl) = GetDlgItem(hwnd, id) {
        SendMessageW(ctrl, BM_GETCHECK, WPARAM(0), LPARAM(0)).0 == BST_CHECKED_VAL as isize
    } else {
        false
    }
}

unsafe fn save_settings(hwnd: HWND) {
    let mut settings = Settings::default();
    
    settings.position = if is_checked(hwnd, ID_POS_LEFT) {
        OverlayPosition::TopLeft
    } else {
        OverlayPosition::TopRight
    };
    
    settings.fps_color = if is_checked(hwnd, ID_COLOR_GREEN) {
        FpsColor::Green
    } else {
        FpsColor::White
    };
    
    settings.size = if is_checked(hwnd, ID_SIZE_SMALL) {
        OverlaySize::Small
    } else if is_checked(hwnd, ID_SIZE_LARGE) {
        OverlaySize::Large
    } else {
        OverlaySize::Medium
    };
    
    settings.show_1_percent_low = is_checked(hwnd, ID_SHOW_1LOW);
    settings.start_with_windows = is_checked(hwnd, ID_STARTUP);
    
    let _ = settings.save();
    let _ = settings.set_startup_registry();
    
    SAVE_CALLBACK.with(|c| {
        if let Some(callback) = c.borrow_mut().take() {
            callback(settings);
        }
    });
}

unsafe extern "system" fn settings_wndproc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match msg {
        WM_CREATE => {
            create_controls(hwnd);
            LRESULT(0)
        }
        WM_CTLCOLORSTATIC | WM_CTLCOLORBTN => {
            let hdc = HDC(wparam.0 as _);
            SetTextColor(hdc, windows::Win32::Foundation::COLORREF(0xFFFFFF));
            SetBkColor(hdc, windows::Win32::Foundation::COLORREF(0x000000));
            LRESULT(GetStockObject(BLACK_BRUSH).0 as _)
        }
        WM_COMMAND => {
            let id = (wparam.0 & 0xFFFF) as i32;
            match id {
                ID_SAVE => {
                    save_settings(hwnd);
                    let _ = DestroyWindow(hwnd);
                }
                ID_CANCEL => {
                    let _ = DestroyWindow(hwnd);
                }
                _ => {}
            }
            LRESULT(0)
        }
        WM_DESTROY => {
            PostQuitMessage(0);
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}
