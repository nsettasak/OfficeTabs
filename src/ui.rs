//! DPI-aware native dialog fonts and control geometry, shared by Manager/Setup.
use crate::native::wide;
use std::{
    mem::zeroed,
    ptr::{null, null_mut},
};
use windows_sys::Win32::{
    Foundation::*,
    Graphics::Gdi::*,
    System::LibraryLoader::*,
    UI::{HiDpi::*, WindowsAndMessaging::*},
};
struct Scale {
    parent: HWND,
    old: i32,
    new: i32,
    font: HFONT,
    title: HFONT,
    heading: HFONT,
}
unsafe extern "system" fn child(h: HWND, data: isize) -> i32 {
    let s = &*(data as *const Scale);
    let mut r = RECT::default();
    GetWindowRect(h, &mut r);
    MapWindowPoints(null_mut(), s.parent, &mut r as *mut _ as *mut POINT, 2);
    SetWindowPos(
        h,
        null_mut(),
        r.left * s.new / s.old,
        r.top * s.new / s.old,
        (r.right - r.left) * s.new / s.old,
        (r.bottom - r.top) * s.new / s.old,
        SWP_NOACTIVATE | SWP_NOZORDER,
    );
    let role = GetPropW(h, wide("OfficeTabs.TextRole").as_ptr()) as usize;
    let font = match role {
        1 => s.title,
        2 => s.heading,
        _ => s.font,
    };
    SendMessageW(h, WM_SETFONT, font as usize, 1);
    1
}
unsafe fn scale(h: HWND, new_dpi: u32) {
    let key = wide("OfficeTabs.Dpi");
    let old = GetPropW(h, key.as_ptr()) as usize;
    if old == new_dpi as usize {
        return;
    }
    let old = if old == 0 { 96 } else { old as i32 };
    let new = (new_dpi as i32).max(96);
    let font = CreateFontW(
        -14 * new / 96,
        0,
        0,
        0,
        400,
        0,
        0,
        0,
        DEFAULT_CHARSET as u32,
        0,
        0,
        CLEARTYPE_QUALITY as u32,
        0,
        wide("Segoe UI").as_ptr(),
    );
    let title = CreateFontW(
        -26 * new / 96,
        0,
        0,
        0,
        600,
        0,
        0,
        0,
        DEFAULT_CHARSET as u32,
        0,
        0,
        CLEARTYPE_QUALITY as u32,
        0,
        wide("Segoe UI").as_ptr(),
    );
    let heading = CreateFontW(
        -20 * new / 96,
        0,
        0,
        0,
        600,
        0,
        0,
        0,
        DEFAULT_CHARSET as u32,
        0,
        0,
        CLEARTYPE_QUALITY as u32,
        0,
        wide("Segoe UI").as_ptr(),
    );
    let data = Scale {
        parent: h,
        old,
        new,
        font,
        title,
        heading,
    };
    EnumChildWindows(h, Some(child), &data as *const _ as isize);
    let fontkey = wide("OfficeTabs.Font");
    let previous = GetPropW(h, fontkey.as_ptr());
    SetPropW(h, fontkey.as_ptr(), font);
    if !previous.is_null() {
        DeleteObject(previous);
    }
    SetPropW(h, key.as_ptr(), new as usize as HANDLE);
    for (name, value) in [
        ("OfficeTabs.TitleFont", title),
        ("OfficeTabs.HeadingFont", heading),
    ] {
        let key = wide(name);
        let old = GetPropW(h, key.as_ptr());
        SetPropW(h, key.as_ptr(), value);
        if !old.is_null() {
            DeleteObject(old);
        }
    }
}
unsafe fn initialize(h: HWND, width: i32, height: i32) {
    let dpi = GetDpiForWindow(h).max(96);
    scale(h, dpi);
    let mut r: RECT = zeroed();
    GetWindowRect(h, &mut r);
    let mut client = RECT {
        left: 0,
        top: 0,
        right: width * dpi as i32 / 96,
        bottom: height * dpi as i32 / 96,
    };
    AdjustWindowRectExForDpi(
        &mut client,
        GetWindowLongW(h, GWL_STYLE) as u32,
        0,
        GetWindowLongW(h, GWL_EXSTYLE) as u32,
        dpi,
    );
    SetWindowPos(
        h,
        null_mut(),
        r.left,
        r.top,
        client.right - client.left,
        client.bottom - client.top,
        SWP_NOACTIVATE | SWP_NOZORDER,
    );
}
/// # Safety
/// `h` must be a live window created by [`create`].
pub unsafe fn destroy(h: HWND) {
    for name in ["OfficeTabs.TitleFont", "OfficeTabs.HeadingFont"] {
        let f = RemovePropW(h, wide(name).as_ptr());
        if !f.is_null() {
            DeleteObject(f);
        }
    }
    let font = RemovePropW(h, wide("OfficeTabs.Font").as_ptr());
    if !font.is_null() {
        DeleteObject(font);
    }
    RemovePropW(h, wide("OfficeTabs.Dpi").as_ptr());
}

