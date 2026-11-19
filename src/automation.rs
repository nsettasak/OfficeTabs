use std::{ffi::c_void, ptr};
use windows_sys::{
    core::GUID,
    Win32::{
        Foundation::*,
        System::{Com::*, Variant::*},
    },
};
pub type Raw = *mut c_void;
pub const IID_NULL: GUID = GUID::from_u128(0);
pub const IID_UNKNOWN: GUID = GUID::from_u128(0x00000000_0000_0000_c000_000000000046);
pub const IID_DISPATCH: GUID = GUID::from_u128(0x00020400_0000_0000_c000_000000000046);
#[repr(C)]
pub struct DispatchVtable {
    pub query: unsafe extern "system" fn(Raw, *const GUID, *mut Raw) -> i32,
    pub add: unsafe extern "system" fn(Raw) -> u32,
    pub release: unsafe extern "system" fn(Raw) -> u32,
    pub type_count: unsafe extern "system" fn(Raw, *mut u32) -> i32,
    pub type_info: unsafe extern "system" fn(Raw, u32, u32, *mut Raw) -> i32,
    pub ids:
        unsafe extern "system" fn(Raw, *const GUID, *const *const u16, u32, u32, *mut i32) -> i32,
    pub invoke: unsafe extern "system" fn(
        Raw,
        i32,
        *const GUID,
        u32,
        u16,
        *mut DISPPARAMS,
        *mut VARIANT,
        *mut EXCEPINFO,
        *mut u32,
    ) -> i32,
}
/// # Safety
/// `p` must be a live COM interface pointer whose vtable begins with `DispatchVtable`.
pub unsafe fn table(p: Raw) -> &'static DispatchVtable {
    &**(p as *const *const DispatchVtable)
}
pub struct Dispatch(pub Raw);
impl Clone for Dispatch {
    fn clone(&self) -> Self {
        unsafe {
            (table(self.0).add)(self.0);
        }
        Self(self.0)
    }
}
impl Drop for Dispatch {
    fn drop(&mut self) {
        unsafe {
            (table(self.0).release)(self.0);
        }
    }
}
pub struct Value(pub VARIANT);
impl Drop for Value {
    fn drop(&mut self) {
        unsafe {
            VariantClear(&mut self.0);
        }
    }
}
impl Value {
    pub fn int(v: i32) -> Self {
        let mut value = VARIANT::default();
        value.Anonymous.Anonymous.vt = VT_I4;
        value.Anonymous.Anonymous.Anonymous.lVal = v;
        Self(value)
    }
    pub fn missing() -> Self {
        let mut value = VARIANT::default();
        value.Anonymous.Anonymous.vt = VT_ERROR;
        value.Anonymous.Anonymous.Anonymous.scode = 0x80020004u32 as i32;
        Self(value)
    }
    pub fn number(&self) -> i32 {
        unsafe {
            match self.0.Anonymous.Anonymous.vt {
                VT_I4 | VT_INT => self.0.Anonymous.Anonymous.Anonymous.lVal,
                VT_I2 => self.0.Anonymous.Anonymous.Anonymous.iVal as i32,
                _ => 0,
            }
        }
    }
    pub fn boolean(&self) -> bool {
        unsafe {
            self.0.Anonymous.Anonymous.vt == VT_BOOL
                && self.0.Anonymous.Anonymous.Anonymous.boolVal != 0
        }
    }
    pub fn string(&self) -> String {
        unsafe {
            if self.0.Anonymous.Anonymous.vt != VT_BSTR {
                return String::new();
            }
            let p = self.0.Anonymous.Anonymous.Anonymous.bstrVal;
            if p.is_null() {
                return String::new();
            }
            String::from_utf16_lossy(std::slice::from_raw_parts(p, SysStringLen(p) as usize))
        }
    }
    pub fn dispatch(&self) -> Result<Dispatch, i32> {
        unsafe {
            if self.0.Anonymous.Anonymous.vt != VT_DISPATCH {
                return Err(0x80020005u32 as i32);
            }
            let p = self.0.Anonymous.Anonymous.Anonymous.pdispVal;
            if p.is_null() {
                return Err(0x80004003u32 as i32);
            }
            (table(p).add)(p);
            Ok(Dispatch(p))
        }
    }
}
impl Dispatch {
    /// # Safety
    /// `p` must be a live `IDispatch`; the returned guard releases it on drop.
    pub unsafe fn borrow(p: Raw) -> Option<Self> {
        if p.is_null() {
            None
        } else {
            (table(p).add)(p);
            Some(Self(p))
        }
    }
    pub fn identity(&self) -> usize {
        unsafe {
            let mut value = ptr::null_mut();
            if (table(self.0).query)(self.0, &IID_UNKNOWN, &mut value) < 0 {
                return self.0 as usize;
            }
            let id = value as usize;
            (table(value).release)(value);
            id
        }
    }
    pub fn call(&self, name: &str, flags: u16, args: Vec<Value>) -> Result<Value, i32> {
        unsafe {
            let wide: Vec<u16> = name.encode_utf16().chain(Some(0)).collect();
            let p = wide.as_ptr();
            let mut id = 0;
            let hr = (table(self.0).ids)(self.0, &IID_NULL, &p, 1, 0x400, &mut id);
            if hr < 0 {
                return Err(hr);
            }
            let mut values: Vec<VARIANT> = args.iter().rev().map(|v| v.0).collect();
            let mut named = -3;
            let mut params = DISPPARAMS {
                rgvarg: values.as_mut_ptr(),
                rgdispidNamedArgs: if flags & 4 != 0 {
                    &mut named
                } else {
                    ptr::null_mut()
                },
                cArgs: values.len() as u32,
                cNamedArgs: if flags & 4 != 0 { 1 } else { 0 },
            };
            let mut result = VARIANT::default();
            let mut exception = EXCEPINFO::default();
            let mut argument = 0;
            let hr = (table(self.0).invoke)(
                self.0,
                id,
                &IID_NULL,
                0x400,
                flags,
                &mut params,
                &mut result,
                &mut exception,
                &mut argument,
            );
            if let Some(fill) = exception.pfnDeferredFillIn {
                fill(&mut exception);
            }
            for p in [
                exception.bstrSource,
                exception.bstrDescription,
                exception.bstrHelpFile,
            ] {
                if !p.is_null() {
                    SysFreeString(p);
                }
            }
            if hr < 0 {
                VariantClear(&mut result);
                Err(hr)
            } else {
                Ok(Value(result))
            }
        }
    }
    pub fn get(&self, name: &str) -> Result<Value, i32> {
        self.call(name, 2, vec![])
    }
    pub fn object(&self, name: &str) -> Result<Dispatch, i32> {
        self.get(name)?.dispatch()
    }
    pub fn item(&self, index: i32) -> Result<Dispatch, i32> {
        self.call("Item", 3, vec![Value::int(index)])?.dispatch()
    }
    pub fn invoke(&self, name: &str) -> Result<(), i32> {
        self.call(name, 1, vec![]).map(|_| ())
    }
}
