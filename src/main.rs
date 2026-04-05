use serde::Serialize;
use std::io::{self, BufRead, Write};
use std::sync::{atomic::{AtomicBool, Ordering}, Mutex};
use std::thread;
use std::time::Duration;

#[cfg(windows)]
use windows::Win32::System::Console::FreeConsole;
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM, HINSTANCE};
use windows::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, DispatchMessageW, HHOOK, KBDLLHOOKSTRUCT, MSG,
    PeekMessageW, PM_REMOVE, SetWindowsHookExW, TranslateMessage, UnhookWindowsHookEx,
    WH_KEYBOARD_LL, WM_KEYDOWN, WM_KEYUP, WM_SYSKEYDOWN, WM_SYSKEYUP,
};

#[derive(Serialize)]
struct KeyEvent {
    r#type: &'static str,
    event: &'static str,
    code: u32,
}

fn send_event(event_type: &'static str, vk_code: u32) {
    // 🔧 Формируем JSON вручную — надёжнее и быстрее
    let json = format!(r#"{{"type":"{}","event":"{}","code":{}}}"#, 
        event_type, 
        if event_type == "key" { "down" } else { "up" },
        vk_code
    );
    
    let stdout = io::stdout();
    let mut handle = stdout.lock();
    let _ = writeln!(handle, "{}", json);
    let _ = handle.flush();
}

static RUNNING: AtomicBool = AtomicBool::new(false);
static HOOK_HANDLE: Mutex<Option<HHOOK>> = Mutex::new(None);

extern "system" fn keyboard_hook_proc(n_code: i32, w_param: WPARAM, l_param: LPARAM) -> LRESULT {
    if n_code < 0 {
        // 🔧 В windows@0.52: HHOOK(0)
        return unsafe { CallNextHookEx(HHOOK(0), n_code, w_param, l_param) };
    }

    let kb = unsafe { &*(l_param.0 as *const KBDLLHOOKSTRUCT) };
    let vk = kb.vkCode;
    
    match w_param.0 as u32 {
        code if code == WM_KEYDOWN || code == WM_SYSKEYDOWN => {
            send_event("key", vk);
        }
        _ => {}
    }

    let hook_guard = HOOK_HANDLE.lock().unwrap();
    if let Some(hook) = *hook_guard {
        unsafe { CallNextHookEx(hook, n_code, w_param, l_param) }
    } else {
        LRESULT(0)
    }
}

fn install_hook() -> Result<HHOOK, String> {
    unsafe {
        SetWindowsHookExW(WH_KEYBOARD_LL, Some(keyboard_hook_proc), HINSTANCE(0), 0)
    }.map_err(|e| format!("SetWindowsHookExW failed: {}", e))
}

fn message_loop() {
    let mut msg = MSG::default();
    while RUNNING.load(Ordering::SeqCst) {
        let has_msg = unsafe { PeekMessageW(&mut msg, HWND(0), 0, 0, PM_REMOVE) };
        if has_msg.as_bool() {
            unsafe {
                TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        } else {
            thread::sleep(Duration::from_millis(2));
        }
    }
}

fn cleanup_hook() {
    if let Ok(mut guard) = HOOK_HANDLE.lock() {
        if let Some(hook) = guard.take() {
            unsafe { let _ = UnhookWindowsHookEx(hook); }
        }
    }
}

fn main() {
    // 🔧 Скрываем консоль на Windows (опционально)
    #[cfg(windows)]
    unsafe { let _ = FreeConsole(); }

    // Сигнал готовности
    let _ = writeln!(io::stdout(), "{{\"type\":\"init\",\"status\":\"ready\"}}");
    let _ = io::stdout().flush();

    RUNNING.store(true, Ordering::SeqCst);

    // Установка хука
    let hook = match install_hook() {
        Ok(h) => {
            *HOOK_HANDLE.lock().unwrap() = Some(h);
            h
        }
        Err(e) => {
            let _ = writeln!(io::stderr(), "{{\"type\":\"error\",\"message\":\"{}\"}}", e);
            let _ = io::stderr().flush();
            return;
        }
    };

    // Чтение команд с stdin (например, "stop")
    let stdin_handle = thread::spawn(|| {
        let stdin = io::stdin();
        for line in stdin.lock().lines().flatten() {
            if line.trim().eq_ignore_ascii_case("stop") {
                RUNNING.store(false, Ordering::SeqCst);
                break;
            }
        }
    });

    message_loop();
    let _ = stdin_handle.join();
    cleanup_hook();
    
    let _ = writeln!(io::stdout(), "{{\"type\":\"exit\",\"status\":\"clean\"}}");
    let _ = io::stdout().flush();
}