//! Window placement capture/apply helpers shared by the controller and strip code.
use crate::native::{host_name, rect, same, setting, settings, wide};
use std::mem::{size_of, zeroed};
use std::ptr::null_mut;
use windows_sys::Win32::{
    Foundation::*,
    Graphics::Gdi::*,
    UI::{Controls::*, WindowsAndMessaging::*},
};

#[derive(Clone, Copy)]
pub(crate) struct CapturedPlacement {
    placement: WINDOWPLACEMENT,
    screen: RECT,
}
pub(crate) unsafe fn capture_placement(h: HWND) -> Option<CapturedPlacement> {
    let mut p = WINDOWPLACEMENT {
        length: size_of::<WINDOWPLACEMENT>() as u32,
        ..zeroed()
    };
    if GetWindowPlacement(h, &mut p) == 0 || IsIconic(h) != 0 {
        return None;
    }
    let mut screen = RECT::default();
    if GetWindowRect(h, &mut screen) == 0
        || screen.right <= screen.left
        || screen.bottom <= screen.top
    {
        return None;
    }
    p.flags = 0;
    p.ptMinPosition = POINT { x: -1, y: -1 };
    p.ptMaxPosition = POINT { x: -1, y: -1 };
    Some(CapturedPlacement {
        placement: p,
        screen,
    })
}
pub(crate) unsafe fn apply_placement(h: HWND, captured: CapturedPlacement) {
    if IsWindow(h) == 0 {
        return;
    }
    let p = captured.placement;
    let r = captured.screen;
    if p.showCmd != SW_SHOWMAXIMIZED as u32 {
        // A snapped window can report SW_SHOWNORMAL while rcNormalPosition is
        // its PRE-SNAP restore rectangle. Transfer current screen bounds instead.
        if IsZoomed(h) != 0 || IsIconic(h) != 0 {
            ShowWindow(h, SW_SHOWNOACTIVATE);
        }
        if !same(rect(h), r) {
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
        return;
    }
    let dest = MonitorFromRect(&r, MONITOR_DEFAULTTONEAREST);
    if IsZoomed(h) != 0 && MonitorFromWindow(h, MONITOR_DEFAULTTONEAREST) == dest {
        return;
    }
    if MonitorFromWindow(h, MONITOR_DEFAULTTONEAREST) != dest {
        if IsZoomed(h) != 0 || IsIconic(h) != 0 {
            ShowWindow(h, SW_SHOWNOACTIVATE);
        }
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
    // WINDOWPLACEMENT stays in workspace coordinates; never clamp it as screen coordinates.
    SetWindowPlacement(h, &p);
}
pub(crate) unsafe fn copy_placement(from: HWND, to: HWND) {
    if from == to || from.is_null() || to.is_null() {
        return;
    }
    if let Some(p) = capture_placement(from) {
        apply_placement(to, p);
    }
}
pub(crate) unsafe fn close_prompt(owner: HWND, excel: bool) -> i32 {
    if std::fs::read_to_string(settings(excel, "close-warning"))
        .unwrap_or_default()
        .trim()
        == "skip"
    {
        return 100;
    }
    let title = wide("OfficeTabs");
    let heading = wide(&format!("Tabs for {}", host_name(excel)));
    let text = wide("Multiple windows are about to be closed. Continue?");
    let ok = wide("OK");
    let active = wide("Close Active");
    let cancel = wide("Cancel");
    let verify = wide("Don't warn me when closing windows");
    let buttons = [
        TASKDIALOG_BUTTON {
            nButtonID: 100,
            pszButtonText: ok.as_ptr(),
        },
        TASKDIALOG_BUTTON {
            nButtonID: 101,
            pszButtonText: active.as_ptr(),
        },
        TASKDIALOG_BUTTON {
            nButtonID: 102,
            pszButtonText: cancel.as_ptr(),
        },
    ];
    let mut config: TASKDIALOGCONFIG = zeroed();
    config.cbSize = size_of::<TASKDIALOGCONFIG>() as u32;
    config.hwndParent = owner;
    config.pszWindowTitle = title.as_ptr();
    config.pszMainInstruction = heading.as_ptr();
    config.pszContent = text.as_ptr();
    config.pszVerificationText = verify.as_ptr();
    config.cButtons = 3;
    config.pButtons = buttons.as_ptr();
    config.nDefaultButton = 102;
    config.dwFlags = TDF_ALLOW_DIALOG_CANCELLATION;
    let mut choice = 102;
    let mut checked = 0;
    if TaskDialogIndirect(&config, &mut choice, null_mut(), &mut checked) < 0 {
        return 102;
    }
    if choice != 102 && checked != 0 {
        let _ = setting(excel, "close-warning", "skip");
    }
    choice
}
#[cfg(test)]
mod placement_tests {
    use super::*;
    use std::ptr::null;
    use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
    struct TestWindow(HWND);
    impl Drop for TestWindow {
        fn drop(&mut self) {
            unsafe {
                DestroyWindow(self.0);
            }
        }
    }
    #[test]
    fn switching_uses_current_bounds_not_pre_snap_restore_bounds() {
        unsafe {
            // Hidden test windows only: no Office, registration or user documents.
            let create = || {
                TestWindow(CreateWindowExW(
                    0,
                    wide("STATIC").as_ptr(),
                    wide("OfficeTabs placement test").as_ptr(),
                    WS_OVERLAPPEDWINDOW,
                    80,
                    90,
                    900,
                    650,
                    null_mut(),
                    null_mut(),
                    GetModuleHandleW(null()),
                    null(),
                ))
            };
            let a = create();
            let b = create();
            assert!(!a.0.is_null() && !b.0.is_null());
            for screen in [
                RECT {
                    left: 40,
                    top: 70,
                    right: 680,
                    bottom: 570,
                },
                RECT {
                    left: -900,
                    top: 50,
                    right: -260,
                    bottom: 550,
                },
            ] {
                let mut captured = capture_placement(a.0).unwrap();
                captured.screen = screen;
                captured.placement.showCmd = SW_SHOWNORMAL as u32;
                captured.placement.rcNormalPosition = RECT {
                    left: 80,
                    top: 90,
                    right: 980,
                    bottom: 740,
                };
                apply_placement(b.0, captured);
                assert!(
                    same(rect(b.0), screen),
                    "must not restore the pre-snap rectangle"
                );
                for _ in 0..32 {
                    copy_placement(b.0, a.0);
                    copy_placement(a.0, b.0);
                    assert!(
                        same(rect(a.0), screen) && same(rect(b.0), screen),
                        "tab switches must not drift"
                    );
                }
                assert_eq!(IsWindowVisible(a.0), 0);
                assert_eq!(IsWindowVisible(b.0), 0);
            }
        }
    }
}
