//! Notification-area icon (V = Vietnamese, E = English, – = paused) and its
//! menu. Left click toggles Vietnamese/English, like Unikey.
//!
//! The hook never touches the tray or the disk itself (Shell_NotifyIcon talks
//! to Explorer and could stall the hook): it posts [`WM_CHANGED`] and this
//! window saves the settings and redraws the icon.

use std::cell::Cell;

use windows::core::{w, Result, HSTRING, PCWSTR};
use windows::Win32::Foundation::{COLORREF, HWND, LPARAM, LRESULT, POINT, RECT, TRUE, WPARAM};
use windows::Win32::Graphics::Gdi::{
    CreateBitmap, CreateCompatibleBitmap, CreateCompatibleDC, CreateFontW, CreateSolidBrush,
    DeleteDC, DeleteObject, DrawTextW, FillRect, GetDC, ReleaseDC, SelectObject, SetBkMode,
    SetTextColor, CLEARTYPE_QUALITY, CLIP_DEFAULT_PRECIS, DEFAULT_CHARSET, DT_CENTER,
    DT_SINGLELINE, DT_VCENTER, FF_SWISS, FW_BOLD, OUT_DEFAULT_PRECIS, TRANSPARENT, VARIABLE_PITCH,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Shell::{
    Shell_NotifyIconW, NIF_ICON, NIF_MESSAGE, NIF_TIP, NIM_ADD, NIM_DELETE, NIM_MODIFY,
    NOTIFYICONDATAW,
};
use windows::Win32::UI::WindowsAndMessaging::{
    AppendMenuW, CreateIconIndirect, CreatePopupMenu, CreateWindowExW, DefWindowProcW,
    DestroyMenu, GetCursorPos, PostMessageW, PostQuitMessage, RegisterClassW,
    RegisterWindowMessageW, SetForegroundWindow, TrackPopupMenu, HICON, ICONINFO, MF_CHECKED,
    MF_SEPARATOR, MF_STRING, MF_UNCHECKED, TPM_BOTTOMALIGN, TPM_RIGHTBUTTON, WINDOW_EX_STYLE,
    WM_APP, WM_COMMAND, WM_CONTEXTMENU, WM_LBUTTONDBLCLK, WM_LBUTTONUP, WM_NULL, WM_RBUTTONUP, WNDCLASSW,
    WS_OVERLAPPED,
};

use crate::{hook, settings};

const WM_TRAY: u32 = WM_APP + 1;
/// Settings changed: save them and redraw the icon.
const WM_CHANGED: u32 = WM_APP + 2;

const ID_VIETNAMESE: usize = 1;
const ID_CORRECTIONS: usize = 2;
const ID_PAUSED: usize = 3;
const ID_AUTOSTART: usize = 4;
const ID_JOURNAL: usize = 5;
const ID_CODE_ENGLISH: usize = 6;
const ID_GUARD: usize = 7;
const ID_RESTORE: usize = 8;
const ID_PERSONAL: usize = 10;
const ID_EXIT: usize = 9;
const ID_SETTINGS: usize = 11;
const ID_EDITS: usize = 12;
const ID_HARD: usize = 13;
const ID_DELAYED: usize = 14;

thread_local! {
    static WINDOW: Cell<HWND> = Cell::new(HWND::default());
    static ICONS: Cell<[HICON; 3]> = Cell::new([HICON::default(); 3]);
    /// Explorer broadcasts this after restarting: the icon must be re-added.
    static TASKBAR_CREATED: Cell<u32> = const { Cell::new(0) };
}

/// Creates the hidden window and the icon. Call on the hook thread.
pub fn create() -> Result<()> {
    unsafe {
        let instance = GetModuleHandleW(None)?.into();
        let class = w!("AutoCorrectTray");
        let wc = WNDCLASSW {
            lpfnWndProc: Some(window_proc),
            hInstance: instance,
            lpszClassName: class,
            ..Default::default()
        };
        RegisterClassW(&wc);
        let hwnd = CreateWindowExW(
            WINDOW_EX_STYLE(0),
            class,
            w!("AutoCorrect"),
            WS_OVERLAPPED,
            0,
            0,
            0,
            0,
            None,
            None,
            Some(instance),
            None,
        )?;
        WINDOW.with(|w| w.set(hwnd));
        TASKBAR_CREATED.with(|t| t.set(RegisterWindowMessageW(w!("TaskbarCreated"))));
        ICONS.with(|i| {
            i.set([
                letter_icon("V", COLORREF(0x0030_30D0)), // red, like Unikey
                letter_icon("E", COLORREF(0x00C0_6030)), // blue
                letter_icon("–", COLORREF(0x0080_8080)), // grey: paused
            ])
        });
        let _ = Shell_NotifyIconW(NIM_ADD, &notify_data());
    }
    Ok(())
}

pub fn remove() {
    unsafe {
        let _ = Shell_NotifyIconW(NIM_DELETE, &notify_data());
    }
}

/// Asks the tray window to save settings and redraw (safe from the hook).
pub fn changed() {
    let hwnd = WINDOW.with(Cell::get);
    unsafe {
        let _ = PostMessageW(Some(hwnd), WM_CHANGED, WPARAM(0), LPARAM(0));
    }
}

fn notify_data() -> NOTIFYICONDATAW {
    let s = hook::settings();
    let [vi, en, paused] = ICONS.with(Cell::get);
    let (icon, tip) = if s.paused {
        (paused, "AutoCorrect – tạm dừng".to_string())
    } else if s.vietnamese {
        (vi, format!("AutoCorrect – Tiếng Việt ({})", s.hotkey))
    } else {
        (en, format!("AutoCorrect – English ({})", s.hotkey))
    };
    let mut data = NOTIFYICONDATAW {
        cbSize: std::mem::size_of::<NOTIFYICONDATAW>() as u32,
        hWnd: WINDOW.with(Cell::get),
        uID: 1,
        uFlags: NIF_ICON | NIF_MESSAGE | NIF_TIP,
        uCallbackMessage: WM_TRAY,
        hIcon: icon,
        ..Default::default()
    };
    for (dst, src) in data.szTip.iter_mut().zip(tip.encode_utf16()) {
        *dst = src;
    }
    data
}

unsafe extern "system" fn window_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        WM_TRAY => match lparam.0 as u32 {
            WM_LBUTTONUP => hook::update(|s| s.vietnamese = !s.vietnamese),
            WM_LBUTTONDBLCLK => settings::open_settings_window(),
            WM_RBUTTONUP | WM_CONTEXTMENU => show_menu(hwnd),
            _ => {}
        },
        WM_CHANGED => {
            settings::save(&hook::settings());
            let _ = Shell_NotifyIconW(NIM_MODIFY, &notify_data());
        }
        WM_COMMAND => match wparam.0 & 0xFFFF {
            ID_VIETNAMESE => hook::update(|s| s.vietnamese = !s.vietnamese),
            ID_CORRECTIONS => hook::update(|s| s.corrections = !s.corrections),
            ID_PAUSED => hook::update(|s| s.paused = !s.paused),
            ID_JOURNAL => hook::update(|s| s.journal = !s.journal),
            ID_EDITS => hook::update(|s| s.journal_edits = !s.journal_edits),
            ID_HARD => hook::update(|s| s.journal_hard = !s.journal_hard),
            ID_DELAYED => hook::update(|s| s.delayed = !s.delayed),
            ID_CODE_ENGLISH => hook::update(|s| s.code_english = !s.code_english),
            ID_GUARD => hook::update(|s| s.autocomplete_guard = !s.autocomplete_guard),
            ID_RESTORE => hook::update(|s| s.restore_marks = !s.restore_marks),
            ID_PERSONAL => settings::open_personal(),
            ID_SETTINGS => settings::open_settings_window(),
            ID_AUTOSTART => settings::set_autostart(!settings::autostart()),
            ID_EXIT => PostQuitMessage(0),
            _ => {}
        },
        m if m != 0 && m == TASKBAR_CREATED.with(Cell::get) => {
            let _ = Shell_NotifyIconW(NIM_ADD, &notify_data());
        }
        _ => return DefWindowProcW(hwnd, msg, wparam, lparam),
    }
    LRESULT(0)
}

