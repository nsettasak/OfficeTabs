#![cfg(windows)]
use officetabs::{
    automation::{table, DispatchVtable, Raw, IID_UNKNOWN},
    com_server::{DllCanUnloadNow, DllGetClassObject, EXCEL, WORD},
};
use std::{
    ptr::{null, null_mut},
    sync::atomic::{AtomicU32, Ordering},
};
use windows_sys::{
    core::GUID,
    Win32::{
        System::{Com::*, Variant::*},
        UI::WindowsAndMessaging::*,
    },
};
#[repr(C)]
struct Factory {
    query: unsafe extern "system" fn(Raw, *const GUID, *mut Raw) -> i32,
    add: unsafe extern "system" fn(Raw) -> u32,
    release: unsafe extern "system" fn(Raw) -> u32,
    create: unsafe extern "system" fn(Raw, Raw, *const GUID, *mut Raw) -> i32,
    lock: unsafe extern "system" fn(Raw, i32) -> i32,
}
#[repr(C)]
struct Ext {
    base: DispatchVtable,
    connect: unsafe extern "system" fn(Raw, Raw, i32, Raw, *mut Raw) -> i32,
    disconnect: unsafe extern "system" fn(Raw, i32, *mut Raw) -> i32,
    update: usize,
    start: usize,
    shutdown: usize,
}
#[repr(C)]
struct Mock {
    vt: *const DispatchVtable,
    refs: AtomicU32,
}
unsafe extern "system" fn query(p: Raw, _: *const GUID, out: *mut Raw) -> i32 {
    *out = p;
    add(p);
    0
}
unsafe extern "system" fn add(p: Raw) -> u32 {
    (*(p as *mut Mock)).refs.fetch_add(1, Ordering::Relaxed) + 1
}
unsafe extern "system" fn release(p: Raw) -> u32 {
    (*(p as *mut Mock)).refs.fetch_sub(1, Ordering::Relaxed) - 1
}
unsafe extern "system" fn count(_: Raw, p: *mut u32) -> i32 {
    *p = 0;
    0
}
unsafe extern "system" fn info(_: Raw, _: u32, _: u32, _: *mut Raw) -> i32 {
    -1
}
unsafe extern "system" fn ids(
    _: Raw,
    _: *const GUID,
    p: *const *const u16,
    _: u32,
    _: u32,
    out: *mut i32,
) -> i32 {
    let mut n = 0;
    while *(*p).add(n) != 0 {
        n += 1;
    }
    *out = match String::from_utf16_lossy(std::slice::from_raw_parts(*p, n)).as_str() {
        "Documents" | "Workbooks" => 1,
        "Count" => 2,
        _ => return -1,
    };
    0
}
unsafe extern "system" fn invoke(
    p: Raw,
    id: i32,
    _: *const GUID,
    _: u32,
    _: u16,
    _: *mut DISPPARAMS,
    out: *mut VARIANT,
    _: *mut EXCEPINFO,
    _: *mut u32,
) -> i32 {
    *out = VARIANT::default();
    if id == 1 {
        (*out).Anonymous.Anonymous.vt = VT_DISPATCH;
        (*out).Anonymous.Anonymous.Anonymous.pdispVal = p;
        add(p);
    } else {
        (*out).Anonymous.Anonymous.vt = VT_I4;
        (*out).Anonymous.Anonymous.Anonymous.lVal = 0;
    }
    0
}
static MOCK: DispatchVtable = DispatchVtable {
    query,
    add,
    release,
    type_count: count,
    type_info: info,
    ids,
    invoke,
};
#[test]
fn native_com_lifecycle_without_office_or_registration() {
    unsafe {
        assert_eq!(DllCanUnloadNow(), 0);
        assert!(DllGetClassObject(null(), null(), null_mut()) < 0);
        let factory_iid = GUID::from_u128(0x00000001_0000_0000_c000_000000000046);
        let ext_iid = GUID::from_u128(0xb65ad801_abaf_11d0_bb8b_00a0c90f2744);
        for clsid in [WORD, EXCEL] {
            let mut f = null_mut();
            assert_eq!(DllGetClassObject(&clsid, &factory_iid, &mut f), 0);
            assert_eq!(DllCanUnloadNow(), 1);
            let ft = &**(f as *const *const Factory);
            let mut obj = null_mut();
            assert_eq!((ft.create)(f, null_mut(), &ext_iid, &mut obj), 0);
            let ext = &**(obj as *const *const Ext);
            assert!((ext.connect)(obj, null_mut(), 0, null_mut(), null_mut()) < 0);
            let mut mock = Box::new(Mock {
                vt: &MOCK,
                refs: AtomicU32::new(1),
            });
            let app = &mut *mock as *mut Mock as Raw;
            assert_eq!((ext.connect)(obj, app, 0, null_mut(), null_mut()), 0);
            assert_eq!(officetabs::native::HOST_COUNT.load(Ordering::Acquire), 1);
            let mut msg = MSG::default();
            for _ in 0..20 {
                if PeekMessageW(&mut msg, null_mut(), 0, 0, PM_REMOVE) == 0 {
                    break;
                }
                TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
            let mut identity = null_mut();
            assert_eq!((table(obj).query)(obj, &IID_UNKNOWN, &mut identity), 0);
            assert_eq!(identity, obj);
            (table(identity).release)(identity);
            assert_eq!((ext.disconnect)(obj, 0, null_mut()), 0);
            assert_eq!(officetabs::native::HOST_COUNT.load(Ordering::Acquire), 0);
            assert_eq!(mock.refs.load(Ordering::Relaxed), 1);
            (table(obj).release)(obj);
            (ft.release)(f);
            assert_eq!(DllCanUnloadNow(), 0);
        }
    }
}
