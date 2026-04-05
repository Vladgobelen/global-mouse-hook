use napi::bindgen_prelude::*;
use napi::threadsafe_function::{ThreadsafeFunction, ThreadsafeFunctionCallMode};
use napi_derive::napi;
use std::sync::{atomic::{AtomicBool, Ordering}, Mutex};
use std::ptr;
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM, HINSTANCE};
use windows::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, DispatchMessageW, HHOOK, KBDLLHOOKSTRUCT, MSG,
    PeekMessageW, PM_REMOVE, SetWindowsHookExW, TranslateMessage, UnhookWindowsHookEx,
    WH_KEYBOARD_LL, WM_KEYDOWN, WM_KEYUP, WM_SYSKEYDOWN, WM_SYSKEYUP,
};

static CALLBACK: Mutex<Option<ThreadsafeFunction<String>>> = Mutex::new(None);
static RUNNING: AtomicBool = AtomicBool::new(false);
static HOOK_HANDLE: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

extern "system" fn keyboard_hook_proc(n_code: i32, w_param: WPARAM, l_param: LPARAM) -> LRESULT {
    if n_code < 0 {
        return unsafe { CallNextHookEx(HHOOK(ptr::null_mut()), n_code, w_param, l_param) };
    }

    let kb = unsafe { &*(l_param.0 as *const KBDLLHOOKSTRUCT) };
    
    let payload = match w_param.0 as u32 {
        code if code == WM_KEYDOWN || code == WM_SYSKEYDOWN => {
            Some(format!("down:{}", kb.vkCode))
        }
        code if code == WM_KEYUP || code == WM_SYSKEYUP => {
            Some(format!("up:{}", kb.vkCode))
        }
        _ => None,
    };

    if let Some(msg) = payload {
        eprintln!("[RUST_HOOK_RAW] Captured: {}", msg);
        
        if let Ok(guard) = CALLBACK.lock() {
            if let Some(cb) = guard.as_ref() {
                eprintln!("[RUST_HOOK] Calling JS callback with: '{}'", msg);
                // 🔧 Ключевое исправление: передаём строку напрямую, napi сам конвертирует
                let status = cb.call(msg, ThreadsafeFunctionCallMode::NonBlocking);
                match status {
                    napi::Status::Ok => eprintln!("[RUST_HOOK] Callback call OK"),
                    e => eprintln!("[RUST_HOOK_ERR] Callback failed with status: {:?}", e),
                }
            }
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
    eprintln!("[RUST_HOOK] === start_global_keyboard_hook() called ===");
    
    if RUNNING.load(Ordering::SeqCst) {
        eprintln!("[RUST_HOOK_ERR] Hook is already running");
        return Err(Error::new(napi::Status::GenericFailure, "Hook is already running".to_owned()));
    }
    
    *CALLBACK.lock().unwrap() = Some(callback);
    RUNNING.store(true, Ordering::SeqCst);
    eprintln!("[RUST_HOOK] Callback registered, RUNNING=true");

    std::thread::spawn(|| {
        eprintln!("[RUST_HOOK] Thread spawned, installing WH_KEYBOARD_LL...");
        
        let hook_result = unsafe {
            SetWindowsHookExW(WH_KEYBOARD_LL, Some(keyboard_hook_proc), HINSTANCE(ptr::null_mut()), 0)
        };

        let hook = match hook_result {
            Ok(h) => {
                eprintln!("[RUST_HOOK] ✓ Hook installed OK (handle={:?})", h.0);
                h
            }
            Err(e) => {
                eprintln!("[RUST_HOOK] ✗ FAILED to install hook: {}", e);
                RUNNING.store(false, Ordering::SeqCst);
                return;
            }
        };

        HOOK_HANDLE.store(hook.0 as usize, Ordering::Release);

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

        let hook_ptr = HOOK_HANDLE.swap(0, Ordering::AcqRel) as *mut std::ffi::c_void;
        if !hook_ptr.is_null() {
            unsafe { let _ = UnhookWindowsHookEx(HHOOK(hook_ptr)); }
            eprintln!("[RUST_HOOK] Hook uninstalled");
        }
        *CALLBACK.lock().unwrap() = None;
        eprintln!("[RUST_HOOK] Thread exited cleanly");
    });

    eprintln!("[RUST_HOOK] === start_global_keyboard_hook() returning Ok ===");
    Ok(())
}

#[napi]
pub fn stop_global_keyboard_hook() -> Result<()> {
    eprintln!("[RUST_HOOK] === stop_global_keyboard_hook() called ===");
    
    if !RUNNING.load(Ordering::SeqCst) {
        eprintln!("[RUST_HOOK] Hook not running, nothing to stop");
        return Ok(());
    }
    
    RUNNING.store(false, Ordering::SeqCst);
    eprintln!("[RUST_HOOK] RUNNING=false, thread will exit soon");
    Ok(())
}