unsafe fn show_menu(hwnd: HWND) {
    let s = hook::settings();
    let Ok(menu) = CreatePopupMenu() else { return };
    let check = |on: bool| if on { MF_CHECKED } else { MF_UNCHECKED };
    let vietnamese = HSTRING::from(format!("Tiếng Việt (Telex)\t{}", s.hotkey));
    let items: [(usize, PCWSTR, bool); 11] = [
        (ID_VIETNAMESE, PCWSTR(vietnamese.as_ptr()), s.vietnamese),
        (ID_CORRECTIONS, w!("Tự sửa lỗi gõ"), s.corrections),
        (ID_PAUSED, w!("Tạm dừng"), s.paused),
        (ID_AUTOSTART, w!("Khởi động cùng Windows"), settings::autostart()),
        (ID_CODE_ENGLISH, w!("Sửa lỗi tiếng Anh cả trong IDE/terminal"), s.code_english),
        (ID_RESTORE, w!("Tự thêm dấu khi gõ không dấu (khong → không)"), s.restore_marks),
        (ID_DELAYED, w!("Sửa muộn khi đã gõ từ kế tiếp (thử nghiệm)"), s.delayed),
        (ID_GUARD, w!("Chống lỗi gợi ý trong trình duyệt/ô tìm kiếm"), s.autocomplete_guard),
        (ID_JOURNAL, w!("Ghi nhật ký sửa lỗi (để tinh chỉnh)"), s.journal),
        (ID_EDITS, w!("Ghi chữ bạn tự sửa tay (để học)"), s.journal_edits),
        (ID_HARD, w!("Ghi ca khó app bỏ qua (để học)"), s.journal_hard),
    ];
    for (id, label, on) in items {
        let _ = AppendMenuW(menu, MF_STRING | check(on), id, label);
    }
    let _ = AppendMenuW(menu, MF_SEPARATOR, 0, PCWSTR::null());
    let _ = AppendMenuW(menu, MF_STRING, ID_SETTINGS, w!("Cài đặt..."));
    let _ = AppendMenuW(menu, MF_STRING, ID_PERSONAL, w!("Mở từ điển cá nhân..."));
    let _ = AppendMenuW(menu, MF_STRING, ID_EXIT, w!("Thoát"));

    let mut pt = POINT::default();
    let _ = GetCursorPos(&mut pt);
    // Without this the menu does not close when clicking elsewhere.
    let _ = SetForegroundWindow(hwnd);
    let _ = TrackPopupMenu(menu, TPM_RIGHTBUTTON | TPM_BOTTOMALIGN, pt.x, pt.y, Some(0), hwnd, None);
    let _ = PostMessageW(Some(hwnd), WM_NULL, WPARAM(0), LPARAM(0));
    let _ = DestroyMenu(menu);
}

