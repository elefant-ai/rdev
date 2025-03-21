use crate::{
    rdev::{Event, ListenError},
    windows::common::{HookError, convert, get_scan_code, set_key_hook, set_mouse_hook},
};
use std::{os::raw::c_int, ptr::null_mut, sync::Mutex, time::SystemTime};
use winapi::{
    shared::{
        basetsd::ULONG_PTR,
        minwindef::{LPARAM, LRESULT, WPARAM},
    },
    um::winuser::{CallNextHookEx, GetMessageA, HC_ACTION, PKBDLLHOOKSTRUCT, PMOUSEHOOKSTRUCT},
};

static GLOBAL_CALLBACK: Mutex<Option<Box<dyn FnMut(Event) + Send>>> = Mutex::new(None);

impl From<HookError> for ListenError {
    fn from(error: HookError) -> Self {
        match error {
            HookError::Mouse(code) => ListenError::MouseHookError(code),
            HookError::Key(code) => ListenError::KeyHookError(code),
        }
    }
}

unsafe fn raw_callback(
    code: c_int,
    param: WPARAM,
    lpdata: LPARAM,
    f_get_extra_data: impl FnOnce(isize) -> ULONG_PTR,
) -> LRESULT {
    if code == HC_ACTION {
        let (opt, code) = unsafe { convert(param, lpdata) };
        if let Some(event_type) = opt {
            let event = Event {
                event_type,
                time: SystemTime::now(),
                unicode: None,
                platform_code: code as _,
                position_code: unsafe { get_scan_code(lpdata) },
                usb_hid: 0,
                extra_data: f_get_extra_data(lpdata),
            };
            if let Some(callback) = GLOBAL_CALLBACK.lock().unwrap().as_mut() {
                callback(event);
            }
        }
    }
    unsafe { CallNextHookEx(null_mut(), code, param, lpdata) }
}

unsafe extern "system" fn raw_callback_mouse(code: i32, param: usize, lpdata: isize) -> isize {
    unsafe {
        raw_callback(code, param, lpdata, |data: isize| {
            (*(data as PMOUSEHOOKSTRUCT)).dwExtraInfo
        })
    }
}

unsafe extern "system" fn raw_callback_keyboard(code: i32, param: usize, lpdata: isize) -> isize {
    unsafe {
        raw_callback(code, param, lpdata, |data: isize| {
            (*(data as PKBDLLHOOKSTRUCT)).dwExtraInfo
        })
    }
}

pub fn listen<T>(callback: T) -> Result<(), ListenError>
where
    T: FnMut(Event) + Send + 'static,
{
    {
        let mut cb = GLOBAL_CALLBACK.lock().unwrap();
        *cb = Some(Box::new(callback));
    }
    unsafe {
        set_key_hook(raw_callback_keyboard)?;
        if !crate::keyboard_only() {
            set_mouse_hook(raw_callback_mouse)?;
        }

        GetMessageA(null_mut(), null_mut(), 0, 0);
    }
    Ok(())
}
