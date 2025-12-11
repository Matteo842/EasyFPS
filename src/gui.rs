use crate::settings::{FpsColor, OverlayPosition, OverlaySize, Settings};
use std::sync::atomic::{AtomicBool, Ordering};
use windows::core::PCWSTR;
use windows::Win32::Foundation::{COLORREF, HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::Win32::Graphics::Gdi::*;
// Aggiungiamo l'import per il mouse
use windows::Win32::UI::Input::KeyboardAndMouse::ReleaseCapture;
use windows::Win32::UI::Controls::{
    InitCommonControlsEx, INITCOMMONCONTROLSEX, ICC_BAR_CLASSES,
    TBS_AUTOTICKS, TBS_HORZ,
};

const WM_USER: u32 = 0x0400;
const TBM_GETPOS: u32 = WM_USER;
const TBM_SETPOS: u32 = WM_USER + 5;
const TBM_SETRANGEMIN: u32 = WM_USER + 7;
const TBM_SETRANGEMAX: u32 = WM_USER + 8;

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
const ID_SHOW_CPU: i32 = 112;
const ID_SHOW_GPU: i32 = 113;
const ID_OPACITY_SLIDER: i32 = 114;
const ID_OPACITY_VAL: i32 = 115;
const ID_SAVE: i32 = 110;
const ID_CANCEL: i32 = 111;

// Custom Title Bar IDs
const ID_TITLE_BAR: i32 = 200;
const ID_CLOSE_BTN: i32 = 201;

// Button check states
const BST_CHECKED_VAL: usize = 1;

// Colors (BGR format per Windows)
const COL_BLACK: u32 = 0x000000;
const COL_DARK_GRAY: u32 = 0x2D2D2D; 
const COL_RED: u32 = 0x0000FF;       
const COL_WHITE: u32 = 0xFFFFFF;

// Definiamo manualmente le costanti mancanti per sicurezza
const SS_CENTER: u32 = 0x1;
const SS_NOTIFY: u32 = 0x100;
const SS_CENTERIMAGE: u32 = 0x200;

thread_local! {
    static CURRENT_SETTINGS: std::cell::RefCell<Option<Settings>> = std::cell::RefCell::new(None);
    static SAVE_CALLBACK: std::cell::RefCell<Option<Box<dyn FnOnce(Settings) + Send>>> = std::cell::RefCell::new(None);
    // Correzione: Usiamo std::ptr::null_mut() invece di 0
    static BRUSH_BLACK: std::cell::RefCell<HBRUSH> = std::cell::RefCell::new(HBRUSH(0));
    static BRUSH_DARK_GRAY: std::cell::RefCell<HBRUSH> = std::cell::RefCell::new(HBRUSH(0));
    static BRUSH_RED: std::cell::RefCell<HBRUSH> = std::cell::RefCell::new(HBRUSH(0));
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
    let icc = INITCOMMONCONTROLSEX {
        dwSize: std::mem::size_of::<INITCOMMONCONTROLSEX>() as u32,
        dwICC: ICC_BAR_CLASSES,
    };
    let _ = InitCommonControlsEx(&icc);

    let class_name = windows::core::w!("EasyFPS_Settings");
    
    let wc = WNDCLASSEXW {
        cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
        style: CS_HREDRAW | CS_VREDRAW,
        lpfnWndProc: Some(settings_wndproc),
        hbrBackground: CreateSolidBrush(COLORREF(COL_BLACK)),
        lpszClassName: class_name,
        ..Default::default()
    };
    
    RegisterClassExW(&wc);
    
    // Inizializza i pennelli
    BRUSH_BLACK.with(|b| *b.borrow_mut() = CreateSolidBrush(COLORREF(COL_BLACK)));
    BRUSH_DARK_GRAY.with(|b| *b.borrow_mut() = CreateSolidBrush(COLORREF(COL_DARK_GRAY)));
    BRUSH_RED.with(|b| *b.borrow_mut() = CreateSolidBrush(COLORREF(COL_RED)));

    // Calcolo posizione centrale schermo
    let screen_w = GetSystemMetrics(SM_CXSCREEN);
    let screen_h = GetSystemMetrics(SM_CYSCREEN);
    let win_w = 360; 
    let win_h = 400; // Increased height for Opacity Slider
    let pos_x = (screen_w - win_w) / 2;
    let pos_y = (screen_h - win_h) / 2;

    let hwnd = CreateWindowExW(
        WS_EX_TOPMOST,
        class_name,
        windows::core::w!("EasyFPS"),
        WS_POPUP | WS_VISIBLE | WS_BORDER, 
        pos_x, pos_y,
        win_w, win_h,
        None, None, None, None,
    );
    
    if hwnd.0 != 0 {
        let _ = ShowWindow(hwnd, SW_SHOW);
        let _ = UpdateWindow(hwnd);
        
        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }

    // Pulizia pennelli alla chiusura
    let _ = BRUSH_BLACK.with(|b| DeleteObject(*b.borrow()));
    let _ = BRUSH_DARK_GRAY.with(|b| DeleteObject(*b.borrow()));
    let _ = BRUSH_RED.with(|b| DeleteObject(*b.borrow()));
}

unsafe fn create_controls(hwnd: HWND) {
    let settings = CURRENT_SETTINGS.with(|s| s.borrow().clone().unwrap_or_default());
    
    let button_class = windows::core::w!("BUTTON");
    let static_class = windows::core::w!("STATIC");
    
    // --- CUSTOM TITLE BAR ---
    let _ = CreateWindowExW(
        WINDOW_EX_STYLE::default(),
        static_class,
        windows::core::w!("   EasyFPS - Options"), 
        WS_CHILD | WS_VISIBLE | WINDOW_STYLE(SS_CENTERIMAGE),
        0, 0, 360, 30, 
        hwnd, HMENU(ID_TITLE_BAR as _), None, None,
    );

    // Pulsante X Rosso
    let _ = CreateWindowExW(
        WINDOW_EX_STYLE::default(),
        static_class,
        windows::core::w!("✕"),
        WS_CHILD | WS_VISIBLE | WINDOW_STYLE(SS_CENTER | SS_NOTIFY | SS_CENTERIMAGE),
        360 - 30, 0, 30, 30, 
        hwnd, HMENU(ID_CLOSE_BTN as _), None, None,
    );

    let offset_y = 35; 

    // Position
    create_label(hwnd, static_class, "Position:", 20, 10 + offset_y, 80, 20);
    create_radio(hwnd, button_class, "Right", ID_POS_RIGHT, 110, 10 + offset_y, 80, 20, 
                 settings.position == OverlayPosition::TopRight, true);
    create_radio(hwnd, button_class, "Left", ID_POS_LEFT, 200, 10 + offset_y, 80, 20,
                 settings.position == OverlayPosition::TopLeft, false);
    
    // Color
    create_label(hwnd, static_class, "Color:", 20, 40 + offset_y, 80, 20);
    create_radio(hwnd, button_class, "White", ID_COLOR_WHITE, 110, 40 + offset_y, 80, 20,
                 settings.fps_color == FpsColor::White, true);
    create_radio(hwnd, button_class, "Green", ID_COLOR_GREEN, 200, 40 + offset_y, 80, 20,
                 settings.fps_color == FpsColor::Green, false);
    
    // Size (CORRETTO QUI)
    create_label(hwnd, static_class, "Size:", 20, 70 + offset_y, 80, 20);
    
    // Small: invariato
    create_radio(hwnd, button_class, "Small", ID_SIZE_SMALL, 110, 70 + offset_y, 65, 20,
                 settings.size == OverlaySize::Small, true);
                 
    // Medium: Spostato leggermente e allargato (da 75 a 85px di larghezza)
    create_radio(hwnd, button_class, "Medium", ID_SIZE_MEDIUM, 180, 70 + offset_y, 85, 20,
                 settings.size == OverlaySize::Medium, false);
                 
    // Large: Spostato più a destra (da 260 a 270) per non sovrapporsi a Medium
    create_radio(hwnd, button_class, "Large", ID_SIZE_LARGE, 270, 70 + offset_y, 70, 20,
                 settings.size == OverlaySize::Large, false);
    
    // Checkboxes
    create_checkbox(hwnd, button_class, "Show 1% Low FPS", ID_SHOW_1LOW, 20, 110 + offset_y, 200, 20,
                     settings.show_1_percent_low);
    create_checkbox(hwnd, button_class, "Show CPU Usage", ID_SHOW_CPU, 20, 140 + offset_y, 200, 20,
                     settings.show_cpu_usage);
    create_checkbox(hwnd, button_class, "Show GPU Usage", ID_SHOW_GPU, 20, 170 + offset_y, 200, 20,
                     settings.show_gpu_usage);
    create_checkbox(hwnd, button_class, "Start with Windows", ID_STARTUP, 20, 200 + offset_y, 200, 20,
                     settings.start_with_windows);
    
    // Opacity Slider
    create_label(hwnd, static_class, "Opacity:", 20, 230 + offset_y, 60, 20);
    // Range 40-100
    create_trackbar(hwnd, ID_OPACITY_SLIDER, 90, 230 + offset_y, 200, 30, settings.overlay_opacity);
    
    // Opacity Value Label
    let val_str = format!("{}%", settings.overlay_opacity);
    let val_wide: Vec<u16> = val_str.encode_utf16().chain(std::iter::once(0)).collect();
    let _ = CreateWindowExW(
        WINDOW_EX_STYLE::default(),
        static_class,
        PCWSTR(val_wide.as_ptr()),
        WS_CHILD | WS_VISIBLE,
        300, 230 + offset_y, 40, 20,
        hwnd, HMENU(ID_OPACITY_VAL as _), None, None,
    );

    // Buttons
    let _ = CreateWindowExW(
        WINDOW_EX_STYLE::default(),
        button_class,
        windows::core::w!("Save"),
        WS_CHILD | WS_VISIBLE | WINDOW_STYLE(BS_PUSHBUTTON as u32),
        80, 280 + offset_y, 90, 30, // Lowered y position
        hwnd, HMENU(ID_SAVE as _), None, None,
    );
    
    let _ = CreateWindowExW(
        WINDOW_EX_STYLE::default(),
        button_class,
        windows::core::w!("Cancel"),
        WS_CHILD | WS_VISIBLE | WINDOW_STYLE(BS_PUSHBUTTON as u32),
        190, 280 + offset_y, 90, 30, // Lowered y position
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
    
    let ctrl = CreateWindowExW(
        WINDOW_EX_STYLE::default(),
        class,
        PCWSTR(text_wide.as_ptr()),
        style,
        x, y, w, h,
        hwnd, HMENU(id as _), None, None,
    );

    if ctrl.0 != 0 {
        if checked {
            SendMessageW(ctrl, BM_SETCHECK, WPARAM(BST_CHECKED_VAL), LPARAM(0));
        }
    }
}

unsafe fn create_checkbox(hwnd: HWND, class: PCWSTR, text: &str, id: i32, x: i32, y: i32, w: i32, h: i32, checked: bool) {
    let text_wide: Vec<u16> = text.encode_utf16().chain(std::iter::once(0)).collect();
    
    let ctrl = CreateWindowExW(
        WINDOW_EX_STYLE::default(),
        class,
        PCWSTR(text_wide.as_ptr()),
        WS_CHILD | WS_VISIBLE | WINDOW_STYLE(BS_AUTOCHECKBOX as u32),
        x, y, w, h,
        hwnd, HMENU(id as _), None, None,
    );

    if ctrl.0 != 0 {
        if checked {
            SendMessageW(ctrl, BM_SETCHECK, WPARAM(BST_CHECKED_VAL), LPARAM(0));
        }
    }
}

unsafe fn is_checked(hwnd: HWND, id: i32) -> bool {
    let ctrl = GetDlgItem(hwnd, id);
    if ctrl.0 != 0 {
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
    settings.show_cpu_usage = is_checked(hwnd, ID_SHOW_CPU);
    settings.show_gpu_usage = is_checked(hwnd, ID_SHOW_GPU);
    settings.start_with_windows = is_checked(hwnd, ID_STARTUP);
    settings.overlay_opacity = get_trackbar_pos(hwnd, ID_OPACITY_SLIDER);
    
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
        WM_LBUTTONDOWN => {
            let _ = ReleaseCapture(); // <--- Corretto con let _ =
            SendMessageW(hwnd, WM_NCLBUTTONDOWN, WPARAM(HTCAPTION as _), LPARAM(0));
            LRESULT(0)
        }
        WM_CTLCOLORSTATIC | WM_CTLCOLORBTN => {
            let ctrl_id = GetDlgCtrlID(HWND(lparam.0 as isize));
            let hdc = HDC(wparam.0 as _);
            
            if ctrl_id == ID_CLOSE_BTN {
                SetTextColor(hdc, COLORREF(COL_WHITE));
                SetBkColor(hdc, COLORREF(COL_RED));
                let brush = BRUSH_RED.with(|b| *b.borrow());
                return LRESULT(brush.0 as _);
            } else if ctrl_id == ID_TITLE_BAR {
                SetTextColor(hdc, COLORREF(COL_WHITE));
                SetBkColor(hdc, COLORREF(COL_DARK_GRAY));
                let brush = BRUSH_DARK_GRAY.with(|b| *b.borrow());
                return LRESULT(brush.0 as _);
            } else {
                SetTextColor(hdc, COLORREF(COL_WHITE));
                SetBkColor(hdc, COLORREF(COL_BLACK));
                let brush = BRUSH_BLACK.with(|b| *b.borrow());
                return LRESULT(brush.0 as _);
            }
        }
        WM_COMMAND => {
            let id = (wparam.0 & 0xFFFF) as i32;
            if id == ID_CLOSE_BTN {
                 let _ = DestroyWindow(hwnd);
                 return LRESULT(0);
            }

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
        WM_HSCROLL => {
            if lparam.0 != 0 {
                let ctrl_hwnd = HWND(lparam.0 as isize);
                let ctrl_id = GetDlgCtrlID(ctrl_hwnd);
                
                if ctrl_id == ID_OPACITY_SLIDER {
                     let pos = SendMessageW(ctrl_hwnd, TBM_GETPOS, WPARAM(0), LPARAM(0)).0;
                     
                     let val_str = format!("{}%", pos);
                     let val_wide: Vec<u16> = val_str.encode_utf16().chain(std::iter::once(0)).collect();
                     
                     let label_hwnd = GetDlgItem(hwnd, ID_OPACITY_VAL);
                     if label_hwnd.0 != 0 {
                         let _ = SetWindowTextW(label_hwnd, PCWSTR(val_wide.as_ptr()));
                     }
                }
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

unsafe fn create_trackbar(hwnd: HWND, id: i32, x: i32, y: i32, w: i32, h: i32, value: u8) {
    let trackbar_class = windows::core::w!("msctls_trackbar32");
    
    let ctrl = CreateWindowExW(
        WINDOW_EX_STYLE::default(),
        trackbar_class,
        windows::core::w!("Scale"),
        WS_CHILD | WS_VISIBLE | WINDOW_STYLE(TBS_AUTOTICKS | TBS_HORZ),
        x, y, w, h,
        hwnd, HMENU(id as _), None, None,
    );
    
    if ctrl.0 != 0 {
        // Range 40-100
        SendMessageW(ctrl, TBM_SETRANGEMIN, WPARAM(1), LPARAM(40));
        SendMessageW(ctrl, TBM_SETRANGEMAX, WPARAM(1), LPARAM(100));
        SendMessageW(ctrl, TBM_SETPOS, WPARAM(1), LPARAM(value as isize));
    }
}

unsafe fn get_trackbar_pos(hwnd: HWND, id: i32) -> u8 {
    let ctrl = GetDlgItem(hwnd, id);
    if ctrl.0 != 0 {
        let val = SendMessageW(ctrl, TBM_GETPOS, WPARAM(0), LPARAM(0)).0;
        val as u8
    } else {
        100 // default
    }
}