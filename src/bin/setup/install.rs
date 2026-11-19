//! The install itself: handing the embedded MSI to Windows Installer with a
//! progress callback, and cancelling cleanly when the window asks it to.
use crate::registry::{field, installed_product, office_running, uninstall_string};
use officetabs::{model::Progress, native::wide};
use std::{
    ffi::c_void,
    ptr::{null, null_mut},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex, OnceLock,
    },
};
use windows_sys::Win32::{
    System::ApplicationInstallationAndServicing::*, UI::WindowsAndMessaging::*,
};
include!(concat!(env!("OUT_DIR"), "/payload.rs"));

#[derive(Default)]
pub(crate) struct Status {
    pub(crate) progress: Progress,
    pub(crate) label: String,
    pub(crate) detail: String,
    pub(crate) result: Option<u32>,
}
pub(crate) struct Shared {
    pub(crate) status: Mutex<Status>,
    pub(crate) cancel: AtomicBool,
    pub(crate) delivered: AtomicBool,
}
pub(crate) static SHARED: OnceLock<Arc<Shared>> = OnceLock::new();
/// True while the progress bar is animating because no total is known yet.
pub(crate) static MARQUEE: AtomicBool = AtomicBool::new(true);
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

pub(crate) unsafe extern "system" fn callback(context: *mut c_void, kind: u32, record: u32) -> i32 {
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

pub(crate) fn run(shared: Arc<Shared>, uninstall: bool) {
    let result = (|| -> Result<u32, String> {
        if office_running() {
            return Err("Word, Excel, PowerPoint or OfficeTabs Manager is still running. Close it and run Setup again.".into());
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
            if let Some(product) = installed_product() {
                if uninstall_string(&product, "DisplayVersion").as_deref()
                    == Some(env!("CARGO_PKG_VERSION"))
                {
                    return Err(format!(
                        "OfficeTabs {} is already installed. Remove it from Settings > Apps, then run Setup again.",
                        env!("CARGO_PKG_VERSION")
                    ));
                }
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
                let product =
                    installed_product().ok_or("OfficeTabs is not installed for this user.")?;
                MsiConfigureProductExW(
                    wide(&product).as_ptr(),
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
