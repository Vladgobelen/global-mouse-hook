use napi::bindgen_prelude::*;
use napi::threadsafe_function::{ThreadsafeFunction, ThreadsafeFunctionCallMode};
use napi_derive::napi;
use std::sync::{atomic::{AtomicBool, Ordering}, Mutex};
use windows::Win32::Foundation::*;
use windows::Win32::UI::WindowsAndMessaging::*;

// Глобальные переменные
static CALLBACK: Mutex<Option<ThreadsafeFunction<String>>> = Mutex::new(None);
static RUNNING: AtomicBool = AtomicBool::new(false);
static HOOK: Mutex<Option<HHOOK>> = Mutex::new(None);

extern "system" fn keyboard_hook_proc(n_code: i32, w_param: WPARAM, l_param: LPARAM) -> LRESULT {
    if n_code < 0 {
        return unsafe { CallNextHookEx(HHOOK(0), n_code, w_param, l_param) };
    }

    let kb = unsafe { &*(l_param.0 as *const KBDLLHOOKSTRUCT) };
    let payload = match w_param.0 as u32 {
        WM_KEYDOWN | WM_SYSKEYDOWN => Some(format!("down:{}", kb.vkCode)),
        WM_KEYUP | WM_SYSKEYUP => Some(format!("up:{}", kb.vkCode)),
        _ => None,
    };

    if let Some(msg) = payload {
        if let Ok(guard) = CALLBACK.lock() {
            if let Some(cb) = guard.as_ref() {
                let _ = cb.call(Ok(msg), ThreadsafeFunctionCallMode::NonBlocking);
            }
        }
    }

    let hook_guard = HOOK.lock().unwrap();
    if let Some(hook) = *hook_guard {
        unsafe { CallNextHookEx(hook, n_code, w_param, l_param) }
    } else {
        LRESULT(0)
    }
}

#[napi]
pub fn start_global_keyboard_hook(callback: ThreadsafeFunction<String>) -> Result<()> {
    if RUNNING.load(Ordering::SeqCst) {
        return Err(Error::new(Status::GenericFailure, "Hook is already running".to_owned()));
    }
    
    *CALLBACK.lock().unwrap() = Some(callback);
    RUNNING.store(true, Ordering::SeqCst);

    std::thread::spawn(|| {
        let hook_result = unsafe {
            SetWindowsHookExW(WH_KEYBOARD_LL, Some(keyboard_hook_proc), HINSTANCE(0), 0)
        };

        let hook = match hook_result {
            Ok(h) => h,
            Err(e) => {
                eprintln!("[hook] Failed to set WH_KEYBOARD_LL: {}", e);
                RUNNING.store(false, Ordering::SeqCst);
                return;
            }
        };

        *HOOK.lock().unwrap() = Some(hook);

        let mut msg = MSG::default();
        while RUNNING.load(Ordering::SeqCst) {
            let has_msg = unsafe { PeekMessageW(&mut msg, HWND(0), 0, 0, PM_REMOVE) };
            if has_msg.as_bool() {
                unsafe {
                    TranslateMessage(&msg);
                    DispatchMessageW(&msg);
                }
            } else {
                std::thread::sleep(std::time::Duration::from_millis(5));
            }
        }

        if let Some(h) = HOOK.lock().unwrap().take() {
            unsafe { let _ = UnhookWindowsHookEx(h); }
        }
        *CALLBACK.lock().unwrap() = None;
        eprintln!("[hook] Keyboard hook thread exited cleanly");
    });

    Ok(())
}

#[napi]
pub fn stop_global_keyboard_hook() -> Result<()> {
    if !RUNNING.load(Ordering::SeqCst) {
        return Ok(());
    }
    RUNNING.store(false, Ordering::SeqCst);
    Ok(())
}