/// # Safety
/// `h` must be a live control created by [`control`].
pub unsafe fn role(h: HWND, role: usize) {
    SetPropW(h, wide("OfficeTabs.TextRole").as_ptr(), role as HANDLE);
}
unsafe fn fill(dc: HDC, r: RECT, color: u32) {
    let brush = CreateSolidBrush(color);
    FillRect(dc, &r, brush);
    DeleteObject(brush);
}
/// Light surface matching the established OfficeTabs cards and installer chrome.
/// # Safety
/// `h` must be a live window and `dc` a device context currently being painted.
pub unsafe fn surface(h: HWND, dc: HDC, manager: bool) {
    let dpi = GetDpiForWindow(h).max(96) as i32;
    let px = |v: i32| v * dpi / 96;
    let mut r = RECT::default();
    GetClientRect(h, &mut r);
    fill(dc, r, if manager { 0x00fbf8f6 } else { 0x00ffffff });
    let footer = if manager { 414 } else { 430 };
    fill(
        dc,
        RECT {
            left: 0,
            top: px(footer),
            right: r.right,
            bottom: r.bottom,
        },
        0x00f6f1ee,
    );
    fill(
        dc,
        RECT {
            left: 0,
            top: px(footer),
            right: r.right,
            bottom: px(footer + 1),
        },
        0x00e6dfd8,
    );
    if manager {
        for (y, color) in [(118, 0x00b85c18), (248, 0x004b7c10)] {
            fill(
                dc,
                RECT {
                    left: px(30),
                    top: px(y),
                    right: px(770),
                    bottom: px(y + 116),
                },
                0x00ffffff,
            );
            fill(
                dc,
                RECT {
                    left: px(30),
                    top: px(y),
                    right: px(35),
                    bottom: px(y + 116),
                },
                color,
            );
        }
    } else {
        fill(
            dc,
            RECT {
                left: 0,
                top: 0,
                right: r.right,
                bottom: px(108),
            },
            0x00fbf8f6,
        );
        fill(
            dc,
            RECT {
                left: 0,
                top: px(108),
                right: r.right,
                bottom: px(109),
            },
            0x00e6dfd8,
        );
    }
    let icon = LoadImageW(
        windows_sys::Win32::System::LibraryLoader::GetModuleHandleW(std::ptr::null()),
        101usize as *const u16,
        IMAGE_ICON,
        px(48),
        px(48),
        0,
    );
    if !icon.is_null() {
        DrawIconEx(
            dc,
            px(30),
            px(26),
            icon,
            px(48),
            px(48),
            0,
            std::ptr::null_mut(),
            DI_NORMAL,
        );
        DestroyIcon(icon);
    }
}

/// Create a child control with the shared dialog styles (`label`, `button`, `edit`, ...).
///
/// # Safety
/// `parent` must be a live window; `class` and `text` must be valid UTF-8 strings.
pub unsafe fn control(
    parent: HWND,
    class: &str,
    text: &str,
    id: usize,
    style: u32,
    rect: (i32, i32, i32, i32),
) -> HWND {
    CreateWindowExW(
        0,
        wide(class).as_ptr(),
        wide(text).as_ptr(),
        WS_CHILD | WS_VISIBLE | style,
        rect.0,
        rect.1,
        rect.2,
        rect.3,
        parent,
        id as HMENU,
        GetModuleHandleW(null()),
        null(),
    )
}

/// Register and show one top-level window: DPI context, class, icon, client size.
/// # Safety
/// `proc` must be a valid window procedure for `class`, and `size`/`client` must be
/// positive pixel values.
pub unsafe fn create(
    class: &str,
    title: &str,
    size: (i32, i32),
    client: (i32, i32),
    proc: WNDPROC,
    icon: usize,
) -> HWND {
    SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
    let name = wide(class);
    let instance = GetModuleHandleW(null());
    let cls = WNDCLASSW {
        lpfnWndProc: proc,
        hInstance: instance,
        lpszClassName: name.as_ptr(),
        hIcon: LoadIconW(instance, icon as *const u16),
        hCursor: LoadCursorW(null_mut(), IDC_ARROW),
        hbrBackground: (COLOR_WINDOW + 1) as HBRUSH,
        ..zeroed()
    };
    RegisterClassW(&cls);
    let h = CreateWindowExW(
        0,
        name.as_ptr(),
        wide(title).as_ptr(),
        WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU | WS_MINIMIZEBOX | WS_VISIBLE | WS_CLIPCHILDREN,
        CW_USEDEFAULT,
        CW_USEDEFAULT,
        size.0,
        size.1,
        null_mut(),
        null_mut(),
        instance,
        null(),
    );
    if !h.is_null() {
        initialize(h, client.0, client.1);
    }
    h
}

/// # Safety
/// Must be called from `WM_PAINT` for `h`.
pub unsafe fn paint(h: HWND, manager: bool) {
    let mut ps = PAINTSTRUCT::default();
    let dc = BeginPaint(h, &mut ps);
    surface(h, dc, manager);
    EndPaint(h, &ps);
}

/// # Safety
/// `dc` must be the device context supplied with `WM_CTLCOLOR*`.
pub unsafe fn ctlcolor(dc: HDC, background: u32, text: u32) -> isize {
    SetBkColor(dc, background);
    SetTextColor(dc, text);
    SetDCBrushColor(dc, background);
    GetStockObject(DC_BRUSH) as isize
}

/// # Safety
/// Must be called from `WM_DPICHANGED` with that message's `WPARAM`/`LPARAM`.
pub unsafe fn dpi_changed(h: HWND, w: WPARAM, l: LPARAM) {
    scale(h, (w & 0xffff) as u32);
    let r = *(l as *const RECT);
    SetWindowPos(
        h,
        null_mut(),
        r.left,
        r.top,
        r.right - r.left,
        r.bottom - r.top,
        SWP_NOACTIVATE | SWP_NOZORDER,
    );
}

/// # Safety
/// `h` must be a live window created by [`create`].
pub unsafe fn destroy_and_quit(h: HWND) {
    destroy(h);
    PostQuitMessage(0);
}

/// # Safety
/// `h` must be a live top-level window; returns once it is destroyed.
pub unsafe fn run_loop(h: HWND) {
    let mut msg = zeroed();
    while GetMessageW(&mut msg, null_mut(), 0, 0) > 0 {
        if IsDialogMessageW(h, &msg) == 0 {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
}