/// A 32x32 icon: white `letter` on a solid `background`.
unsafe fn letter_icon(letter: &str, background: COLORREF) -> HICON {
    const SIZE: i32 = 32;
    let screen = GetDC(None);
    let dc = CreateCompatibleDC(Some(screen));
    let color = CreateCompatibleBitmap(screen, SIZE, SIZE);
    let old_bitmap = SelectObject(dc, color.into());

    let rect = RECT { left: 0, top: 0, right: SIZE, bottom: SIZE };
    let brush = CreateSolidBrush(background);
    FillRect(dc, &rect, brush);
    SetBkMode(dc, TRANSPARENT);
    SetTextColor(dc, COLORREF(0x00FF_FFFF));
    let font = CreateFontW(
        -26,
        0,
        0,
        0,
        FW_BOLD.0 as i32,
        0,
        0,
        0,
        DEFAULT_CHARSET,
        OUT_DEFAULT_PRECIS,
        CLIP_DEFAULT_PRECIS,
        CLEARTYPE_QUALITY,
        (VARIABLE_PITCH.0 | FF_SWISS.0) as u32,
        w!("Segoe UI"),
    );
    let old_font = SelectObject(dc, font.into());
    let mut text: Vec<u16> = letter.encode_utf16().collect();
    let mut text_rect = rect;
    DrawTextW(dc, &mut text, &mut text_rect, DT_CENTER | DT_VCENTER | DT_SINGLELINE);
    SelectObject(dc, old_font);
    SelectObject(dc, old_bitmap);

    // All-zero AND mask: every pixel of the colour bitmap is shown.
    let mask_bits = [0u8; (SIZE * SIZE / 8) as usize];
    let mask = CreateBitmap(SIZE, SIZE, 1, 1, Some(mask_bits.as_ptr().cast()));
    let info = ICONINFO { fIcon: TRUE, xHotspot: 0, yHotspot: 0, hbmMask: mask, hbmColor: color };
    let icon = CreateIconIndirect(&info).unwrap_or_default();

    let _ = DeleteObject(font.into());
    let _ = DeleteObject(brush.into());
    let _ = DeleteObject(mask.into());
    let _ = DeleteObject(color.into());
    let _ = DeleteDC(dc);
    ReleaseDC(None, screen);
    icon
}
