use napi::bindgen_prelude::*;
use napi::threadsafe_function::{ThreadsafeFunction, ThreadsafeFunctionCallMode};
use napi_derive::napi;
use std::sync::{atomic::{AtomicBool, Ordering}, Mutex};
use std::ptr;
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM, HINSTANCE};
use windows::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, DispatchMessageW, HHOOK, KBDLLHOOKSTRUCT, MSG,
    PeekMessageW, PM_REMOVE, SetWindowsHookExW, TranslateMessage, UnhookWindowsHookEx,
    WH_KEYBOARD_LL,
    // 🔧 Явно импортируем константы, чтобы избежать проблем с матчингом
    WM_KEYDOWN, WM_KEYUP, WM_SYSKEYDOWN, WM_SYSKEYUP,
};

static CALLBACK: Mutex<Option<ThreadsafeFunction<String>>> = Mutex::new(None);
static RUNNING: AtomicBool = AtomicBool::new(false);
static HOOK_HANDLE: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

extern "system" fn keyboard_hook_proc(n_code: i32, w_param: WPARAM, l_param: LPARAM) -> LRESULT {
    // 🔍 Лог №1: хук вызван
    eprintln!("[HOOK_PROC] Called: n_code={}, w_param={}", n_code, w_param.0);
    
    if n_code < 0 {
        eprintln!("[HOOK_PROC] n_code < 0, passing to next");
        return unsafe { CallNextHookEx(HHOOK(ptr::null_mut()), n_code, w_param, l_param) };
    }

    let kb = unsafe { &*(l_param.0 as *const KBDLLHOOKSTRUCT) };
    let vk = kb.vkCode;
    
    // 🔍 Лог №2: какая клавиша и код сообщения
    let msg_code = w_param.0 as u32;
    eprintln!("[HOOK_PROC] VK={}, msg_code={}", vk, msg_code);
    
    // 🔧 Матчим явно через if, чтобы избежать проблем с импортом констант
    let payload = if msg_code == WM_KEYDOWN || msg_code == WM_SYSKEYDOWN {
        eprintln!("[HOOK_PROC] → Key DOWN");
        Some(format!("down:{}", vk))
    } else if msg_code == WM_KEYUP || msg_code == WM_SYSKEYUP {
        eprintln!("[HOOK_PROC] → Key UP");
        Some(format!("up:{}", vk))
    } else {
        eprintln!("[HOOK_PROC] → Other message, skipping");
        None
    };

    if let Some(msg) = payload {
        eprintln!("[HOOK_PROC] Payload ready: '{}'", msg);
        
        if let Ok(guard) = CALLBACK.lock() {
            eprintln!("[HOOK_PROC] CALLBACK locked");
            if let Some(cb) = guard.as_ref() {
                eprintln!("[HOOK_PROC] Calling JS callback...");
                let status = cb.call(Ok(msg.clone()), ThreadsafeFunctionCallMode::NonBlocking);
                eprintln!("[HOOK_PROC] Callback call status: {:?}", status);
            } else {
                eprintln!("[HOOK_PROC] ❌ CALLBACK is None");
            }
        } else {
            eprintln!("[HOOK_PROC] ❌ Failed to lock CALLBACK");
        }
    }

    let hook_ptr = HOOK_HANDLE.load(Ordering::Acquire) as *mut std::ffi::c_void;
    if !hook_ptr.is_null() {
        unsafe { CallNextHookEx(HHOOK(hook_ptr), n_code, w_param, l_param) }
    } else {
        LRESULT(0)
    }
}

#[napi]
pub fn start_global_keyboard_hook(callback: ThreadsafeFunction<String>) -> Result<()> {
    eprintln!("[HOOK_INIT] === start() called ===");
    
    if RUNNING.load(Ordering::SeqCst) {
        return Err(Error::new(napi::Status::GenericFailure, "Already running".into()));
    }
    
    *CALLBACK.lock().unwrap() = Some(callback);
    RUNNING.store(true, Ordering::SeqCst);

    std::thread::spawn(|| {
        eprintln!("[HOOK_INIT] Thread: installing hook...");
        
        let hook = unsafe {
            SetWindowsHookExW(WH_KEYBOARD_LL, Some(keyboard_hook_proc), HINSTANCE(ptr::null_mut()), 0)
        };

        match hook {
            Ok(h) => {
                eprintln!("[HOOK_INIT] ✓ Hook installed: handle={:?}", h.0);
                HOOK_HANDLE.store(h.0 as usize, Ordering::Release);
            }
            Err(e) => {
                eprintln!("[HOOK_INIT] ❌ Install failed: {}", e);
                RUNNING.store(false, Ordering::SeqCst);
                return;
            }
        }

        // Message loop
        let mut msg = MSG::default();
        while RUNNING.load(Ordering::SeqCst) {
            let has_msg = unsafe { PeekMessageW(&mut msg, HWND(ptr::null_mut()), 0, 0, PM_REMOVE) };
            if has_msg.as_bool() {
                unsafe {
                    TranslateMessage(&msg);
                    DispatchMessageW(&msg);
                }
            } else {
                std::thread::sleep(std::time::Duration::from_millis(2));
            }
        }

        // Cleanup
        let ptr = HOOK_HANDLE.swap(0, Ordering::AcqRel) as *mut std::ffi::c_void;
        if !ptr.is_null() {
            unsafe { let _ = UnhookWindowsHookEx(HHOOK(ptr)); }
            eprintln!("[HOOK_INIT] Hook uninstalled");
        }
        *CALLBACK.lock().unwrap() = None;
        eprintln!("[HOOK_INIT] Thread exited");
    });

    Ok(())
}

#[napi]
pub fn stop_global_keyboard_hook() -> Result<()> {
    eprintln!("[HOOK_STOP] Called");
    RUNNING.store(false, Ordering::SeqCst);
    Ok(())
}