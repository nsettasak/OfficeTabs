//! What this installer asks Windows before it touches anything: which Office
//! processes are running, what is installed for this user, and how to remove it.
use officetabs::native::wide;
use std::{
    mem::{size_of, zeroed},
    ptr::{null, null_mut},
};
use windows_sys::Win32::{
    Foundation::*,
    System::{ApplicationInstallationAndServicing::*, Diagnostics::ToolHelp::*},
    UI::WindowsAndMessaging::*,
};

pub(crate) fn field(record: u32, index: u32) -> String {
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

/// Executables that hold the DLL or the installation folder open.
const BLOCKING: [&str; 4] = [
    "winword.exe",
    "excel.exe",
    "powerpnt.exe",
    "officetabs.manager.exe",
];
/// PIDs of the Office processes (and our Manager) that are currently running.
pub(crate) fn office_pids() -> Vec<u32> {
    let mut found = Vec::new();
    unsafe {
        let snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
        if snapshot == INVALID_HANDLE_VALUE {
            return found;
        }
        let mut entry = PROCESSENTRY32W {
            dwSize: size_of::<PROCESSENTRY32W>() as u32,
            ..zeroed()
        };
        let mut more = Process32FirstW(snapshot, &mut entry);
        while more != 0 {
            let len = entry.szExeFile.iter().position(|v| *v == 0).unwrap_or(260);
            let name = String::from_utf16_lossy(&entry.szExeFile[..len]).to_ascii_lowercase();
            if BLOCKING.contains(&name.as_str()) {
                found.push(entry.th32ProcessID);
            }
            more = Process32NextW(snapshot, &mut entry);
        }
        CloseHandle(snapshot);
    }
    found
}
pub(crate) fn office_running() -> bool {
    !office_pids().is_empty()
}
unsafe extern "system" fn close_if_ours(window: HWND, target: LPARAM) -> i32 {
    let pids = &*(target as *const Vec<u32>);
    let mut owner = 0u32;
    GetWindowThreadProcessId(window, &mut owner);
    if pids.contains(&owner) && IsWindowVisible(window) != 0 {
        // WM_CLOSE, never TerminateProcess: Office runs its own save prompt.
        PostMessageW(window, WM_CLOSE, 0, 0);
    }
    1
}
/// Asks every Office window to close and waits for the processes to exit.
pub(crate) fn close_office() -> bool {
    let pids = office_pids();
    if pids.is_empty() {
        return true;
    }
    unsafe {
        EnumWindows(Some(close_if_ours), &pids as *const Vec<u32> as LPARAM);
    }
    // Office may show a save prompt; give the user time to answer it.
    for _ in 0..120 {
        if !office_running() {
            return true;
        }
        std::thread::sleep(std::time::Duration::from_millis(250));
    }
    !office_running()
}
/// Product code of the installed package, read from Add/Remove Programs so the
/// uninstaller keeps working after the ProductCode changes with every version.
pub(crate) fn installed_product() -> Option<String> {
    use windows_sys::Win32::System::Registry::*;
    let uninstall = wide("Software\\Microsoft\\Windows\\CurrentVersion\\Uninstall");
    for root in [HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE] {
        unsafe {
            let mut key: HKEY = null_mut();
            if RegOpenKeyExW(root, uninstall.as_ptr(), 0, KEY_READ, &mut key) != 0 {
                continue;
            }
            let mut index = 0u32;
            loop {
                let mut name = [0u16; 128];
                let mut length = name.len() as u32;
                let status = RegEnumKeyExW(
                    key,
                    index,
                    name.as_mut_ptr(),
                    &mut length,
                    null_mut(),
                    null_mut(),
                    null_mut(),
                    null_mut(),
                );
                if status != 0 {
                    break;
                }
                index += 1;
                let mut sub: HKEY = null_mut();
                if RegOpenKeyExW(key, name.as_ptr(), 0, KEY_READ, &mut sub) != 0 {
                    continue;
                }
                let field = wide("DisplayName");
                let mut kind = 0u32;
                let mut size = 0u32;
                let readable = RegQueryValueExW(
                    sub,
                    field.as_ptr(),
                    null(),
                    &mut kind,
                    null_mut(),
                    &mut size,
                ) == 0
                    && size > 1
                    && size < 512;
                let mut display = String::new();
                if readable {
                    let mut buffer = vec![0u8; size as usize];
                    if RegQueryValueExW(
                        sub,
                        field.as_ptr(),
                        null(),
                        &mut kind,
                        buffer.as_mut_ptr(),
                        &mut size,
                    ) == 0
                    {
                        let units = size as usize / 2;
                        let value =
                            std::slice::from_raw_parts(buffer.as_ptr() as *const u16, units);
                        display = String::from_utf16_lossy(value)
                            .trim_end_matches('\0')
                            .into();
                    }
                }
                RegCloseKey(sub);
                if display == "OfficeTabs" {
                    RegCloseKey(key);
                    return Some(String::from_utf16_lossy(&name[..length as usize]));
                }
            }
            RegCloseKey(key);
        }
    }
    None
}
pub(crate) fn uninstall_string(key_name: &str, field: &str) -> Option<String> {
    use windows_sys::Win32::System::Registry::*;
    let path = wide(&format!(
        "Software\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\{key_name}"
    ));
    for root in [HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE] {
        unsafe {
            let mut key: HKEY = null_mut();
            if RegOpenKeyExW(root, path.as_ptr(), 0, KEY_READ, &mut key) != 0 {
                continue;
            }
            let field = wide(field);
            let mut kind = 0u32;
            let mut size = 0u32;
            let mut found = None;
            if RegQueryValueExW(
                key,
                field.as_ptr(),
                null(),
                &mut kind,
                null_mut(),
                &mut size,
            ) == 0
                && size > 1
                && size < 1024
            {
                let mut buffer = vec![0u8; size as usize];
                if RegQueryValueExW(
                    key,
                    field.as_ptr(),
                    null(),
                    &mut kind,
                    buffer.as_mut_ptr(),
                    &mut size,
                ) == 0
                {
                    let units = size as usize / 2;
                    let text = String::from_utf16_lossy(std::slice::from_raw_parts(
                        buffer.as_ptr() as *const u16,
                        units,
                    ));
                    found = Some(text.trim_end_matches('\0').to_string());
                }
            }
            RegCloseKey(key);
            if found.is_some() {
                return found;
            }
        }
    }
    None
}
