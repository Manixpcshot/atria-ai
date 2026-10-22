//! v0.18.0: minimal Windows-only FFI surface (no external crates).
//!
//! * `clipboard_text` — read CF_UNICODETEXT from the system clipboard.
//! * `flash_and_beep` — flash the taskbar icon + information beep (the
//!   "run finished while window unfocused" notification).
//! * `start_hotkey` — a message-only window + RegisterHotKey loop on its own
//!   thread; Ctrl+Alt+A toggles the main window (show/hide/focus).
//!
//! Everything is cfg-gated; on other platforms the functions report that the
//! feature is Windows-only.

/// Read up to 50k chars of text from the system clipboard.
pub fn clipboard_text() -> Result<String, String> {
    #[cfg(windows)]
    {
        const CF_UNICODETEXT: u32 = 13;
        extern "system" {
            fn OpenClipboard(hwnd: isize) -> i32;
            fn CloseClipboard() -> i32;
            fn GetClipboardData(fmt: u32) -> *mut std::os::raw::c_void;
            fn GlobalSize(h: *mut std::os::raw::c_void) -> usize;
            fn GlobalLock(h: *mut std::os::raw::c_void) -> *mut std::os::raw::c_void;
            fn GlobalUnlock(h: *mut std::os::raw::c_void) -> i32;
        }
        unsafe {
            if OpenClipboard(0) == 0 {
                return Err("براشدن کلیپ‌بورد ممکن نبود".into());
            }
            let data = GetClipboardData(CF_UNICODETEXT);
            let res: Result<String, String> = if data.is_null() {
                CloseClipboard();
                Err("کلیپ‌بورد حاوی متن نیست".into())
            } else {
                let size = GlobalSize(data);
                let ptr = GlobalLock(data);
                let t = if ptr.is_null() {
                    CloseClipboard();
                    Err("قفل حافظهٔ کلیپ‌بورد ممکن نبود".into())
                } else {
                    let n = size / 2;
                    let slice = std::slice::from_raw_parts(ptr as *const u16, n);
                    let t = String::from_utf16_lossy(slice);
                    GlobalUnlock(data);
                    Ok(t)
                };
                CloseClipboard();
                t
            };
            let t = res?;
            Ok(t.chars().take(50_000).collect())
        }
    }
    #[cfg(not(windows))]
    {
        Err("خواندن کلیپ‌بورد فقط روی Windows در دسترس است".into())
    }
}

/// Flash the taskbar icon of `hwnd` and play the information beep.
#[cfg_attr(not(windows), allow(dead_code))]
pub fn flash_and_beep(hwnd: Option<isize>) {
    #[cfg(windows)]
    {
        #[repr(C)]
        struct Fwex {
            cb: u32,
            hwnd: isize,
            flags: u32,
            count: u32,
        }
        const FLASHW_ALL: u32 = 0x3;
        const MB_ICONINFORMATION: u32 = 0x40;
        extern "system" {
            fn FlashWindowExW(p: *mut Fwex) -> i32;
            fn MessageBeep(u: u32) -> i32;
        }
        unsafe {
            if let Some(h) = hwnd {
                let mut f = Fwex {
                    cb: std::mem::size_of::<Fwex>() as u32,
                    hwnd: h,
                    flags: FLASHW_ALL,
                    count: 5,
                };
                let _ = FlashWindowExW(&mut f);
            }
            let _ = MessageBeep(MB_ICONINFORMATION);
        }
    }
    #[cfg(not(windows))]
    {
        let _ = hwnd;
    }
}

/// Register Ctrl+Alt+A on a private message-only window; on each press the
/// main window toggles (hidden <-> shown+focused).
pub fn start_hotkey(app: tauri::AppHandle) -> bool {
    #[cfg(windows)]
    {
        use tauri::Manager;
        const MOD_ALT: u32 = 0x1;
        const MOD_CONTROL: u32 = 0x2;
        const VK_A: u32 = 0x41;
        const WM_HOTKEY: u32 = 0x0312;
        const HWND_MESSAGE: isize = -3;
        #[repr(C)]
        struct Msg {
            hwnd: isize,
            message: u32,
            wparam: usize,
            lparam: usize,
            time: u32,
            x: i32,
            y: i32,
        }
        extern "system" {
            fn CreateWindowExW(
                ex: u32,
                cls: *const u16,
                name: *const u16,
                style: u32,
                x: i32,
                y: i32,
                w: i32,
                h: i32,
                parent: isize,
                menu: isize,
                inst: isize,
                param: isize,
            ) -> isize;
            fn RegisterHotKey(hwnd: isize, id: i32, modfs: u32, vk: u32) -> i32;
            fn GetMessageW(m: *mut Msg, hwnd: isize, lo: u32, hi: u32) -> i32;
            fn TranslateMessage(m: *const Msg) -> i32;
            fn DispatchMessageW(m: *const Msg) -> isize;
        }
        std::thread::Builder::new()
            .name("atria-hotkey".into())
            .spawn(move || unsafe {
                let cls: Vec<u16> = "STATIC".encode_utf16().collect();
                let hwnd = CreateWindowExW(
                    0, cls.as_ptr(), std::ptr::null(), 0, 0, 0, 0, 0, HWND_MESSAGE, 0, 0, 0,
                );
                if hwnd == 0 {
                    return;
                }
                if RegisterHotKey(hwnd, 1, MOD_CONTROL | MOD_ALT, VK_A) == 0 {
                    return;
                }
                let mut m = Msg {
                    hwnd: 0,
                    message: 0,
                    wparam: 0,
                    lparam: 0,
                    time: 0,
                    x: 0,
                    y: 0,
                };
                loop {
                    let r = GetMessageW(&mut m, 0, 0, 0);
                    if r == 0 || r == -1 {
                        break;
                    }
                    if m.message == WM_HOTKEY {
                        if let Some(w) = app.get_webview_window("main") {
                            let visible = w.is_visible().unwrap_or(true);
                            if visible {
                                let _ = w.hide();
                            } else {
                                let _ = w.show();
                                let _ = w.set_focus();
                            }
                        }
                    }
                    let _ = TranslateMessage(&m);
                    let _ = DispatchMessageW(&m);
                }
            })
            .is_ok()
    }
    #[cfg(not(windows))]
    {
        let _ = app;
        false
    }
}
