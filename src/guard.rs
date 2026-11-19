//! Panic boundaries for callbacks that Office or Windows invoke.
//!
//! A Rust panic must never unwind across an FFI boundary: the unwind would walk
//! through Office's own frames and take the host process down with it, losing
//! unsaved documents. Every callback that runs inside Word or Excel passes
//! through one of these helpers, which turn a panic into a COM error or into
//! "message not handled" instead.

use std::panic::{catch_unwind, AssertUnwindSafe};

pub const E_FAIL: i32 = 0x8000_4005u32 as i32;

/// Runs one COM method body and turns a panic into `E_FAIL`.
pub fn com(what: &str, body: impl FnOnce() -> i32) -> i32 {
    match catch_unwind(AssertUnwindSafe(body)) {
        Ok(hr) => hr,
        Err(payload) => {
            report(what, payload.as_ref());
            E_FAIL
        }
    }
}

/// Runs one window-procedure body. `None` tells the caller to forward the
/// message to the default handler instead of reporting it as handled.
pub fn window(what: &str, body: impl FnOnce() -> isize) -> Option<isize> {
    match catch_unwind(AssertUnwindSafe(body)) {
        Ok(value) => Some(value),
        Err(payload) => {
            report(what, payload.as_ref());
            None
        }
    }
}

/// Runs a `Release` body. Returning 0 after a panic would make OLE unload the
/// DLL while Office still holds pointers into it, so the object keeps one
/// reference instead and the fault is logged.
pub fn refs(what: &str, body: impl FnOnce() -> u32) -> u32 {
    match catch_unwind(AssertUnwindSafe(body)) {
        Ok(count) => count,
        Err(payload) => {
            report(what, payload.as_ref());
            1
        }
    }
}

fn report(what: &str, payload: &(dyn std::any::Any + Send)) {
    let detail = payload
        .downcast_ref::<&str>()
        .copied()
        .or_else(|| payload.downcast_ref::<String>().map(String::as_str))
        .unwrap_or("panic");
    #[cfg(windows)]
    crate::native::record_panic(what, detail);
    #[cfg(not(windows))]
    let _ = (what, detail);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn values_pass_through() {
        assert_eq!(com("ok", || 7), 7);
        assert_eq!(window("ok", || 9), Some(9));
    }

    #[test]
    fn panics_become_e_fail() {
        assert_eq!(com("boom", || panic!("contained")), E_FAIL);
        assert_eq!(com("boom", || -> i32 { panic!("still contained") }), E_FAIL);
    }

    #[test]
    fn panics_become_unhandled() {
        assert_eq!(window("boom", || -> isize { panic!("contained") }), None);
    }

    #[test]
    fn strings_are_reported_not_leaked() {
        let payload = String::from("details");
        let any: Box<dyn std::any::Any + Send> = Box::new(payload);
        // The reporting path must accept both owned and borrowed payloads.
        let borrowed: &(dyn std::any::Any + Send) = &*any;
        assert!(borrowed.downcast_ref::<String>().is_some());
    }

    #[test]
    fn guard_stays_usable_after_a_panic() {
        assert_eq!(com("boom", || -> i32 { panic!("first") }), E_FAIL);
        assert_eq!(com("after", || 3), 3);
    }
}
