#![allow(improper_ctypes_definitions)]
use crate::macos::common::*;
use crate::rdev::{Event, GrabError};
use cocoa::base::nil;
use cocoa::foundation::NSAutoreleasePool;
use core_graphics::event::{CGEventTapLocation, CGEventType};
use std::os::raw::c_void;
use std::sync::Mutex;

static GLOBAL_CALLBACK: Mutex<Option<Box<dyn FnMut(Event) -> Option<Event> + Send>>> = Mutex::new(None);

unsafe extern "C" fn raw_callback(
    _proxy: CGEventTapProxy,
    _type: CGEventType,
    cg_event: CGEventRef,
    _user_info: *mut c_void,
) -> CGEventRef { unsafe {
    // println!("Event ref {:?}", cg_event_ptr);
    // let cg_event: CGEvent = transmute_copy::<*mut c_void, CGEvent>(&cg_event_ptr);
    if let Ok(mut state) = KEYBOARD_STATE.lock() {
        if let Some(keyboard) = state.as_mut() {
            if let Some(event) = convert(_type, &cg_event, keyboard) {
                if let Some(callback) = GLOBAL_CALLBACK.lock().unwrap().as_mut() {
                    if callback(event).is_none() {
                        cg_event.set_type(CGEventType::Null);
                    }
                }
            }
        }
    }
    cg_event
}}

static mut CUR_LOOP: CFRunLoopSourceRef = std::ptr::null_mut();

#[inline]
pub fn is_grabbed() -> bool {
    unsafe {
        !CUR_LOOP.is_null()
    }
}

pub fn grab<T>(callback: T) -> Result<(), GrabError>
where
    T: FnMut(Event) -> Option<Event> + Send + 'static,
{
    if is_grabbed() {
        return Ok(());
    }

    unsafe {
        {
            let mut cb = GLOBAL_CALLBACK.lock().unwrap();
            *cb = Some(Box::new(callback));
        }
        let _pool = NSAutoreleasePool::new(nil);
        let tap = CGEventTapCreate(
            CGEventTapLocation::Session, // HID, Session, AnnotatedSession,
            kCGHeadInsertEventTap,
            CGEventTapOption::Default,
            kCGEventMaskForAllEvents,
            raw_callback,
            nil,
        );
        if tap.is_null() {
            return Err(GrabError::EventTapError);
        }
        let _loop = CFMachPortCreateRunLoopSource(nil, tap, 0);
        if _loop.is_null() {
            return Err(GrabError::LoopSourceError);
        }

        CUR_LOOP = CFRunLoopGetCurrent() as _;
        CFRunLoopAddSource(CUR_LOOP, _loop, kCFRunLoopCommonModes);

        CGEventTapEnable(tap, true);
        CFRunLoopRun();
    }
    Ok(())
}

pub fn exit_grab() -> Result<(), GrabError> {
    unsafe {
        if !CUR_LOOP.is_null() {
            CFRunLoopStop(CUR_LOOP);
            CUR_LOOP = std::ptr::null_mut();
        }
    }
    Ok(())
}
