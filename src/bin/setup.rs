#![windows_subsystem = "windows"]
use officetabs::{model::Progress, native::wide, ui};
use std::{
    ffi::c_void,
    mem::{size_of, zeroed},
    ptr::{null, null_mut},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex, OnceLock,
    },
    thread,
};
use windows_sys::Win32::{
    Foundation::*,
    Graphics::Gdi::HDC,
    System::{ApplicationInstallationAndServicing::*, Diagnostics::ToolHelp::*},
    UI::{Controls::*, Input::KeyboardAndMouse::EnableWindow, WindowsAndMessaging::*},
};
include!(concat!(env!("OUT_DIR"), "/payload.rs"));
const PRODUCT: &str = "{959121E7-1F35-4D07-84F6-303030303033}";
#[derive(Default)]
struct Status {
    progress: Progress,
    label: String,
    detail: String,
    result: Option<u32>,
}
struct Shared {
    status: Mutex<Status>,
    cancel: AtomicBool,
    delivered: AtomicBool,
}
static SHARED: OnceLock<Arc<Shared>> = OnceLock::new();
#[link(name = "msi")]
extern "system" {
    #[link_name = "MsiSetExternalUIRecord"]
    fn external(
        handler: PINSTALLUI_HANDLER_RECORD,
        filter: u32,
        context: *const c_void,
        previous: *mut PINSTALLUI_HANDLER_RECORD,
    ) -> u32;
}
fn field(record: u32, index: u32) -> String {
    unsafe {
        let mut length = 0;
        MsiRecordGetStringW(record, index, null_mut(), &mut length);
        let mut value = vec![0u16; length as usize + 1];
        if MsiRecordGetStringW(record, index, value.as_mut_ptr(), &mut length) == 0 {
            String::from_utf16_lossy(&value[..length as usize])
        } else {
            String::new()
        }
    }
}
unsafe extern "system" fn callback(context: *mut c_void, kind: u32, record: u32) -> i32 {
    let shared = &*(context as *const Shared);
    if shared.cancel.load(Ordering::Relaxed) && !shared.delivered.swap(true, Ordering::Relaxed) {
        return IDCANCEL;
    }
    if let Ok(mut s) = shared.status.lock() {
        match (kind & 0xff000000) as i32 {
            INSTALLMESSAGE_PROGRESS => s.progress.message(
                MsiRecordGetInteger(record, 1),
                MsiRecordGetInteger(record, 2),
                MsiRecordGetInteger(record, 3),
                MsiRecordGetInteger(record, 4),
            ),
            INSTALLMESSAGE_ACTIONSTART => {
                s.label = field(record, 2);
                s.detail.clear();
                s.progress.action_data = false;
            }
            INSTALLMESSAGE_ACTIONDATA => {
                s.progress.action();
                s.detail = field(record, 1);
            }
            INSTALLMESSAGE_ERROR | INSTALLMESSAGE_FATALEXIT => {
                s.detail = format!(
                    "Windows Installer error {}: {}",
                    MsiRecordGetInteger(record, 1),
                    field(record, 2)
                );
            }
            _ => {}
        }
    }
    IDOK
}
fn blocked() -> bool {
    unsafe {
        let snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
        if snapshot == INVALID_HANDLE_VALUE {
            return true;
        }
        let mut entry = PROCESSENTRY32W {
            dwSize: size_of::<PROCESSENTRY32W>() as u32,
            ..zeroed()
        };
        let mut more = Process32FirstW(snapshot, &mut entry);
        let mut blocked = false;
        while more != 0 {
            let len = entry.szExeFile.iter().position(|v| *v == 0).unwrap_or(260);
            let name = String::from_utf16_lossy(&entry.szExeFile[..len]).to_ascii_lowercase();
            if [
                "winword.exe",
                "excel.exe",
                "powerpnt.exe",
                "officetabs.manager.exe",
            ]
            .contains(&name.as_str())
            {
                blocked = true;
                break;
            }
            more = Process32NextW(snapshot, &mut entry);
        }
        CloseHandle(snapshot);
        blocked
    }
}
fn run(shared: Arc<Shared>, uninstall: bool) {
    let result = (|| -> Result<u32, String> {
        if blocked() {
            return Err("Save your documents and close Word, Excel, PowerPoint and OfficeTabs Manager, then run Setup again.".into());
        }
        let root = std::path::PathBuf::from(
            std::env::var_os("LOCALAPPDATA").ok_or("LOCALAPPDATA is unavailable")?,
        );
        let logs = root.join("OfficeTabs").join("Logs");
        std::fs::create_dir_all(&logs).map_err(|e| e.to_string())?;
        let log = logs.join(format!("Native-Setup-{}.log", std::process::id()));
        let folder = std::env::temp_dir().join(format!("OfficeTabs-Rust-{}", std::process::id()));
        std::fs::create_dir_all(&folder).map_err(|e| e.to_string())?;
        let package = folder.join("OfficeTabs.msi");
        if !uninstall {
            if MSI.is_empty() {
                return Err(
                    "This executable has no installer payload. Use the packaged Setup.exe.".into(),
                );
            }
            std::fs::write(&package, MSI).map_err(|e| e.to_string())?;
        }
        unsafe {
            MsiEnableLogW(0x7fff, wide(&log.to_string_lossy()).as_ptr(), 0);
            let previous_ui = MsiSetInternalUI(INSTALLUILEVEL_NONE, null_mut());
            let mut previous = None;
            let filter = (INSTALLLOGMODE_PROGRESS
                | INSTALLLOGMODE_ACTIONSTART
                | INSTALLLOGMODE_ACTIONDATA
                | INSTALLLOGMODE_ERROR
                | INSTALLLOGMODE_FATALEXIT
                | INSTALLLOGMODE_WARNING) as u32;
            let hr = external(
                Some(callback),
                filter,
                Arc::as_ptr(&shared) as *const c_void,
                &mut previous,
            );
            if hr != 0 {
                MsiSetInternalUI(previous_ui, null_mut());
                return Err(format!("Cannot receive installer progress ({hr})."));
            }
            let code = if uninstall {
                MsiConfigureProductExW(
                    wide(PRODUCT).as_ptr(),
                    0,
                    INSTALLSTATE_ABSENT,
                    wide("REMOVE=ALL REBOOT=ReallySuppress").as_ptr(),
                )
            } else {
                MsiInstallProductW(
                    wide(&package.to_string_lossy()).as_ptr(),
                    wide("REBOOT=ReallySuppress").as_ptr(),
                )
            };
            external(previous, 0, null(), null_mut());
            MsiSetInternalUI(previous_ui, null_mut());
            MsiEnableLogW(0, null(), 0);
            if package.exists() {
                let _ = std::fs::remove_file(package);
            }
            let _ = std::fs::remove_dir(folder);
            Ok(code)
        }
    })();
    let mut s = shared.status.lock().unwrap();
    match result {
        Ok(code) => {
            s.result = Some(code);
            s.label = match code {
                0 => "Completed successfully.",
                3010 => "Completed. Restart Windows to finish.",
                1602 => "Cancelled. Windows Installer has finished rollback.",
                _ => "Setup did not complete. See the installer log.",
            }
            .into();
        }
        Err(message) => {
            s.label = message;
            s.result = Some(1603);
        }
    }
}
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
                include_str!("../../LICENSE.txt"),
                13,
                ES_MULTILINE as u32 | ES_READONLY as u32 | WS_VSCROLL | WS_BORDER | WS_TABSTOP,
                (32, 184, 636, 216),
            );
            ui::control(
                h,
                "msctls_progress32",
                "",
                14,
                PBS_SMOOTH,
                (32, 210, 636, 24),
            );
            ShowWindow(GetDlgItem(h, 14), SW_HIDE);
            ui::control(h, "STATIC", "", 15, 0, (32, 254, 636, 120));
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
                        text(h, 11, "Preparing installation...");
                        text(h, 12, "");
                        ShowWindow(GetDlgItem(h, 14), SW_SHOW);
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
                    text(h, 11, &s.label);
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
                            SendMessageW(GetDlgItem(h, 14), PBM_SETPOS, 100, 0);
                        }
                        text(h, 20, "Finish");
                        EnableWindow(GetDlgItem(h, 20), 1);
                        ShowWindow(GetDlgItem(h, 21), SW_HIDE);
                    } else if s.progress.script || s.progress.percent().is_none() {
                        text(
                            h,
                            12,
                            "Preparing installation — progress is not available yet.",
                        );
                    } else {
                        let percent = s.progress.percent().unwrap();
                        text(h, 12, &format!("{percent}%"));
                        SendMessageW(GetDlgItem(h, 14), PBM_SETPOS, percent as usize, 0);
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
mod tests {
    use super::*;
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
