#![windows_subsystem = "windows"]
//! The setup window: it draws the dialog, collects input, and hands the work to
//! `install`. What it asks about this machine lives in `registry`.
mod install;
mod registry;

use officetabs::{native::wide, ui};
use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    thread,
};
use windows_sys::Win32::{
    Foundation::*,
    Graphics::Gdi::HDC,
    UI::{Controls::*, Input::KeyboardAndMouse::EnableWindow, WindowsAndMessaging::*},
};
include!(concat!(env!("OUT_DIR"), "/payload.rs"));
use install::{run, Shared, Status, MARQUEE, SHARED};
use registry::{close_office, office_running};

unsafe fn text(h: HWND, id: i32, value: &str) {
    let control = GetDlgItem(h, id);
    let mut old = vec![0u16; GetWindowTextLengthW(control) as usize + 1];
    GetWindowTextW(control, old.as_mut_ptr(), old.len() as i32);
    let value = wide(value);
    if old != value {
        SetWindowTextW(control, value.as_ptr());
    }
}
unsafe extern "system" fn proc(h: HWND, m: u32, w: WPARAM, l: LPARAM) -> LRESULT {
    match m {
        WM_CREATE => {
            ui::control(h, "STATIC", "OfficeTabs Setup", 10, 0, (96, 24, 560, 36));
            ui::role(GetDlgItem(h, 10), 1);
            ui::control(
                h,
                "STATIC",
                "Document tabs for Word and Excel",
                11,
                0,
                (98, 68, 560, 28),
            );
            ui::control(
                h,
                "STATIC",
                "Review the license, then choose Next to continue.",
                12,
                0,
                (32, 132, 636, 44),
            );
            ui::control(
                h,
                "EDIT",
                include_str!("../../../LICENSE.txt"),
                13,
                ES_MULTILINE as u32 | ES_READONLY as u32 | WS_VSCROLL | WS_BORDER | WS_TABSTOP,
                (32, 184, 636, 216),
            );
            ui::control(h, "STATIC", "", 16, 0, (32, 196, 636, 28));
            ShowWindow(GetDlgItem(h, 16), SW_HIDE);
            ui::control(
                h,
                "msctls_progress32",
                "",
                14,
                PBS_SMOOTH | PBS_MARQUEE,
                (32, 232, 636, 20),
            );
            ShowWindow(GetDlgItem(h, 14), SW_HIDE);
            ui::control(h, "STATIC", "", 15, 0, (32, 266, 636, 140));
            ShowWindow(GetDlgItem(h, 15), SW_HIDE);
            ui::control(h, "BUTTON", "Back", 22, WS_TABSTOP, (340, 446, 100, 32));
            EnableWindow(GetDlgItem(h, 22), 0);
            ui::control(
                h,
                "BUTTON",
                "Next",
                20,
                WS_TABSTOP | BS_DEFPUSHBUTTON as u32,
                (452, 446, 100, 32),
            );
            ui::control(h, "BUTTON", "Cancel", 21, WS_TABSTOP, (564, 446, 104, 32));
            SetTimer(h, 1, 100, None);
            return 0;
        }
        WM_COMMAND => {
            let shared = SHARED.get().unwrap();
            let phase = GetWindowLongPtrW(h, GWLP_USERDATA);
            match w & 0xffff {
                20 => {
                    if phase == 0 {
                        SetWindowLongPtrW(h, GWLP_USERDATA, 1);
                        text(h, 10, "Ready to install OfficeTabs");
                        text(h, 11, "Publisher: OfficeTabs • Word and Excel (64-bit)");
                        text(h,12,"The native preview will replace the previous OfficeTabs installation. Close Office before continuing.");
                        text(h, 20, "Install");
                        ShowWindow(GetDlgItem(h, 13), SW_HIDE);
                        text(
                            h,
                            15,
                            "Components\r\nTabs for Word\r\nTabs for Excel\r\nOfficeTabs Manager",
                        );
                        ShowWindow(GetDlgItem(h, 15), SW_SHOW);
                        EnableWindow(GetDlgItem(h, 22), 1);
                    } else if phase == 1 {
                        if office_running() {
                            let message = "Word, Excel or OfficeTabs Manager is still open.\n\nSetup can ask Office to close so it can save or cancel your documents. Save your work first.\n\nClose Office and continue?";
                            let answer = MessageBoxW(
                                h,
                                wide(message).as_ptr(),
                                wide("Close Office to continue").as_ptr(),
                                MB_ICONWARNING | MB_OKCANCEL | MB_DEFBUTTON1,
                            );
                            if answer != IDOK {
                                return 0;
                            }
                            text(
                                h,
                                12,
                                "Waiting for Office to close. Answer any save prompt.",
                            );
                            if !close_office() {
                                MessageBoxW(
                                    h,
                                    wide("Office is still running. Save and close Word, Excel and OfficeTabs Manager, then choose Install again.")
                                        .as_ptr(),
                                    wide("Office is still running").as_ptr(),
                                    MB_ICONERROR | MB_OK,
                                );
                                text(h, 12, "The native preview will replace the previous OfficeTabs installation. Close Office before continuing.");
                                return 0;
                            }
                        }
                        SetWindowLongPtrW(h, GWLP_USERDATA, 2);
                        let removing =
                            std::env::args().any(|a| a == "--worker-remove" || a == "--uninstall");
                        text(
                            h,
                            10,
                            if removing {
                                "Removing OfficeTabs"
                            } else {
                                "Installing OfficeTabs"
                            },
                        );
                        ShowWindow(GetDlgItem(h, 22), SW_HIDE);
                        ShowWindow(GetDlgItem(h, 13), SW_HIDE);
                        text(h, 11, "Preparing installation...");
                        text(h, 12, "");
                        ShowWindow(GetDlgItem(h, 14), SW_SHOW);
                        SendMessageW(GetDlgItem(h, 14), PBM_SETMARQUEE, 1, 30);
                        text(h, 16, "Starting Windows Installer...");
                        ShowWindow(GetDlgItem(h, 16), SW_SHOW);
                        text(h, 15, "");
                        ShowWindow(GetDlgItem(h, 15), SW_SHOW);
                        SendMessageW(GetDlgItem(h, 14), PBM_SETRANGE32, 0, 100);
                        EnableWindow(GetDlgItem(h, 20), 0);
                        let clone = shared.clone();
                        let uninstall =
                            std::env::args().any(|a| a == "--uninstall" || a == "--worker-remove");
                        thread::spawn(move || run(clone, uninstall));
                    } else if phase == 3 {
                        DestroyWindow(h);
                    }
                }
                22 if phase == 1 => {
                    SetWindowLongPtrW(h, GWLP_USERDATA, 0);
                    text(h, 10, "OfficeTabs Setup");
                    text(h, 11, "Document tabs for Word and Excel");
                    text(h, 12, "Review the license, then choose Next to continue.");
                    text(h, 20, "Next");
                    ShowWindow(GetDlgItem(h, 13), SW_SHOW);
                    ShowWindow(GetDlgItem(h, 15), SW_HIDE);
                    ShowWindow(GetDlgItem(h, 16), SW_HIDE);
                    ShowWindow(GetDlgItem(h, 14), SW_HIDE);
                    SendMessageW(GetDlgItem(h, 14), PBM_SETMARQUEE, 0, 0);
                    EnableWindow(GetDlgItem(h, 22), 0);
                }
                2 | 21 => {
                    if phase == 2 {
                        shared.cancel.store(true, Ordering::Relaxed);
                        text(h, 21, "Cancelling...");
                        EnableWindow(GetDlgItem(h, 21), 0);
                    } else {
                        DestroyWindow(h);
                    }
                }
                _ => {}
            }
            return 0;
        }
        WM_TIMER => {
            let shared = SHARED.get().unwrap();
            if GetWindowLongPtrW(h, GWLP_USERDATA) == 2 {
                if let Ok(s) = shared.status.lock() {
                    text(
                        h,
                        11,
                        if s.label.is_empty() {
                            "Installing OfficeTabs"
                        } else {
                            &s.label
                        },
                    );
                    text(h, 15, &s.detail);
                    if let Some(code) = s.result {
                        SetWindowLongPtrW(h, GWLP_USERDATA, 3);
                        text(
                            h,
                            10,
                            if code == 0 || code == 3010 {
                                "OfficeTabs Setup completed"
                            } else {
                                "OfficeTabs Setup stopped"
                            },
                        );
                        if code == 0 || code == 3010 {
                            SendMessageW(GetDlgItem(h, 14), PBM_SETMARQUEE, 0, 0);
                            SendMessageW(GetDlgItem(h, 14), PBM_SETPOS, 100, 0);
                            text(h, 16, "Installation finished.");
                        } else {
                            SendMessageW(GetDlgItem(h, 14), PBM_SETMARQUEE, 0, 0);
                            text(h, 16, "Nothing was changed.");
                        }
                        text(h, 20, "Finish");
                        EnableWindow(GetDlgItem(h, 20), 1);
                        ShowWindow(GetDlgItem(h, 21), SW_HIDE);
                    } else if let Some(percent) = s.progress.percent() {
                        if !MARQUEE.swap(false, Ordering::Relaxed) {
                            // Switch the bar from its animation to real positions.
                            SendMessageW(GetDlgItem(h, 14), PBM_SETMARQUEE, 0, 0);
                        }
                        text(h, 16, &format!("{percent}% complete"));
                        SendMessageW(GetDlgItem(h, 14), PBM_SETPOS, percent as usize, 0);
                    } else {
                        text(
                            h,
                            16,
                            "Working — Windows Installer has not reported a total yet.",
                        );
                    }
                }
            }
            return 0;
        }
        WM_PAINT => {
            ui::paint(h, false);
            return 0;
        }
        WM_CTLCOLORSTATIC => {
            let dc = w as HDC;
            let id = GetDlgCtrlID(l as HWND);
            let background = if matches!(id, 10 | 11) {
                0x00fbf8f6
            } else {
                0x00ffffff
            };
            let text = if id == 10 { 0x002d251d } else { 0x00796455 };
            return ui::ctlcolor(dc, background, text);
        }
        WM_CLOSE => {
            if GetWindowLongPtrW(h, GWLP_USERDATA) == 2 {
                SHARED.get().unwrap().cancel.store(true, Ordering::Relaxed);
                text(h, 21, "Cancelling...");
                EnableWindow(GetDlgItem(h, 21), 0);
            } else {
                DestroyWindow(h);
            }
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
    // Uninstall from a temporary copy so MSI can delete the installed executable.
    let args: Vec<String> = std::env::args().collect();
    let exe = std::env::current_exe().unwrap();
    let uninstall = exe
        .file_stem()
        .unwrap_or_default()
        .to_string_lossy()
        .eq_ignore_ascii_case("uninstaller")
        || args.iter().any(|v| v == "--uninstall");
    if uninstall && !args.iter().any(|v| v == "--worker-remove") {
        let dir = std::env::temp_dir().join(format!("OfficeTabs-Uninstall-{}", std::process::id()));
        if std::fs::create_dir_all(&dir).is_ok() {
            let target = dir.join("uninstaller.exe");
            if std::fs::copy(&exe, &target).is_ok() {
                let _ = std::process::Command::new(target)
                    .arg("--worker-remove")
                    .spawn();
            }
        }
        return;
    }
    SHARED
        .set(Arc::new(Shared {
            status: Mutex::new(Status::default()),
            cancel: AtomicBool::new(false),
            delivered: AtomicBool::new(false),
        }))
        .ok();
    unsafe {
        InitCommonControls();
        let h = ui::create(
            "OfficeTabs.Rust.Setup",
            "OfficeTabs Setup",
            (610, 440),
            (700, 496),
            Some(proc),
            101,
        );
        if h.is_null() {
            return;
        }
        if uninstall || args.iter().any(|v| v == "--worker-remove") {
            SetWindowLongPtrW(h, GWLP_USERDATA, 1);
            text(h, 10, "Remove OfficeTabs?");
            text(
                h,
                11,
                "This removes the native add-in and Manager. Personal settings are retained.",
            );
            text(h, 20, "Remove");
            text(h, 12, "Close Office before removing OfficeTabs.");
            ShowWindow(GetDlgItem(h, 22), SW_HIDE);
            ShowWindow(GetDlgItem(h, 13), SW_HIDE);
        }
        ui::run_loop(h);
    }
}
#[cfg(test)]
#[cfg(test)]
mod tests {
    use super::*;
    use install::callback;
    use std::ffi::c_void;
    use windows_sys::Win32::System::ApplicationInstallationAndServicing::{
        MsiCloseHandle, MsiCreateRecord, MsiRecordSetInteger, INSTALLMESSAGE_PROGRESS,
    };
    fn shared() -> Shared {
        Shared {
            status: Mutex::new(Status::default()),
            cancel: AtomicBool::new(false),
            delivered: AtomicBool::new(false),
        }
    }
    unsafe fn record(fields: &[i32]) -> u32 {
        let record = MsiCreateRecord(fields.len() as u32);
        for (i, value) in fields.iter().enumerate() {
            assert_eq!(MsiRecordSetInteger(record, i as u32 + 1, *value), 0);
        }
        record
    }
    #[test]
    fn real_msi_record_updates_progress_without_installing() {
        unsafe {
            let s = shared();
            let p = &s as *const _ as *mut c_void;
            let r = record(&[0, 1000, 0, 0]);
            assert_eq!(callback(p, INSTALLMESSAGE_PROGRESS as u32, r), IDOK);
            MsiCloseHandle(r);
            let r = record(&[2, 380, 0, 0]);
            callback(p, INSTALLMESSAGE_PROGRESS as u32, r);
            MsiCloseHandle(r);
            assert_eq!(s.status.lock().unwrap().progress.percent(), Some(38));
        }
    }
    #[test]
    fn cancel_is_delivered_once_and_rollback_can_continue() {
        unsafe {
            let s = shared();
            s.cancel.store(true, Ordering::Relaxed);
            let p = &s as *const _ as *mut c_void;
            let r = record(&[0, 100, 1, 0]);
            assert_eq!(callback(p, INSTALLMESSAGE_PROGRESS as u32, r), IDCANCEL);
            assert_eq!(callback(p, INSTALLMESSAGE_PROGRESS as u32, r), IDOK);
            MsiCloseHandle(r);
            assert!(s.status.lock().unwrap().progress.reverse);
        }
    }
    #[test]
    fn preparation_does_not_become_text_zero() {
        unsafe {
            let s = shared();
            let p = &s as *const _ as *mut c_void;
            let r = record(&[0, 100, 0, 1]);
            callback(p, INSTALLMESSAGE_PROGRESS as u32, r);
            MsiCloseHandle(r);
            let state = s.status.lock().unwrap();
            assert!(state.progress.script);
            assert_ne!(state.label, "0");
        }
    }
}
