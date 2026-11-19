use crate::automation::*;
use std::{
    ffi::c_void,
    ptr,
    sync::atomic::{AtomicU32, Ordering},
};
use windows_sys::{
    core::GUID,
    Win32::System::{Com::*, Variant::*},
};
pub const WORD: GUID = GUID::from_u128(0xd6217423_507f_43ae_bf5b_5ce26989e401);
pub const EXCEL: GUID = GUID::from_u128(0x371ad550_752a_42f8_bb8d_7d17961d9f02);
const EXT: GUID = GUID::from_u128(0xb65ad801_abaf_11d0_bb8b_00a0c90f2744);
const FACTORY: GUID = GUID::from_u128(0x00000001_0000_0000_c000_000000000046);
static OBJECTS: AtomicU32 = AtomicU32::new(0);
static LOCKS: AtomicU32 = AtomicU32::new(0);
fn eq(a: &GUID, b: &GUID) -> bool {
    a.data1 == b.data1 && a.data2 == b.data2 && a.data3 == b.data3 && a.data4 == b.data4
}
#[repr(C)]
struct ExtensionTable {
    base: DispatchVtable,
    connect: unsafe extern "system" fn(Raw, Raw, i32, Raw, *mut Raw) -> i32,
    disconnect: unsafe extern "system" fn(Raw, i32, *mut Raw) -> i32,
    update: unsafe extern "system" fn(Raw, *mut Raw) -> i32,
    start: unsafe extern "system" fn(Raw, *mut Raw) -> i32,
    shutdown: unsafe extern "system" fn(Raw, *mut Raw) -> i32,
}
#[repr(C)]
struct Addin {
    vtable: *const ExtensionTable,
    refs: AtomicU32,
    excel: bool,
    controller: usize,
}
unsafe extern "system" fn query(p: Raw, iid: *const GUID, out: *mut Raw) -> i32 {
    if out.is_null() || iid.is_null() {
        return 0x80004003u32 as i32;
    }
    *out = ptr::null_mut();
    if eq(&*iid, &IID_UNKNOWN) || eq(&*iid, &IID_DISPATCH) || eq(&*iid, &EXT) {
        *out = p;
        add(p);
        0
    } else {
        0x80004002u32 as i32
    }
}
unsafe extern "system" fn add(p: Raw) -> u32 {
    (*(p as *mut Addin)).refs.fetch_add(1, Ordering::Relaxed) + 1
}
unsafe extern "system" fn release(p: Raw) -> u32 {
    let count = (*(p as *mut Addin)).refs.fetch_sub(1, Ordering::AcqRel) - 1;
    if count == 0 {
        let obj = Box::from_raw(p as *mut Addin);
        crate::native::stop(obj.controller);
        OBJECTS.fetch_sub(1, Ordering::AcqRel);
    }
    count
}
unsafe extern "system" fn count(_: Raw, out: *mut u32) -> i32 {
    if out.is_null() {
        return 0x80004003u32 as i32;
    }
    *out = 0;
    0
}
unsafe extern "system" fn info(_: Raw, _: u32, _: u32, out: *mut Raw) -> i32 {
    if !out.is_null() {
        *out = ptr::null_mut();
    }
    0x80004001u32 as i32
}
unsafe extern "system" fn ids(
    _: Raw,
    _: *const GUID,
    names: *const *const u16,
    n: u32,
    _: u32,
    out: *mut i32,
) -> i32 {
    if names.is_null() || out.is_null() {
        return 0x80004003u32 as i32;
    }
    for i in 0..n as usize {
        let p = *names.add(i);
        let mut len = 0;
        while *p.add(len) != 0 && len < 128 {
            len += 1;
        }
        let name =
            String::from_utf16_lossy(std::slice::from_raw_parts(p, len)).to_ascii_lowercase();
        *out.add(i) = match name.as_str() {
            "onconnection" => 1,
            "ondisconnection" => 2,
            "onaddinsupdate" => 3,
            "onstartupcomplete" => 4,
            "onbeginshutdown" => 5,
            _ => return 0x80020006u32 as i32,
        };
    }
    0
}
unsafe extern "system" fn invoke(
    p: Raw,
    id: i32,
    _: *const GUID,
    _: u32,
    _: u16,
    args: *mut DISPPARAMS,
    _: *mut VARIANT,
    _: *mut EXCEPINFO,
    _: *mut u32,
) -> i32 {
    match id {
        1 => {
            if args.is_null() || (*args).cArgs != 4 {
                return 0x8002000eu32 as i32;
            }
            let v = &*(*args).rgvarg.add(3);
            if v.Anonymous.Anonymous.vt != VT_DISPATCH {
                return 0x80020005u32 as i32;
            }
            connect(
                p,
                v.Anonymous.Anonymous.Anonymous.pdispVal,
                0,
                ptr::null_mut(),
                ptr::null_mut(),
            )
        }
        2 => disconnect(p, 0, ptr::null_mut()),
        3 | 4 => 0,
        5 => shutdown(p, ptr::null_mut()),
        _ => 0x80020003u32 as i32,
    }
}
unsafe extern "system" fn connect(p: Raw, app: Raw, _: i32, _: Raw, _: *mut Raw) -> i32 {
    let obj = &mut *(p as *mut Addin);
    if obj.controller != 0 {
        return 0;
    }
    let Some(app) = Dispatch::borrow(app) else {
        return 0x80004003u32 as i32;
    };
    match crate::native::start(app, obj.excel) {
        Ok(h) => {
            obj.controller = h;
            0
        }
        Err(hr) => hr,
    }
}
unsafe extern "system" fn disconnect(p: Raw, _: i32, _: *mut Raw) -> i32 {
    let obj = &mut *(p as *mut Addin);
    crate::native::stop(obj.controller);
    obj.controller = 0;
    0
}
unsafe extern "system" fn noop(_: Raw, _: *mut Raw) -> i32 {
    0
}
unsafe extern "system" fn shutdown(p: Raw, _: *mut Raw) -> i32 {
    disconnect(p, 0, ptr::null_mut())
}
static VTABLE: ExtensionTable = ExtensionTable {
    base: DispatchVtable {
        query,
        add,
        release,
        type_count: count,
        type_info: info,
        ids,
        invoke,
    },
    connect,
    disconnect,
    update: noop,
    start: noop,
    shutdown,
};
#[repr(C)]
struct FactoryTable {
    query: unsafe extern "system" fn(Raw, *const GUID, *mut Raw) -> i32,
    add: unsafe extern "system" fn(Raw) -> u32,
    release: unsafe extern "system" fn(Raw) -> u32,
    create: unsafe extern "system" fn(Raw, Raw, *const GUID, *mut Raw) -> i32,
    lock: unsafe extern "system" fn(Raw, i32) -> i32,
}
#[repr(C)]
struct ClassFactory {
    vtable: *const FactoryTable,
    refs: AtomicU32,
    excel: bool,
}
unsafe extern "system" fn fq(p: Raw, iid: *const GUID, out: *mut Raw) -> i32 {
    if out.is_null() || iid.is_null() {
        return 0x80004003u32 as i32;
    }
    *out = ptr::null_mut();
    if eq(&*iid, &IID_UNKNOWN) || eq(&*iid, &FACTORY) {
        *out = p;
        fa(p);
        0
    } else {
        0x80004002u32 as i32
    }
}
unsafe extern "system" fn fa(p: Raw) -> u32 {
    (*(p as *mut ClassFactory))
        .refs
        .fetch_add(1, Ordering::Relaxed)
        + 1
}
unsafe extern "system" fn fr(p: Raw) -> u32 {
    let n = (*(p as *mut ClassFactory))
        .refs
        .fetch_sub(1, Ordering::AcqRel)
        - 1;
    if n == 0 {
        drop(Box::from_raw(p as *mut ClassFactory));
        OBJECTS.fetch_sub(1, Ordering::AcqRel);
    }
    n
}
unsafe extern "system" fn create(p: Raw, outer: Raw, iid: *const GUID, out: *mut Raw) -> i32 {
    if !outer.is_null() {
        return 0x80040110u32 as i32;
    }
    let obj = Box::new(Addin {
        vtable: &VTABLE,
        refs: AtomicU32::new(1),
        excel: (*(p as *mut ClassFactory)).excel,
        controller: 0,
    });
    OBJECTS.fetch_add(1, Ordering::Relaxed);
    let raw = Box::into_raw(obj) as Raw;
    let hr = query(raw, iid, out);
    release(raw);
    hr
}
unsafe extern "system" fn lock(_: Raw, value: i32) -> i32 {
    if value != 0 {
        LOCKS.fetch_add(1, Ordering::Relaxed);
    } else {
        let _ = LOCKS.try_update(Ordering::AcqRel, Ordering::Relaxed, |v| v.checked_sub(1));
    }
    0
}
static FACTORY_TABLE: FactoryTable = FactoryTable {
    query: fq,
    add: fa,
    release: fr,
    create,
    lock,
};
/// # Safety
/// COM entry point: `clsid`, `iid` and `out` must be valid pointers supplied by OLE.
#[no_mangle]
pub unsafe extern "system" fn DllGetClassObject(
    clsid: *const GUID,
    iid: *const GUID,
    out: *mut Raw,
) -> i32 {
    if clsid.is_null() || iid.is_null() || out.is_null() {
        return 0x80004003u32 as i32;
    }
    *out = ptr::null_mut();
    if !eq(&*clsid, &WORD) && !eq(&*clsid, &EXCEL) {
        return 0x80040111u32 as i32;
    }
    let factory = Box::new(ClassFactory {
        vtable: &FACTORY_TABLE,
        refs: AtomicU32::new(1),
        excel: eq(&*clsid, &EXCEL),
    });
    OBJECTS.fetch_add(1, Ordering::Relaxed);
    let raw = Box::into_raw(factory) as *mut c_void;
    let hr = fq(raw, iid, out);
    fr(raw);
    hr
}
#[no_mangle]
pub extern "system" fn DllCanUnloadNow() -> i32 {
    if OBJECTS.load(Ordering::Acquire) == 0
        && LOCKS.load(Ordering::Acquire) == 0
        && crate::native::HOST_COUNT.load(Ordering::Acquire) == 0
    {
        0
    } else {
        1
    }
}
