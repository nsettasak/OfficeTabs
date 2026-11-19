#![windows_subsystem = "windows"]
use officetabs::{
    native::{enabled, setting, wide},
    ui,
};
use std::ptr::null_mut;
use windows_sys::Win32::{
    Foundation::*, Graphics::Gdi::HDC, System::Registry::*, UI::WindowsAndMessaging::*,
};
unsafe fn label(h: HWND, text: &str, id: usize, x: i32, y: i32, w: i32, height: i32) -> HWND {
    ui::control(h, "STATIC", text, id, 0, (x, y, w, height))
}
unsafe fn button(h: HWND, text: &str, id: usize, x: i32, y: i32, w: i32) {
    ui::control(h, "BUTTON", text, id, WS_TABSTOP, (x, y, w, 34));
}
unsafe fn update(h: HWND) {
    for (excel, id) in [(false, 101), (true, 102)] {
        SetWindowTextW(
            GetDlgItem(h, id + 400),
            wide(if enabled(excel) {
                "Document tabs are enabled."
            } else {
                "Document tabs are disabled."
            })
            .as_ptr(),
        );
        SetWindowTextW(
            GetDlgItem(h, id),
            wide(if enabled(excel) { "Disable" } else { "Enable" }).as_ptr(),
        );
    }
}
unsafe fn set_load(excel: bool, value: u32) -> Result<(), u32> {
    let host = if excel { "Excel" } else { "Word" };
    let key = wide(&format!(
        "Software\\Microsoft\\Office\\{host}\\Addins\\IndependentOfficeTabs.{host}"
    ));
    let mut handle = null_mut();
    let status = RegOpenKeyExW(
        HKEY_CURRENT_USER,
        key.as_ptr(),
        0,
        KEY_SET_VALUE | KEY_WOW64_64KEY,
        &mut handle,
    );
    if status != 0 {
        return Err(status);
    }
    let result = RegSetValueExW(
        handle,
        wide("LoadBehavior").as_ptr(),
        0,
        REG_DWORD,
        &value as *const _ as *const u8,
        4,
    );
    RegCloseKey(handle);
    if result == 0 {
        Ok(())
    } else {
        Err(result)
    }
}
/// Short About text: name, purpose, publisher and the licence line. The full MIT
/// text stays in LICENSE.txt in the installation folder.
fn about_text() -> String {
    let licence = include_str!("../../LICENSE.txt");
    let copyright = licence
        .lines()
        .find(|line| {
            line.trim_start()
                .to_ascii_lowercase()
                .starts_with("copyright")
        })
        .unwrap_or("Copyright (c) OfficeTabs")
        .trim();
    format!(
        "OfficeTabs {}\nDocument tabs for Microsoft Word and Excel, 64-bit.\n\nPublisher: OfficeTabs\nBuilt with Rust.\n\nMIT License. {}\nFull license text: LICENSE.txt in the installation folder.",
        env!("CARGO_PKG_VERSION"),
        copyright
    )
}
unsafe extern "system" fn proc(h: HWND, m: u32, w: WPARAM, l: LPARAM) -> LRESULT {
    match m {
        WM_CREATE => {
            officetabs::ui::role(label(h, "OfficeTabs Manager", 10, 94, 22, 650, 36), 1);
            label(h, "Document tabs for Word and Excel", 11, 96, 68, 640, 26);
            for (name, base, id) in [("Word", 118, 101), ("Excel", 248, 102)] {
                officetabs::ui::role(label(h, name, id + 300, 55, base + 17, 460, 30), 2);
                label(h, "", id + 400, 55, base + 62, 490, 36);
                button(h, "", id, 590, base + 16, 153);
                button(h, "Reset settings", id + 100, 590, base + 60, 153);
            }
            label(
                h,
                "Changes are saved immediately. Restart Office after enabling tabs.",
                12,
                32,
                382,
                740,
                24,
            );
            button(h, "About", 301, 24, 426, 100);
            button(h, "OK", 302, 570, 426, 96);
            button(h, "Cancel", 303, 680, 426, 96);
            update(h);
            return 0;
        }
        WM_COMMAND => {
            match w & 0xffff {
                id @ (101 | 102) => {
                    let excel = id == 102;
                    let next = !enabled(excel);
                    let value = if next { "enabled" } else { "disabled" };
                    match set_load(excel, if next { 3 } else { 0 }) {
                        Ok(()) => {
                            if let Err(e) = setting(excel, "enabled", value) {
                                MessageBoxW(
                                    h,
                                    wide(&e.to_string()).as_ptr(),
                                    wide("OfficeTabs").as_ptr(),
                                    MB_ICONERROR,
                                );
                            }
                        }
                        Err(code) => {
                            MessageBoxW(h,wide(&format!("OfficeTabs native add-in registration was not found or could not be changed (error {code}). Install the native preview first.")).as_ptr(),wide("OfficeTabs").as_ptr(),MB_ICONERROR);
                        }
                    }
                    update(h);
                }
                id @ (201 | 202) => {
                    let excel = id == 202;
                    if let Err(e) = setting(excel, "close-warning", "warn") {
                        MessageBoxW(
                            h,
                            wide(&e.to_string()).as_ptr(),
                            wide("OfficeTabs").as_ptr(),
                            MB_ICONERROR,
                        );
                    } else {
                        MessageBoxW(h,wide("Close confirmation has been reset. Restart Office to reset session tab order and docking.").as_ptr(),wide("OfficeTabs").as_ptr(),MB_OK);
                    }
                }
                301 => {
                    MessageBoxW(
                        h,
                        wide(&about_text()).as_ptr(),
                        wide("About OfficeTabs").as_ptr(),
                        MB_OK | MB_ICONINFORMATION,
                    );
                }
                2 | 302 | 303 => {
                    DestroyWindow(h);
                }
                _ => {}
            }
            return 0;
        }
        WM_PAINT => {
            ui::paint(h, true);
            return 0;
        }
        WM_CTLCOLORSTATIC => {
            let dc = w as HDC;
            let id = GetDlgCtrlID(l as HWND);
            let background = if matches!(id, 401 | 402 | 501 | 502) {
                0x00ffffff
            } else {
                0x00fbf8f6
            };
            let text = if matches!(id, 11 | 12 | 501 | 502) {
                0x00796455
            } else {
                0x002d251d
            };
            return ui::ctlcolor(dc, background, text);
        }
        WM_CLOSE => {
            DestroyWindow(h);
            return 0;
        }
        WM_DPICHANGED => {
            ui::dpi_changed(h, w, l);
            return 0;
        }
        WM_DESTROY => {
            ui::destroy_and_quit(h);
            return 0;
        }
        _ => {}
    }
    DefWindowProcW(h, m, w, l)
}
fn main() {
    unsafe {
        let h = ui::create(
            "OfficeTabs.Rust.Manager",
            "OfficeTabs Manager",
            (580, 430),
            (800, 476),
            Some(proc),
            101,
        );
        if h.is_null() {
            return;
        }
        ui::run_loop(h);
    }
}
